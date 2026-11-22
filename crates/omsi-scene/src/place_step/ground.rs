//! Ground meshes and painted layers for an uploaded tile.
use super::*;

impl World {
    /// The ground of a tile: its terrain (cut under the roads and lit by its light map),
    /// the walls of the holes, the painted ground layers and the water.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn place_ground(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        gpu: &mut GpuCache,
        p: &Prepared,
        tg: &mut TileGpu,
        pl: &mut Placing,
        mesh: &MeshData,
        g: &GroundIds,
    ) {
        let GroundIds {
            ground_id,
            plain_terrain_mat,
            ground_detail,
            ground_repeats,
            ground_wet,
            water_mat,
            ..
        } = *g;
        let id = gpu.add_mesh(renderer, scene, mesh);
        tg.meshes.push(id);
        // the tile's night light map (lamp light pools on the ground)
        let lm = p.light_map.as_ref().map(|img| {
            let t = gpu.add_data(renderer, scene, img);
            tg.textures.push(t);
            t
        });
        let mat = match &p.cut {
            Some(img) => {
                let tex = gpu.add_data(renderer, scene, img);
                tg.textures.push(tex);
                let m = renderer.add_terrain_material(
                    scene,
                    ground_id,
                    Some(tex),
                    ground_detail,
                    ground_repeats,
                    lm,
                    ground_wet,
                );
                let m = gpu.material(renderer, scene, m);
                tg.materials.push(m);
                m
            }
            None if lm.is_some() => {
                let m = renderer.add_terrain_material(
                    scene,
                    ground_id,
                    None,
                    ground_detail,
                    ground_repeats,
                    lm,
                    ground_wet,
                );
                let m = gpu.material(renderer, scene, m);
                tg.materials.push(m);
                m
            }
            None => plain_terrain_mat,
        };
        let ground_instance = instance!(gpu, renderer, scene, tg; renderer.add_instance(
            scene,
            id,
            p.origin,
            Mat4::IDENTITY,
            vec![mat]
        ));
        if let Some(inst) = scene.instances.get_mut(ground_instance) {
            inst.render_phase = RenderPhase::Terrain;
        }
        // (the base layer once more without the cut, when the tile has one)
        let uncut = match (&p.cut, lm) {
            (None, _) => mat,
            (Some(_), None) => plain_terrain_mat,
            (Some(_), Some(_)) => {
                let m = renderer.add_terrain_material(
                    scene,
                    ground_id,
                    None,
                    ground_detail,
                    ground_repeats,
                    lm,
                    ground_wet,
                );
                let m = gpu.material(renderer, scene, m);
                tg.materials.push(m);
                m
            }
        };
        pl.terrain_mapping_mat = Some(uncut);
        let wall_id = if p.hole_walls.indices.is_empty() {
            None
        } else {
            let wall = gpu.add_mesh(renderer, scene, &p.hole_walls);
            tg.meshes.push(wall);
            let wi = instance!(gpu, renderer, scene, tg; renderer.add_instance(
                scene,
                wall,
                p.origin,
                Mat4::IDENTITY,
                vec![uncut]
            ));
            if let Some(inst) = scene.instances.get_mut(wi) {
                inst.render_phase = RenderPhase::Terrain;
            }
            Some(wall)
        };
        self.place_ground_paint(renderer, scene, gpu, p, tg, id, wall_id, lm);
        place_water(renderer, scene, gpu, p, tg, water_mat);
    }

    #[allow(clippy::too_many_arguments)]
    fn place_ground_paint(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        gpu: &mut GpuCache,
        p: &Prepared,
        tg: &mut TileGpu,
        id: MeshId,
        wall_id: Option<MeshId>,
        lm: Option<TextureId>,
    ) {
        let images: &HashMap<PathBuf, Arc<TextureData>> = &p.images;
        let ground_dirs = self.ground_dirs();
        // The painted ground: every further [groundtex] the editor's brush put on this
        // tile is the same tile mesh once more, blended in through its own mask - which
        // is how OMSI's car parks get their asphalt, its side streets their cobbles and
        // its meadows their fields.
        let no_paint = omsi_cfg::flags::OMSI_NO_GROUND_PAINT.is_set();
        for (layer, mask, painted) in p.paint.iter().filter(|_| !no_paint) {
            let Some(gt) = self.global.ground_textures.get(*layer) else {
                continue;
            };
            let tex = gpu.add_data(renderer, scene, mask);
            tg.textures.push(tex);
            let layer_tex = gpu
                .texture(renderer, scene, &gt.texture, &ground_dirs, images)
                .map(|(id, path)| {
                    tg.shared_textures.push(path);
                    id
                });
            let detail = gpu
                .texture(renderer, scene, &gt.detail_texture, &ground_dirs, images)
                .map(|(id, path)| {
                    tg.shared_textures.push(path);
                    (id, gt.detail_repeats())
                });
            let gdirs: Vec<&Path> = ground_dirs.iter().map(|p| p.as_path()).collect();
            let gcfg = self.textures.cfg(&gt.texture, &gdirs);
            let wet = gcfg.wetness_kind();
            let m = renderer.add_terrain_layer_material(
                scene,
                layer_tex,
                tex,
                detail,
                gt.repeats(),
                lm,
                wet,
            );
            let m = gpu.material(renderer, scene, m);
            tg.materials.push(m);
            let li = instance!(gpu, renderer, scene, tg; renderer.add_surface_instance(
                scene,
                id,
                p.origin,
                Mat4::IDENTITY,
                vec![m]
            ));
            if let Some(inst) = scene.instances.get_mut(li) {
                inst.ground_layer = true;
                inst.render_phase = RenderPhase::Terrain;
            }
            if omsi_cfg::flags::OMSI_DEBUG_SURFACES.is_set() {
                log::info!(
                    "tile ({}, {}): ground layer {layer} '{}' painted on {:.1} % of the tile, mask {:?}",
                    p.tx,
                    p.ty,
                    gt.texture,
                    painted * 100.0,
                    mask.format
                );
            }
        }
        // Exposed sides keep the original brush layers. The horizontal ground's
        // masks include the hole cut and would erase these vertical faces again.
        if let Some(wall) = wall_id {
            for (layer, mask) in p.wall_paint.iter().filter(|_| !no_paint) {
                let Some(gt) = self.global.ground_textures.get(*layer) else {
                    continue;
                };
                let tex = gpu.add_data(renderer, scene, mask);
                tg.textures.push(tex);
                let layer_tex = gpu
                    .texture(renderer, scene, &gt.texture, &ground_dirs, images)
                    .map(|(id, path)| {
                        tg.shared_textures.push(path);
                        id
                    });
                let detail = gpu
                    .texture(renderer, scene, &gt.detail_texture, &ground_dirs, images)
                    .map(|(id, path)| {
                        tg.shared_textures.push(path);
                        (id, gt.detail_repeats())
                    });
                let gdirs: Vec<&Path> = ground_dirs.iter().map(|p| p.as_path()).collect();
                let cfg = self.textures.cfg(&gt.texture, &gdirs);
                let m = renderer.add_terrain_layer_material(
                    scene,
                    layer_tex,
                    tex,
                    detail,
                    gt.repeats(),
                    lm,
                    cfg.wetness_kind(),
                );
                let m = gpu.material(renderer, scene, m);
                tg.materials.push(m);
                let wi = instance!(gpu, renderer, scene, tg; renderer.add_surface_instance(
                    scene,
                    wall,
                    p.origin,
                    Mat4::IDENTITY,
                    vec![m]
                ));
                if let Some(inst) = scene.instances.get_mut(wi) {
                    inst.ground_layer = true;
                    inst.render_phase = RenderPhase::Terrain;
                }
            }
        }
    }
}
