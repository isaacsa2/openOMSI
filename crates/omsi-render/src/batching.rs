//! Draws turned into instanced batches (draws of the same mesh range with materials that look
//! alike and the same pipeline become one draw), and the batches recorded into a pass or a
//! bundle with as few state changes as possible.

use super::{MaterialId, Scene};

/// One draw of a mesh range with a material for one per-draw entry, before batching.
/// `pipe` picks the pipeline within a pass (and orders the batches).
#[derive(Clone, Copy)]
pub(super) struct DrawItem {
    pub(super) pipe: u8,
    pub(super) mesh: u32,
    pub(super) range: u32,
    pub(super) material: u32,
    /// The material's look: draws of materials that look alike are batched together.
    pub(super) look: u32,
    pub(super) entry: u32,
}

/// Draws of the same mesh range with materials that look alike and the same pipeline, made as one instanced
/// draw: `instances` indexes the frame's draw list, which holds each instance's per-draw
/// entry (the vertex shader looks it up). Thousands of single draws were the biggest CPU
/// cost of a frame - wgpu validates and records every one - and trees, lamps, fences,
/// people and the AI cars' shared meshes collapse into a few hundred batches.
#[derive(Clone)]
pub(super) struct Batch {
    pub(super) pipe: u8,
    pub(super) mesh: u32,
    pub(super) first: u32,
    pub(super) count: u32,
    pub(super) material: u32,
    pub(super) instances: std::ops::Range<u32>,
}

/// Turn draw items into batches, appending their entries to `list`. `sort`: the order does
/// not matter (depth-tested opaque and alpha-tested draws), so draws alike are gathered;
/// otherwise only neighbours are merged (the blended pass keeps its far-to-near order).
pub(super) fn batch_items(
    scene: &Scene,
    items: &mut [DrawItem],
    sort: bool,
    list: &mut Vec<u32>,
    out: &mut Vec<Batch>,
) {
    // a batch keeps the material of the one before it when they look alike: no rebinding
    let mut last = (u32::MAX, 0);
    let mut push = |d: &DrawItem, start: u32, list: &Vec<u32>| {
        if last.0 != d.look {
            last = (d.look, d.material);
        }
        let (first, count, _) = scene.meshes[d.mesh as usize].ranges[d.range as usize];
        out.push(Batch {
            pipe: d.pipe,
            mesh: d.mesh,
            first,
            count,
            material: last.1,
            instances: start..list.len() as u32,
        });
    };
    if sort {
        let bits = |n: u32| u32::BITS - n.leading_zeros();
        let (mut look, mut mesh, mut range) = (0, 0, 0);
        for d in items.iter() {
            (look, mesh, range) = (look | d.look, mesh | d.mesh, range | d.range);
        }
        let mesh_shift = bits(range);
        let look_shift = mesh_shift + bits(mesh);
        let pipe_shift = look_shift + bits(look);
        if pipe_shift + 8 <= 64 {
            // one number a draw sorts twice as fast as comparing the four fields
            let mut keys: Vec<(u64, u32, u32)> = items
                .iter()
                .map(|d| {
                    let key = (d.pipe as u64) << pipe_shift
                        | (d.look as u64) << look_shift
                        | (d.mesh as u64) << mesh_shift
                        | d.range as u64;
                    (key, d.entry, d.material)
                })
                .collect();
            keys.sort_unstable_by_key(|k| k.0);
            let field = |key: u64, from: u32, to: u32| ((key >> from) & ((1 << (to - from)) - 1)) as u32;
            let mut k = 0;
            while k < keys.len() {
                let (key, _, material) = keys[k];
                let start = list.len() as u32;
                while k < keys.len() && keys[k].0 == key {
                    list.push(keys[k].1);
                    k += 1;
                }
                let d = DrawItem {
                    pipe: (key >> pipe_shift) as u8,
                    mesh: field(key, mesh_shift, look_shift),
                    range: field(key, 0, mesh_shift),
                    material,
                    look: field(key, look_shift, pipe_shift),
                    entry: 0,
                };
                push(&d, start, list);
            }
            return;
        }
        items.sort_unstable_by_key(|d| (d.pipe, d.look, d.mesh, d.range));
    }
    let mut k = 0;
    while k < items.len() {
        let d = items[k];
        let start = list.len() as u32;
        while k < items.len() && (items[k].pipe, items[k].mesh, items[k].range, items[k].look) == (d.pipe, d.mesh, d.range, d.look) {
            list.push(items[k].entry);
            k += 1;
        }
        push(&d, start, list);
    }
}

/// The material and look a depth-only draw (shadow map, depth prepass) is batched with: an
/// opaque surface writes its depth whatever its texture, so all of them share the first
/// material and batch across materials; an alpha-tested one needs its own texture for the cut-out.
pub(super) fn depth_only_material(kind: u8, material: MaterialId, look: u32) -> (u32, u32) {
    if kind == 0 {
        (0, 0)
    } else {
        (material as u32, look)
    }
}

/// Record batches into a pass or a bundle, setting pipeline, buffers and material only
/// when they change.
pub(super) fn encode_batches<'a, E: wgpu::util::RenderEncoder<'a>>(
    pass: &mut E,
    scene: &'a Scene,
    batches: &[Batch],
    pipeline: impl Fn(u8) -> &'a wgpu::RenderPipeline,
) {
    encode_batches_filtered(pass, scene, batches, |_| true, pipeline);
}

/// Record the batches accepted by `include`, setting pipeline, buffers and material only
/// when they change.
pub(super) fn encode_batches_filtered<'a, E: wgpu::util::RenderEncoder<'a>>(
    pass: &mut E,
    scene: &'a Scene,
    batches: &[Batch],
    include: impl Fn(&Batch) -> bool,
    pipeline: impl Fn(u8) -> &'a wgpu::RenderPipeline,
) {
    let (mut pipe, mut mesh, mut material) = (u8::MAX, u32::MAX, u32::MAX);
    let mut page: Option<(&wgpu::Buffer, &wgpu::Buffer)> = None;
    let (mut base_vertex, mut first_index) = (0i32, 0u32);
    for b in batches {
        if !include(b) {
            continue;
        }
        if b.pipe != pipe {
            pass.set_pipeline(pipeline(b.pipe));
            pipe = b.pipe;
        }
        if b.mesh != mesh {
            let m = &scene.meshes[b.mesh as usize];
            if page.is_none_or(|(v, i)| *v != m.vertex_buf || *i != m.index_buf) {
                pass.set_vertex_buffer(0, m.vertex_buf.slice(..));
                pass.set_index_buffer(m.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                page = Some((&m.vertex_buf, &m.index_buf));
            }
            (base_vertex, first_index) = (m.base_vertex, m.first_index);
            mesh = b.mesh;
        }
        if b.material != material {
            pass.set_bind_group(
                1,
                Some(&scene.materials[b.material as usize].bind_group),
                &[],
            );
            material = b.material;
        }
        pass.draw_indexed(first_index + b.first..first_index + b.first + b.count, base_vertex, b.instances.clone());
    }
}
