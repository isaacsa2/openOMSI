//! Share identical source material/mapping pairs while preserving per-file names.
use crate::{
    optimise::material_key,
    three_ds::{Object, Scene, UvTransform},
};
use anyhow::{ensure, Result};
use omsi_o3d::Material;
use std::collections::BTreeMap;

type Key = (([u32; 11], String), [u32; 5]);
#[derive(Default)]
pub(crate) struct Registry {
    pub materials: Vec<Material>,
    pub mappings: Vec<UvTransform>,
    seen: BTreeMap<Key, u16>,
}

impl Registry {
    pub fn into_materials(self) -> Vec<Material> {
        self.materials
    }
    fn add(&mut self, material: Material, uv: UvTransform) -> Result<u16> {
        let key = (material_key(&material), uv.key());
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
        self.seen.insert(key, i);
        Ok(i)
    }
    pub fn register(&mut self, scene: &Scene) -> Result<(BTreeMap<String, u16>, u16)> {
        let mut names = BTreeMap::new();
        for (i, (name, material)) in scene.materials.iter().enumerate() {
            let index = self.add(material.clone(), scene.mappings[i])?;
            names.entry(name.clone()).or_insert(index);
        }
        let default = self.add(Material::default(), UvTransform::default())?;
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
