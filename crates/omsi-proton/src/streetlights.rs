//! Streetlight placeholders become native map lights and inexpensive coronas.
use crate::{child, convert::EDGE, files, ini, paths::centre, three_ds::Object, Report};
use anyhow::{ensure, Result};
use std::{collections::BTreeMap, path::Path};

struct Definition {
    prefix: String,
    real: [f32; 3],
    range: f32,
    fake: [f32; 3],
    alpha: f32,
    size: f32,
    texture: Option<String>,
    always_on: bool,
}

pub(crate) struct Lamp {
    pos: [f32; 3],
    color: [f32; 3],
    range: f32,
    size: f32,
    alpha: f32,
    texture: Option<String>,
    always_on: bool,
}

#[derive(Default)]
pub(crate) struct Collector {
    definitions: Vec<Definition>,
    pub tiles: BTreeMap<(i32, i32), Vec<Lamp>>,
    count: usize,
}

impl Collector {
    pub fn new(models: &Path, textures: &Path, dest: &Path, report: &mut Report) -> Result<Self> {
        let mut out = Self::default();
        let folder = child(models, "streetlights")?;
        if !folder.is_dir() {
            return Ok(out);
        }
        for file in files(&folder, "txt")? {
            let cfg = ini(&file)?;
            let get = |s: &str, k: &str| cfg.get(s).and_then(|s| s.get(k)).map(String::as_str);
            let n = |s: &str, k: &str, default: f32| -> Result<f32> {
                let v = get(s, k)
                    .map(str::parse::<f32>)
                    .transpose()?
                    .unwrap_or(default);
                ensure!(v.is_finite(), "invalid streetlight parameter {s}.{k}");
                Ok(v)
            };
            let rgb = |section: &str, defaults: [f32; 3]| -> Result<[f32; 3]> {
                Ok([
                    n(section, "colorr", defaults[0])?.clamp(0.0, 1.0),
                    n(section, "colorg", defaults[1])?.clamp(0.0, 1.0),
                    n(section, "colorb", defaults[2])?.clamp(0.0, 1.0),
                ])
            };
            let prefix = get("streetlight", "prefix")
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    file.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                });
            ensure!(!prefix.is_empty(), "empty streetlight prefix");
            let intensity = n("real", "intensity", 1.0)?.clamp(0.0, 1.0);
            let real = rgb("real", [1.0; 3])?.map(|v| v * intensity);
            let texture = get("fake", "texture")
                .filter(|t| !t.is_empty())
                .map(str::to_owned);
            let texture = if let Some(name) = texture {
                let path = child(textures, &name)?;
                if path.is_file() {
                    let name = path.file_name().unwrap().to_string_lossy().into_owned();
                    std::fs::create_dir_all(dest)?;
                    std::fs::copy(&path, dest.join(&name))?;
                    Some(name)
                } else {
                    report.warnings.push(format!(
                        "Missing streetlight glow texture: {name}; using native glow"
                    ));
                    None
                }
            } else {
                None
            };
            let always_on = get("streetlight", "alwayson") == Some("1");
            if always_on {
                report.warnings.push(format!("{prefix}: alwaysOn is applied to glow sprites; real illumination follows the native night cycle"));
            }
            if get("fake", "alwaysfacecamera") == Some("0") {
                report.warnings.push(format!(
                    "{prefix}: fixed glow orientation is approximated by a native billboard"
                ));
            }
            out.definitions.push(Definition {
                prefix,
                real,
                range: n("real", "range", 45.0)?.clamp(0.0, 200.0),
                fake: rgb("fake", [0.84, 0.77, 0.66])?,
                alpha: n("fake", "colora", 0.5)?.clamp(0.0, 1.0),
                size: n("fake", "texscalex", 10.0)?
                    .abs()
                    .max(n("fake", "texscaley", 10.0)?.abs())
                    .clamp(0.05, 50.0),
                texture,
                always_on,
            });
        }
        // More-specific prefixes win when the author has overlapping lamp names.
        out.definitions
            .sort_by_key(|d| std::cmp::Reverse(d.prefix.len()));
        Ok(out)
    }

    pub fn marker(&mut self, object: &Object, report: &mut Report) -> Result<bool> {
        let real = object.name.contains("_real_");
        let fake = object.name.contains("_fake_");
        if !real && !fake {
            return Ok(false);
        }
        let Some(d) = self
            .definitions
            .iter()
            .find(|d| object.name.contains(&d.prefix))
        else {
            return Ok(false);
        };
        let p = centre(object).ok_or_else(|| anyhow::anyhow!("invalid streetlight centre"))?;
        let key = ((p.x / EDGE).floor() as i32, (p.y / EDGE).floor() as i32);
        let local = [p.x - key.0 as f32 * EDGE, p.y - key.1 as f32 * EDGE, p.z];
        ensure!(self.count < 20_000, "too many streetlight markers");
        self.tiles.entry(key).or_default().push(Lamp {
            pos: local,
            color: if real { d.real } else { d.fake },
            range: if real { d.range } else { 0.0 },
            size: if fake { d.size } else { 0.0 },
            alpha: d.alpha,
            texture: d.texture.clone(),
            always_on: d.always_on,
        });
        self.count += 1;
        report.lights_imported += 1;
        Ok(true)
    }
}

pub(crate) fn append(sco: &mut String, lamps: &[Lamp]) {
    for lamp in lamps {
        let [x, y, z] = lamp.pos;
        let [r, g, b] = lamp.color;
        if lamp.range > 0.0 {
            // Native maplight radius is its bright core, with a six-radius cutoff.
            sco.push_str(&format!(
                "[maplight]\n{x}\n{y}\n{z}\n{r}\n{g}\n{b}\n{}\n",
                lamp.range / 6.0
            ));
        }
        if lamp.size > 0.0 && lamp.alpha > 0.0 {
            let switch = if lamp.always_on { "1" } else { "NightlightA" };
            // Effect bit 2 keeps just the authored glow, without extra fog halos.
            sco.push_str(&format!(
                "[light_enh]\n{x}\n{y}\n{z}\n{}\n{}\n{}\n{}\n{switch}\n{}\n0\n2\n0\n",
                r * 255.0,
                g * 255.0,
                b * 255.0,
                lamp.size,
                lamp.alpha
            ));
            if let Some(texture) = &lamp.texture {
                sco.push_str(&format!("{texture}\n"));
            }
        }
    }
}
