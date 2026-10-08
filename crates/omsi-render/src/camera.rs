//! The camera a picture is drawn from: its place, direction and lens, and the view and
//! projection made from them.

use glam::{DVec3, Mat4, Vec3};

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// World position (f64: maps span millions of metres).
    pub position: DVec3,
    /// Degrees, 0 = looking north (+y), clockwise positive (OMSI heading).
    pub yaw: f32,
    /// Degrees, positive = looking up.
    pub pitch: f32,
    /// Degrees about the view direction (0 = the horizon level; see [`Camera::up`]). A
    /// camera fixed to a vehicle - a mirror's - leans with its body.
    pub roll: f32,
    pub fov_deg: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        Vec3::new(sy * cp, cy * cp, sp)
    }
    pub fn right(&self) -> Vec3 {
        let f = self.forward();
        let r0 = Vec3::new(f.y, -f.x, 0.0).normalize_or_zero();
        if self.roll == 0.0 {
            return r0;
        }
        f.cross(self.up()).normalize_or(r0)
    }
    /// The picture's up: world up for a level camera, turned about the view direction by
    /// `roll` (positive: the top leans to the right).
    pub fn up(&self) -> Vec3 {
        let f = self.forward();
        let r0 = Vec3::new(f.y, -f.x, 0.0).normalize_or_zero();
        if self.roll == 0.0 || r0 == Vec3::ZERO {
            return Vec3::Z;
        }
        let u0 = r0.cross(f);
        let (s, c) = self.roll.to_radians().sin_cos();
        (u0 * c + r0 * s).normalize_or(Vec3::Z)
    }
    /// View-projection relative to a render origin (the camera itself when `origin` is its
    /// position), so that GPU maths stays in small numbers.
    pub fn view_proj(&self, aspect: f32, origin: DVec3) -> Mat4 {
        let view = Mat4::look_to_rh((self.position - origin).as_vec3(), self.forward(), self.up());
        // Reversed Z (near and far swapped): the depth buffer then spends its float
        // precision where the scene is far away instead of where it is close, which is what
        // stops distant roads, kerbs and painted ground from flickering against each other
        // - with a plain 0..1 depth the resolution at a kilometre is a good quarter of a
        // metre, less than the gap between a road surface and the ground under it.
        let proj = Mat4::perspective_rh(self.fov_deg.to_radians(), aspect, self.far, self.near);
        proj * view
    }

    /// Ray through a normalized device coordinate, relative to `origin`.
    pub fn ray(&self, ndc_x: f32, ndc_y: f32, aspect: f32, origin: DVec3) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect, origin).inverse();
        // ndc z = 0 is the far plane with reversed Z: the longest baseline for the ray
        let p = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
        let o = (self.position - origin).as_vec3();
        (o, (p - o).normalize_or_zero())
    }
}
