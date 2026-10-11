//! Static 3DS geometry. Vertex positions in 3DS are already in the file's world frame;
//! its local-axis matrix describes the object's frame, not another transform to apply.
use anyhow::{bail, ensure, Result};
use glam::{Vec2, Vec3};
use omsi_o3d::Material;

#[derive(Default)]
pub struct Model {
    pub materials: Vec<(String, Material)>,
    pub objects: Vec<Object>,
    pub trailing_bytes: usize,
}

#[derive(Default)]
pub struct Object {
    pub name: String,
    pub positions: Vec<Vec3>,
    pub uv: Vec<Vec2>,
    pub faces: Vec<[u16; 3]>,
    pub groups: Vec<(String, Vec<u16>)>,
    pub smoothing: Vec<u32>,
}

struct Chunk<'a> {
    tag: u16,
    data: &'a [u8],
}

fn chunks(mut b: &[u8]) -> Result<Vec<Chunk<'_>>> {
    let mut out = Vec::new();
    while !b.is_empty() {
        ensure!(b.len() >= 6, "truncated 3DS chunk header");
        let tag = u16::from_le_bytes(b[..2].try_into()?);
        let len = u32::from_le_bytes(b[2..6].try_into()?) as usize;
        ensure!(len >= 6 && len <= b.len(), "invalid 3DS chunk length {len}");
        out.push(Chunk {
            tag,
            data: &b[6..len],
        });
        b = &b[len..];
    }
    Ok(out)
}

fn string(b: &[u8]) -> Result<(String, &[u8])> {
    let n = b
        .iter()
        .position(|&v| v == 0)
        .ok_or_else(|| anyhow::anyhow!("unterminated 3DS name"))?;
    let (s, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(&b[..n]);
    Ok((s.into_owned(), &b[n + 1..]))
}

fn records(b: &[u8], size: usize) -> Result<(usize, &[u8], &[u8])> {
    ensure!(b.len() >= 2, "missing 3DS record count");
    let n = u16::from_le_bytes(b[..2].try_into()?) as usize;
    let end = 2 + n * size;
    ensure!(end <= b.len(), "truncated 3DS records");
    Ok((n, &b[2..end], &b[end..]))
}

fn float(b: &[u8]) -> Result<f32> {
    ensure!(b.len() >= 4, "truncated 3DS float");
    let v = f32::from_le_bytes(b[..4].try_into()?);
    ensure!(v.is_finite(), "non-finite 3DS coordinate");
    Ok(v)
}

fn color(b: &[u8]) -> Result<[f32; 3]> {
    let mut value = [1.0; 3];
    for c in chunks(b)? {
        match c.tag {
            0x0010 | 0x0013 => {
                ensure!(c.data.len() >= 12, "truncated 3DS color");
                value = [float(c.data)?, float(&c.data[4..])?, float(&c.data[8..])?];
            }
            0x0011 | 0x0012 => {
                ensure!(c.data.len() >= 3, "truncated 3DS color");
                value = [
                    c.data[0] as f32 / 255.0,
                    c.data[1] as f32 / 255.0,
                    c.data[2] as f32 / 255.0,
                ];
            }
            _ => {}
        }
    }
    Ok(value)
}

fn material(b: &[u8]) -> Result<(String, Material)> {
    let mut name = String::new();
    let mut m = Material::default();
    for c in chunks(b)? {
        match c.tag {
            0xa000 => name = string(c.data)?.0,
            0xa020 => m.diffuse[..3].copy_from_slice(&color(c.data)?),
            0xa050 => {
                for p in chunks(c.data)? {
                    let transparency = match p.tag {
                        0x0030 => {
                            ensure!(p.data.len() >= 2, "truncated percentage");
                            u16::from_le_bytes(p.data[..2].try_into()?) as f32 / 100.0
                        }
                        0x0031 => float(p.data)?,
                        _ => continue,
                    };
                    m.diffuse[3] = 1.0 - transparency.clamp(0.0, 1.0);
                }
            }
            0xa200 => {
                for t in chunks(c.data)? {
                    if t.tag == 0xa300 {
                        m.texture = string(t.data)?.0;
                    }
                }
            }
            _ => {}
        }
    }
    Ok((name, m))
}

fn geometry(b: &[u8], o: &mut Object) -> Result<()> {
    for c in chunks(b)? {
        match c.tag {
            0x4110 => {
                let (_, data, _) = records(c.data, 12)?;
                for p in data.chunks_exact(12) {
                    o.positions
                        .push(Vec3::new(float(p)?, float(&p[4..])?, float(&p[8..])?));
                }
            }
            0x4140 => {
                let (_, data, _) = records(c.data, 8)?;
                for p in data.chunks_exact(8) {
                    o.uv.push(Vec2::new(float(p)?, 1.0 - float(&p[4..])?));
                }
            }
            0x4120 => {
                let (_, data, tail) = records(c.data, 8)?;
                for f in data.chunks_exact(8) {
                    o.faces.push([
                        u16::from_le_bytes(f[..2].try_into()?),
                        u16::from_le_bytes(f[2..4].try_into()?),
                        u16::from_le_bytes(f[4..6].try_into()?),
                    ]);
                }
                for g in chunks(tail)? {
                    match g.tag {
                        0x4130 => {
                            let (name, tail) = string(g.data)?;
                            let (_, data, _) = records(tail, 2)?;
                            o.groups.push((
                                name,
                                data.chunks_exact(2)
                                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                                    .collect(),
                            ));
                        }
                        0x4150 => {
                            ensure!(
                                g.data.len() == o.faces.len() * 4,
                                "invalid smoothing group count"
                            );
                            o.smoothing = g
                                .data
                                .chunks_exact(4)
                                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                                .collect();
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for f in &o.faces {
        ensure!(
            f.iter().all(|&i| (i as usize) < o.positions.len()),
            "3DS face names a missing vertex"
        );
    }
    for (_, faces) in &o.groups {
        ensure!(
            faces.iter().all(|&i| (i as usize) < o.faces.len()),
            "3DS material names a missing face"
        );
    }
    Ok(())
}

pub fn parse(bytes: &[u8]) -> Result<Model> {
    ensure!(
        bytes.len() >= 6 && bytes[..2] == [0x4d, 0x4d],
        "not a 3DS file"
    );
    let end = u32::from_le_bytes(bytes[2..6].try_into()?) as usize;
    ensure!(
        end >= 6 && end <= bytes.len(),
        "invalid 3DS root chunk length {end}"
    );
    // Some files carry extra data after their declared root (Osasco's PFLORA10).
    // Only the declared root belongs to this model; nested chunks remain strictly bounded.
    let top = chunks(&bytes[..end])?;
    let mut m = Model {
        trailing_bytes: bytes.len() - end,
        ..Default::default()
    };
    for root in chunks(top[0].data)? {
        if root.tag != 0x3d3d {
            continue;
        }
        for c in chunks(root.data)? {
            match c.tag {
                0xafff => m.materials.push(material(c.data)?),
                0x4000 => {
                    let (name, rest) = string(c.data)?;
                    let mut o = Object {
                        name,
                        ..Default::default()
                    };
                    for g in chunks(rest)? {
                        if g.tag == 0x4100 {
                            geometry(g.data, &mut o)?;
                        }
                    }
                    if !o.faces.is_empty() {
                        m.objects.push(o);
                    }
                }
                _ => {}
            }
        }
    }
    if m.materials.len() >= u16::MAX as usize {
        bail!("too many 3DS materials");
    }
    Ok(m)
}
