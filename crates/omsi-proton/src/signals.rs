//! Traffic lamp channels and stop triggers share one native clock per junction.
use crate::{
    child,
    convert::{Faces, EDGE},
    files, ini,
    materials::Registry,
    paths::centre,
    settings::{label, number, value},
    tags::Mode,
    three_ds::Object,
    Report,
};
use anyhow::{ensure, Result};
use glam::Vec3;
use omsi_o3d::{Mesh, Triangle, Vertex};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) struct Trigger {
    pub group: usize,
    pub path: usize,
    pub centre: Vec3,
    pub radius: f32,
    pub height: f32,
    pub any_angle: bool,
}
pub(crate) struct Bulb {
    pub key: (i32, i32),
    pub group: usize,
    pub channel: usize,
    pub stem: String,
    pub pos: [f32; 3],
}
pub(crate) struct Program {
    prefix: String,
    pub phases: Vec<Vec<(i32, f32)>>,
    pub cycle: f32,
    radius: f32,
    height: f32,
    paths: usize,
    pub tiles: BTreeSet<(i32, i32)>,
    colors: [[f32; 3]; 3],
}
#[derive(Default)]
pub(crate) struct Collector {
    pub programs: Vec<Program>,
    pub triggers: Vec<Trigger>,
    pub bulbs: Vec<Bulb>,
    faces: usize,
}

impl Collector {
    pub fn new(models: &Path, report: &mut Report) -> Result<Self> {
        let folder = child(models, "trafficlights")?;
        let mut out = Self::default();
        if !folder.is_dir() {
            return Ok(out);
        }
        for file in files(&folder, "txt")? {
            ensure!(out.programs.len() < 1024, "too many traffic-light programs");
            let cfg = ini(&file)?;
            let prefix = label(
                value(&cfg, "trafficlight", "prefix")
                    .unwrap_or(&file.file_name().unwrap().to_string_lossy()),
            )?;
            let paths = number(&cfg, "trafficlight", "howmanypaths", 2.0)? as usize;
            let ticks = number(&cfg, "trafficlight", "howmanyticks", 2.0)? as usize;
            ensure!(
                (1..=16).contains(&paths) && (1..=256).contains(&ticks),
                "traffic-light configuration exceeds budget"
            );
            let interval = number(&cfg, "trafficlight", "tickinterval", 1.0)?;
            ensure!(
                (0.01..=3600.0).contains(&interval),
                "invalid traffic-light interval"
            );
            let mut phases = vec![Vec::new(); paths * 4];
            let first =
                (number(&cfg, "trafficlight", "firstticktorun", 1.0)? as usize).clamp(1, ticks) - 1;
            let mut cycle = 0.0;
            for k in 0..ticks {
                let section = format!("tick{}", (first + k) % ticks + 1);
                let repeat = number(&cfg, &section, "repeat", 1.0)?;
                ensure!(
                    (0.0..=3600.0).contains(&repeat),
                    "invalid traffic-light repeat"
                );
                let duration = repeat * interval;
                if duration == 0.0 {
                    continue;
                }
                cycle += duration;
                ensure!(cycle <= 86400.0, "traffic-light cycle exceeds one day");
                for path in 0..paths {
                    // Keep the three visual channels independent of the stopping channel:
                    // a decorative flashing yellow without a trigger must not hold cars.
                    for (channel, (name, on)) in
                        [("red", 0), ("yellow", 9), ("green", 6), ("trigger", 0)]
                            .into_iter()
                            .enumerate()
                    {
                        let enabled =
                            value(&cfg, &section, &format!("path{}_{name}", path + 1)) == Some("1");
                        phases[path * 4 + channel].push((if enabled { on } else { 12 }, duration));
                    }
                }
            }
            if cycle <= 0.0 {
                report.warnings.push(format!(
                    "{prefix}: zero-duration traffic-light cycle omitted"
                ));
                continue;
            }
            let mut colors = [[1.0, 0.0, 0.0], [1.0, 0.92, 0.02], [0.0, 1.0, 0.0]];
            for (i, section) in ["red_light", "yellow_light", "green_light"]
                .iter()
                .enumerate()
            {
                let intensity = number(&cfg, section, "intensity", 1.0)?.clamp(0.0, 1.0);
                for (j, key) in ["colorr", "colorg", "colorb"].iter().enumerate() {
                    colors[i][j] =
                        number(&cfg, section, key, colors[i][j])?.clamp(0.0, 1.0) * intensity;
                }
            }
            if value(&cfg, "trafficlight", "randomtimestampatstart") != Some("0") || first != 0 {
                report.warnings.push(format!("{prefix}: signal phases use the native game-time clock; start timing is approximated"));
            }
            if value(&cfg, "trafficlight", "usereallights") != Some("0") {
                report.warnings.push(format!("{prefix}: traffic lamps use native glows; coloured real-light range is not imported"));
            }
            out.programs.push(Program {
                prefix,
                phases,
                cycle,
                paths,
                radius: number(&cfg, "trafficlight", "triggerradius", 1.0)?.clamp(0.1, 20.0),
                height: number(&cfg, "trafficlight", "triggerheight", 5.0)?.clamp(0.1, 50.0),
                tiles: BTreeSet::new(),
                colors,
            });
        }
        Ok(out)
    }

