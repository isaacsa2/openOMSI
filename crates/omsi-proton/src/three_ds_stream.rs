//! Two passes over chunk headers: materials first, then one mesh object at a time.
//! Unknown payloads are sought past, never buffered; every child stays inside its parent.
use super::*;
use std::io::{BufReader, Seek};

#[derive(Clone, Copy)]
struct Span {
    tag: u16,
    data: u64,
    end: u64,
}

pub struct Scene {
    reader: BufReader<std::fs::File>,
    root: Span,
    pub materials: Vec<(String, Material)>,
    pub mappings: Vec<UvTransform>,
    pub trailing_bytes: usize,
    pub keyframes: bool,
}

impl Scene {
    pub fn open(path: &Path, cancel: &impl Fn() -> bool) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let size = file.metadata()?.len();
        ensure!(
            size <= FILE_LIMIT_BYTES,
            "3DS file exceeds the 1 GiB import limit"
        );
        let mut s = Self {
            reader: BufReader::with_capacity(64 * 1024, file),
            root: Span {
                tag: 0,
                data: 0,
                end: size,
            },
            materials: Vec::new(),
            mappings: Vec::new(),
            trailing_bytes: 0,
            keyframes: false,
        };
        s.root = s.header(size)?;
        ensure!(s.root.tag == 0x4d4d, "not a 3DS file");
        s.trailing_bytes = (size - s.root.end) as usize;
        let mut position = s.root.data;
        while position < s.root.end {
            ensure!(!cancel(), "map import cancelled");
            let span = s.at(position, s.root.end)?;
            if span.tag == 0x3d3d {
                let mut child = span.data;
                while child < span.end {
                    ensure!(!cancel(), "map import cancelled");
                    let item = s.at(child, span.end)?;
                    if item.tag == 0xafff {
                        let data = s.selected(item, 0)?;
                        let (name, mat, uv) = material(&data)?;
                        s.materials.push((name, mat));
                        s.mappings.push(uv);
                        ensure!(
                            s.materials.len() < u16::MAX as usize,
                            "too many 3DS materials"
                        );
                    }
                    child = item.end;
                }
            } else if span.tag == 0xb000 {
                s.keyframes = true;
            }
            position = span.end;
        }
        Ok(s)
    }

    fn header(&mut self, parent_end: u64) -> Result<Span> {
        let start = self.reader.stream_position()?;
        ensure!(
            parent_end.saturating_sub(start) >= 6,
            "truncated 3DS chunk header"
        );
        let mut header = [0; 6];
        self.reader.read_exact(&mut header)?;
        let len = u32::from_le_bytes(header[2..].try_into()?) as u64;
        let end = start + len;
        ensure!(
            len >= 6 && end <= parent_end,
            "invalid 3DS chunk length {len}"
        );
        Ok(Span {
            tag: u16::from_le_bytes(header[..2].try_into()?),
            data: start + 6,
            end,
        })
    }

    fn at(&mut self, position: u64, parent_end: u64) -> Result<Span> {
        // seek_relative keeps the buffered bytes on sequential traversals.
        let here = self.reader.stream_position()?;
        self.reader.seek_relative(position as i64 - here as i64)?;
        self.header(parent_end)
    }

    fn bytes(&mut self, span: Span, n: usize) -> Result<Vec<u8>> {
        ensure!(span.end - span.data >= n as u64, "truncated 3DS records");
        let mut data = vec![0; n];
        self.reader.read_exact(&mut data)?;
        Ok(data)
    }

    fn name(&mut self, span: Span) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        while self.reader.stream_position()? < span.end {
            let mut byte = [0];
            self.reader.read_exact(&mut byte)?;
            data.push(byte[0]);
            if byte[0] == 0 {
                return Ok(data);
            }
            ensure!(data.len() <= 4096, "3DS name exceeds 4096 bytes");
        }
        bail!("unterminated 3DS name")
    }

    fn record(&mut self, span: Span, width: usize) -> Result<Vec<u8>> {
        let mut count = self.bytes(span, 2)?;
        let n = u16::from_le_bytes(count[..].try_into()?) as usize;
        ensure!(
            2 + n * width <= (span.end - span.data) as usize,
            "truncated 3DS records"
        );
        let mut data = vec![0; n * width];
        self.reader.read_exact(&mut data)?;
        count.extend(data);
        Ok(count)
    }

    fn push(out: &mut Vec<u8>, tag: u16, data: &[u8]) {
        out.extend(tag.to_le_bytes());
        out.extend(((data.len() + 6) as u32).to_le_bytes());
        out.extend(data);
    }

    /// Preserve only understood records, skipping arbitrarily large unknown subchunks.
    fn selected(&mut self, parent: Span, kind: u8) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut position = parent.data;
        while position < parent.end {
            let item = self.at(position, parent.end)?;
            let data = match (kind, item.tag) {
                (0, 0xa000) | (3, 0xa300) => Some(self.name(item)?),
                (0, 0xa010 | 0xa020 | 0xa030) => Some(self.selected(item, 1)?),
                (0, 0xa040 | 0xa041 | 0xa050 | 0xa084) => Some(self.selected(item, 2)?),
                (0, 0xa080) => Some(Vec::new()),
                (0, 0xa200) => Some(self.selected(item, 3)?),
                (1, 0x0010 | 0x0013) => Some(self.bytes(item, 12)?),
                (1, 0x0011 | 0x0012) => Some(self.bytes(item, 3)?),
                (2, 0x0030) => Some(self.bytes(item, 2)?),
                (2, 0x0031) | (3, 0xa354 | 0xa356 | 0xa358 | 0xa35a | 0xa35c) => {
                    Some(self.bytes(item, 4)?)
                }
                _ => None,
            };
            if let Some(data) = data {
                Self::push(&mut out, item.tag, &data);
            }
            position = item.end;
        }
        Ok(out)
    }

    fn mesh(&mut self, span: Span, cancel: &impl Fn() -> bool) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut position = span.data;
        while position < span.end {
            ensure!(!cancel(), "map import cancelled");
            let item = self.at(position, span.end)?;
            let data = match item.tag {
                0x4110 => Some(self.record(item, 12)?),
                0x4140 => Some(self.record(item, 8)?),
                0x4160 => {
                    let data = self.bytes(item, 48)?;
                    for f in data.chunks_exact(4) {
                        float(f)?;
                    }
                    None // Positions already contain the world transform.
                }
                0x4120 => {
                    let mut data = self.record(item, 8)?;
                    let count = u16::from_le_bytes(data[..2].try_into()?) as usize;
                    let mut tail = item.data + data.len() as u64;
                    while tail < item.end {
                        ensure!(!cancel(), "map import cancelled");
                        let child = self.at(tail, item.end)?;
                        match child.tag {
                            0x4130 => {
                                let mut group = self.name(child)?;
                                let current = self.reader.stream_position()?;
                                group.extend(self.record(
                                    Span {
                                        data: current,
                                        ..child
                                    },
                                    2,
                                )?);
                                Self::push(&mut data, child.tag, &group);
                            }
                            0x4150 => {
                                ensure!(
                                    child.end - child.data == (count * 4) as u64,
                                    "invalid smoothing group count"
                                );
                                Self::push(&mut data, child.tag, &self.bytes(child, count * 4)?);
                            }
                            _ => {}
                        }
                        tail = child.end;
                    }
                    Some(data)
                }
                _ => None,
            };
            if let Some(data) = data {
                Self::push(&mut out, item.tag, &data);
            }
            position = item.end;
        }
        Ok(out)
    }

    pub fn visit(
        &mut self,
        cancel: &impl Fn() -> bool,
        mut object: impl FnMut(Object) -> Result<()>,
    ) -> Result<()> {
        let mut position = self.root.data;
        while position < self.root.end {
            ensure!(!cancel(), "map import cancelled");
            let edit = self.at(position, self.root.end)?;
            if edit.tag == 0x3d3d {
                let mut child = edit.data;
                while child < edit.end {
                    ensure!(!cancel(), "map import cancelled");
                    let item = self.at(child, edit.end)?;
                    if item.tag == 0x4000 {
                        let name = string(&self.name(item)?)?.0;
                        let mut mesh_at = self.reader.stream_position()?;
                        let mut data = Vec::new();
                        while mesh_at < item.end {
                            let mesh = self.at(mesh_at, item.end)?;
                            if mesh.tag == 0x4100 {
                                data.extend(self.mesh(mesh, cancel)?);
                            }
                            mesh_at = mesh.end;
                        }
                        let mut o = Object {
                            name,
                            ..Default::default()
                        };
                        geometry(&data, &mut o)?;
                        drop(data);
                        // Authored traffic points can be single vertices with no faces.
                        // Consumers decide whether an object is a marker or scenery.
                        if !o.positions.is_empty() {
                            object(o)?;
                        }
                    }
                    child = item.end;
                }
            }
            position = edit.end;
        }
        Ok(())
    }
}
