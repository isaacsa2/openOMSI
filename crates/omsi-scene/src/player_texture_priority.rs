//! The driven vehicle's texture priority, separate from the AI fleet texture budget.
use super::*;

/// Replace the protected paths when the player changes vehicles, or extend them when
/// adding a coupled section. This does not alter any AI vehicles' texture ownership.
pub(super) fn protect_vehicle_textures(
    pinned: &mut hashbrown::HashSet<PathBuf>,
    held: &[PathBuf],
    replace: bool,
) {
    if replace { pinned.clear(); }
    pinned.extend(held.iter().cloned());
}

impl World {
    /// An AI bus taken over by the player may already have lost its largest mip levels.
    /// Ask the existing worker upload pipeline to restore its pictures without changing
    /// the texture ID which the materials refer to.
    pub(super) fn schedule_player_texture_restores(&self, gpu: &mut GpuCache) {
        let protected = self.player_texture_paths.lock();
        let reduced = self.reduced_vehicle_paths.lock();
        let pending = self.upgrades_pending.lock();
        for path in protected.iter() {
            if reduced.contains(path) && !pending.contains(path) && !gpu.wants_restore.contains(path) {
                gpu.wants_restore.push(path.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coupled_sections_are_protected_and_changing_buses_resets_protection() {
        let mut pinned = hashbrown::HashSet::new();
        let main = PathBuf::from("player/body.dds");
        let rear = PathBuf::from("player/rear.dds");
        protect_vehicle_textures(&mut pinned, std::slice::from_ref(&main), true);
        protect_vehicle_textures(&mut pinned, &[main.clone(), rear.clone()], false);
        assert_eq!(pinned.len(), 2);
        assert!(pinned.contains(&main) && pinned.contains(&rear));
        let next = PathBuf::from("next/body.dds");
        protect_vehicle_textures(&mut pinned, std::slice::from_ref(&next), true);
        assert_eq!(pinned.len(), 1);
        assert!(pinned.contains(&next));
        assert!(!pinned.contains(&main));
    }
}