    pub fn marker(
        &mut self,
        object: &Object,
        registry: &mut Registry,
        names: &BTreeMap<String, u16>,
        default: u16,
        dir: &Path,
        textures: &Path,
        report: &mut Report,
        cancel: &impl Fn() -> bool,
    ) -> Result<bool> {
        for (group, program) in self.programs.iter_mut().enumerate() {
            let Some(rest) = object
                .name
                .strip_prefix('_')
                .unwrap_or(&object.name)
                .strip_prefix(&program.prefix)
                .and_then(|s| s.strip_prefix("_path"))
            else {
                continue;
            };
            let Some((path, tag)) = rest.split_once('_') else {
                continue;
            };
            let Ok(path) = path.parse::<usize>() else {
                continue;
            };
            if !(1..=program.paths).contains(&path) {
                continue;
            }
            let p =
                centre(object).ok_or_else(|| anyhow::anyhow!("invalid traffic-light marker"))?;
            if tag.starts_with("trigger_") || tag.starts_with("meshtrigger_") {
                ensure!(
                    self.triggers.len() < 20_000,
                    "too many traffic-light triggers"
                );
                let radius = if tag.starts_with("meshtrigger_") {
                    report.warnings.push(format!(
                        "{}: mesh stop trigger approximated by its horizontal bounds",
                        object.name
                    ));
                    object
                        .positions
                        .iter()
                        .map(|v| (v.truncate() - p.truncate()).length())
                        .fold(program.radius, f32::max)
                        .min(20.0)
                } else {
                    program.radius
                };
                self.triggers.push(Trigger {
                    group,
                    path: path - 1,
                    centre: p,
                    radius,
                    height: program.height,
                    any_angle: object.name.contains("_anyangle_"),
                });
                return Ok(true);
            }
            let Some(color) = ["red_", "yellow_", "green_"]
                .iter()
                .position(|color| tag.starts_with(color))
            else {
                continue;
            };
            ensure!(
                self.bulbs.len() < 20_000 && self.faces + object.faces.len() <= 250_000,
                "traffic-lamp geometry exceeds budget"
            );
            self.faces += object.faces.len();
            let mode = match crate::tags::classify(&object.name) {
                crate::tags::ObjectKind::Mesh(Mode::Additive) => Mode::Additive,
                crate::tags::ObjectKind::Mesh(Mode::Blend | Mode::EmissiveBlend) => {
                    Mode::EmissiveBlend
                }
                crate::tags::ObjectKind::Mesh(Mode::Cutout | Mode::EmissiveCutout) => {
                    Mode::EmissiveCutout
                }
                _ => Mode::Emissive,
            };
            let faces = registry.object_faces(object, names, default, mode)?;
            let mut sink = BulbMesh {
                mesh: Mesh::default(),
                origin: p,
            };
            let mut bytes = 0;
            crate::convert::append_object(
                object,
                &faces,
                &registry.mappings,
                &mut sink,
                &mut bytes,
                cancel,
            )?;
            // Compact material slots so a tiny lamp never duplicates the whole map's list.
            let mut remap = BTreeMap::new();
            for t in &mut sink.mesh.triangles {
                let next = remap.len() as u16;
                let source = t.material;
                let slot = *remap.entry(source).or_insert_with(|| {
                    let mut m = registry.materials[source as usize].clone();
                    m.emissive = program.colors[color];
                    sink.mesh.materials.push(m);
                    next
                });
                t.material = slot;
            }
            crate::textures(
                textures,
                &dir.join("proton-import/texture"),
                &mut sink.mesh.materials,
                &mut report.warnings,
                cancel,
            )?;
            let stem = format!("signal_{}", self.bulbs.len());
            let models = dir.join("proton-import/model");
            std::fs::create_dir_all(&models)?;
            crate::convert::write_mesh(
                &models.join(format!("{stem}.o3d")),
                &sink.mesh,
                &sink.mesh.materials,
            )?;
            let variable = ["red", "yellow", "green"][color];
            let [r, g, b] = program.colors[color];
            let mut sco = format!("[absheight]\n[fixed]\n[trafficlight]\n[mesh]\n{stem}.o3d\n[visible]\n{variable}\n1\n");
            crate::export::material_overrides(
                &mut sco,
                &sink.mesh.materials,
                &vec![mode; sink.mesh.materials.len()],
            );
            sco.push_str(&format!(
                "[light_enh]\n0\n0\n0\n{}\n{}\n{}\n0.4\n{variable}\n0.5\n0\n2\n0\n",
                r * 255.0,
                g * 255.0,
                b * 255.0
            ));
            std::fs::write(dir.join(format!("proton-import/{stem}.sco")), sco)?;
            let key = ((p.x / EDGE).floor() as i32, (p.y / EDGE).floor() as i32);
            program.tiles.insert(key);
            self.bulbs.push(Bulb {
                key,
                group,
                channel: (path - 1) * 4 + color,
                stem,
                pos: p.to_array(),
            });
            return Ok(true);
        }
        Ok(false)
    }
}

struct BulbMesh {
    mesh: Mesh,
    origin: Vec3,
}
impl Faces for BulbMesh {
    fn face(&mut self, key: (i32, i32), mut vertices: [Vertex; 3], material: u16) -> Result<()> {
        let origin = Vec3::new(key.0 as f32 * EDGE, key.1 as f32 * EDGE, 0.0);
        for v in &mut vertices {
            v.position += origin - self.origin;
        }
        let n = self.mesh.vertices.len() as u32;
        self.mesh.vertices.extend_from_slice(&vertices);
        self.mesh.triangles.push(Triangle {
            indices: [n, n + 1, n + 2],
            material,
        });
        Ok(())
    }
}
