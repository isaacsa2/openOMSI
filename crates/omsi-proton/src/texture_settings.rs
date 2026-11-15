//! Approximate the documented material presets without distributing built-in assets.
use crate::{
    files, ini,
    settings::{number, value},
    Report,
};
use anyhow::{ensure, Result};
use omsi_o3d::Material;
use std::{collections::BTreeMap, path::Path};

#[derive(Default)]
pub(crate) struct Presets(BTreeMap<String, String>);

impl Presets {
    pub fn read(textures: &Path, report: &mut Report) -> Result<Self> {
        let mut out = Self::default();
        if !textures.is_dir() {
            return Ok(out);
        }
        for file in files(textures, "txt")? {
            let filename = file.file_name().unwrap().to_string_lossy();
            let Some(texture) = filename
                .to_ascii_lowercase()
                .strip_suffix(".config.txt")
                .map(str::to_owned)
            else {
                continue;
            };
            ensure!(out.0.len() < 16_384, "too many texture configurations");
            let cfg = ini(&file)?;
            if value(&cfg, "config", "replacematerial") != Some("1") {
                continue;
            }
            let preset = value(&cfg, "config", "material")
                .unwrap_or("")
                .to_ascii_lowercase();
            if !["asphalt/", "concrete/", "pavements/"]
                .iter()
                .any(|p| preset.starts_with(p))
                && preset != "automotive/basicwhite"
            {
                report.warnings.push(format!(
                    "{filename}: unknown material preset; keeping the local texture"
                ));
                continue;
            }
            // Empty fields occur in released maps and mean the loader's default.
            if number(&cfg, "config", "keeptexture", 1.0)? == 0.0 {
                report.warnings.push(format!("{filename}: built-in replacement texture unavailable; keeping the local texture"));
            }
            out.0.insert(texture, preset);
        }
        Ok(out)
    }

    pub fn is_road(&self, texture: &str) -> bool {
        let texture = texture.replace('\\', "/");
        let name = texture.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
        self.0.get(&name).is_some_and(|p| p.starts_with("asphalt/"))
    }

    pub fn apply(&self, m: &mut Material) {
        let name = m.texture.replace('\\', "/");
        let name = name.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
        if let Some(preset) = self.0.get(&name) {
            // Conservative native equivalents. Do not reintroduce the bright exporter
            // specular defaults that used to wash out large road and building textures.
            m.specular = if preset == "automotive/basicwhite" {
                [0.12; 3]
            } else {
                [0.02; 3]
            };
            m.specular_power = if preset == "automotive/basicwhite" {
                32.0
            } else {
                16.0
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_resolve_windows_basename_and_keep_missing_builtin_textures() {
        let f = crate::tests::Fixture::new();
        std::fs::write(
            f.0.join("ROAD.PNG.config.txt"),
            "[config]\nreplaceMaterial=1\nmaterial=asphalt/asphalt017\nkeepTexture=0\n",
        )
        .unwrap();
        let mut report = Report::default();
        let presets = Presets::read(&f.0, &mut report).unwrap();
        let mut m = Material {
            texture: "C:\\artist\\Road.png".into(),
            ..Default::default()
        };
        presets.apply(&mut m);
        assert_eq!(m.specular, [0.02; 3]);
        assert_eq!(m.texture, "C:\\artist\\Road.png");
        assert_eq!(report.warnings.len(), 1);
        assert!(!crate::child(&f.0, "missing.png").unwrap().exists());
    }
}
