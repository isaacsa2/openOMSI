//! Share identical source material/mapping pairs while preserving per-file names.
use crate::{
    optimise::material_key,
    tags::Mode,
    three_ds::{Object, Scene, UvTransform},
};
use anyhow::{ensure, Result};
use omsi_o3d::Material;
use std::collections::BTreeMap;

type Key = (([u32; 11], String), [u32; 5], Mode);
#[derive(Default)]
pub(crate) struct Registry {
    pub materials: Vec<Material>,
    pub mappings: Vec<UvTransform>,
    pub modes: Vec<Mode>,
    seen: BTreeMap<Key, u16>,
}

impl Registry {
    pub fn into_materials(self) -> (Vec<Material>, Vec<Mode>) {
        // Drop the source-name and UV lookups before assembling the dense output tiles.
        (self.materials, self.modes)
    }
    fn add(&mut self, material: Material, uv: UvTransform, mode: Mode) -> Result<u16> {
        let key = (material_key(&material), uv.key(), mode);
        if let Some(&i) = self.seen.get(&key) {
            return Ok(i);
        }
        ensure!(
            self.materials.len() + 1 < u16::MAX as usize,
            "too many distinct map materials"
        );
        let i = self.materials.len() as u16;
        self.materials.push(material);
        self.mappings.push(uv);
        self.modes.push(mode);
        self.seen.insert(key, i);
        Ok(i)
    }
    pub fn object_faces(
        &mut self,
        object: &Object,
        names: &BTreeMap<String, u16>,
        default: u16,
        mode: Mode,
    ) -> Result<Vec<u16>> {
        let mut faces = faces(object, names, default)?;
        if mode != Mode::Auto {
            let mut variants = BTreeMap::new();
            for face in &mut faces {
                if let Some(&variant) = variants.get(face) {
                    *face = variant;
                    continue;
                }
                let source = *face as usize;
                // Invisible colliders have no texture or shading to resolve/upload.
                let material = if mode == Mode::Collision {
                    Material::default()
                } else {
                    self.materials[source].clone()
                };
                let variant = self.add(material, self.mappings[source], mode)?;
                variants.insert(*face, variant);
                *face = variant;
            }
        }
        Ok(faces)
    }
    pub fn register(&mut self, scene: &Scene) -> Result<(BTreeMap<String, u16>, u16)> {
        let mut names = BTreeMap::new();
        for (i, (name, material)) in scene.materials.iter().enumerate() {
            // Proton takes only the diffuse texture from a 3DS material. Exporter
            // defaults (often white specular with power 1) are not authored lighting:
            // carrying them into O3D washes whole faces out under the sun and sky.
            let material = Material {
                texture: material.texture.clone(),
                ..Default::default()
            };
            let index = self.add(material, scene.mappings[i], Mode::Auto)?;
            names.entry(name.clone()).or_insert(index);
        }
        let default = self.add(Material::default(), UvTransform::default(), Mode::Auto)?;
        Ok((names, default))
    }
}

pub(crate) fn faces(o: &Object, names: &BTreeMap<String, u16>, default: u16) -> Result<Vec<u16>> {
    let mut result = vec![default; o.faces.len()];
    for (name, indices) in &o.groups {
        let &material = names
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown 3DS material {name}"))?;
        for &face in indices {
            result[face as usize] = material;
        }
    }
    Ok(result)
}
