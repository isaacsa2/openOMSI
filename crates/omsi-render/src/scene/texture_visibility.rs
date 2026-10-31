//! Material textures drawn by the main camera in its previous culling pass.
//! A priority hint for mip reduction, not a request to evict GPU resources.
use super::*;
use std::collections::HashSet;

fn for_each_drawn(bitmap: &[u64], mut visit: impl FnMut(usize)) {
    for (word_index, &word) in bitmap.iter().enumerate() {
        let mut remaining = word;
        while remaining != 0 {
            let bit = remaining.trailing_zeros() as usize;
            visit(word_index * 64 + bit);
            remaining &= remaining - 1;
        }
    }
}

impl Renderer {
    /// Texture IDs referenced by materials visible in the last main-camera frame.
    /// Includes all material channels, conservatively protecting inactive night
    /// and reflection maps too. Mirrors/shadows have their own culling views.
    pub fn last_main_view_texture_ids(&self, scene: &Scene) -> HashSet<TextureId> {
        let mut ids = HashSet::new();
        let drawn = self.history.cull_drawn.borrow();
        for_each_drawn(&drawn, |i| {
            let Some(inst) = scene.instances.get(i) else { return };
            for &material in &inst.materials {
                let Some(m) = scene.materials.get(material) else { continue };
                let maps = [
                    m.texture, m.nightmap, m.lightmap,
                    m.envmap.map(|(id, _)| id), m.env_mask,
                    m.bump.map(|(id, _)| id), m.transmap.map(|(id, _)| id),
                ];
                ids.extend(maps.into_iter().flatten());
            }
        });
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::for_each_drawn;

    #[test]
    fn main_view_bits_cover_several_words_and_sparse_instances() {
        let mut found = Vec::new();
        for_each_drawn(&[1 | (1 << 63), 1 << 1, 0, 1 << 7], |i| found.push(i));
        assert_eq!(found, vec![0, 63, 65, 199]);
    }

    #[test]
    fn empty_history_never_marks_a_texture_visible() {
        let mut found = Vec::new();
        for_each_drawn(&[], |i| found.push(i));
        assert!(found.is_empty());
    }
}
