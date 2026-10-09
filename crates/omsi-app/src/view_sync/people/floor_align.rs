//! Standing-only procedural mesh grounding.
//!
//! Some OMSI human meshes have shoe vertices offset below the sole estimated from
//! the rig's [links]. The procedural IK places its virtual foot at the floor, but
//! vertices skinned to that foot may still end up below the floor. Align the
//! *actual shoe vertices* after skinning rather than changing map/cabin heights.

use super::*;

fn shoe_min(rest: &[Vec3], skinned: &[Vec3], sole: f32, ankle_y: f32, scale: f32) -> Option<(f32, usize)> {
    let mut lowest = f32::INFINITY;
    let mut count = 0;
    for (before, after) in rest.iter().zip(skinned) {
        // Only shoe-level geometry close to the original ankles. Clothing, hair,
        // bags and benches must never be used to select a grounding offset.
        if before.z < sole - 0.05 * scale
            || before.z > sole + 0.08 * scale
            || (before.y - ankle_y).abs() > 0.42 * scale
            || before.x.abs() > 0.45 * scale
            || !after.is_finite()
        {
            continue;
        }
        lowest = lowest.min(after.z);
        count += 1;
    }
    (count > 0).then_some((lowest, count))
}

fn lift_from_sole(min_z: f32, count: usize, max_lift: f32) -> f32 {
    // No arbitrary global human height adjustment, and no calibration when
    // the model has too few actual shoe samples.
    if count < 4 || !min_z.is_finite() || min_z >= -0.025 {
        return 0.0;
    }
    (-min_z).min(max_lift).max(0.0)
}

/// Called only after a freshly skinned *standing* procedural pose. This never
/// touches seated poses, avatars, the legacy animation or the simulation state.
pub(super) fn align_standing(ty: &HumanType, skins: &mut [(Vec<Vec3>, Vec<Vec3>)]) {
    let rig = &ty.rig;
    let (mut min_z, mut count) = (f32::INFINITY, 0);
    for (mesh, (positions, _)) in ty.meshes.iter().zip(skins.iter()) {
        if let Some((z, n)) = shoe_min(
            &mesh.data.positions, positions, rig.sole, rig.ankle[1].y, rig.scale,
        ) {
            min_z = min_z.min(z);
            count += n;
        }
    }
    let lift = lift_from_sole(min_z, count, rig.leg());
    if lift <= 0.0 {
        return;
    }
    for (positions, _) in skins.iter_mut() {
        for v in positions {
            v.z += lift;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounded_shoes_do_not_move() {
        assert_eq!(lift_from_sole(-0.01, 8, 1.0), 0.0);
        assert_eq!(lift_from_sole(0.02, 8, 1.0), 0.0);
        assert_eq!(lift_from_sole(-0.3, 2, 1.0), 0.0);
    }

    #[test]
    fn submerged_shoes_are_lifted_by_measured_amount() {
        assert!((lift_from_sole(-0.37, 8, 1.0) - 0.37).abs() < 1e-6);
        assert_eq!(lift_from_sole(-1.5, 8, 0.85), 0.85);
    }

    #[test]
    fn only_original_shoe_region_is_sampled() {
        let rest = vec![
            Vec3::new(0.1, 0.0, 0.01),
            Vec3::new(-0.1, 0.0, 0.02),
            Vec3::new(0.1, 0.1, -0.01),
            Vec3::new(-0.1, 0.1, 0.0),
            Vec3::new(0.0, 0.0, 1.5),
        ];
        let posed = vec![
            Vec3::new(0.1, 0.0, -0.25),
            Vec3::new(-0.1, 0.0, -0.24),
            Vec3::new(0.1, 0.1, -0.26),
            Vec3::new(-0.1, 0.1, -0.23),
            Vec3::new(0.0, 0.0, -2.0),
        ];
        let (lowest, count) = shoe_min(&rest, &posed, 0.0, 0.0, 1.0).unwrap();
        assert_eq!(count, 4);
        assert!((lowest + 0.26).abs() < 1e-6);
    }
}
