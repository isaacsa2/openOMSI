//! Texture memory accounting, mip reduction and restoration.
use super::*;

const CARD_STEP: u64 = 32 << 20;

pub(super) fn fit_upgrade(data: &Arc<TextureData>, allowed: u64) -> Option<Arc<TextureData>> {
    let mut upload = data.clone();
    while upload.gpu_bytes() > allowed {
        let side = upload.width.max(upload.height);
        if side <= 1 {
            return None;
        }
        let mut small = omsi_render::fit_texture(&upload, side / 2)?;
        if small.format.is_compressed() && (small.width % 4 != 0 || small.height % 4 != 0) {
            let rgba =
                small
                    .format
                    .decode_level(small.levels.first()?, small.width, small.height, true);
            small.format = omsi_texture::PixelFormat::Rgba8;
            small.levels = vec![rgba];
            small.gpu_mips = true;
        }
        upload = Arc::new(small);
    }
    Some(upload)
}

pub(super) fn card_restore_room(video_limit: u64, card: omsi_render::GpuMemory) -> u64 {
    match card.cap(video_limit) {
        0 => u64::MAX,
        cap => (cap - cap / 10).saturating_sub(card.total()),
    }
}

pub(super) fn card_near_room(video_limit: u64, card: omsi_render::GpuMemory) -> u64 {
    match card.cap(video_limit) {
        0 => u64::MAX,
        cap => cap.saturating_sub(card.total()),
    }
}

pub(super) fn card_hard_over(
    card: omsi_render::GpuMemory,
    video_limit: u64,
    card_over: u64,
    step: u64,
) -> u64 {
    match (card.budget, video_limit) {
        (None, _) => card_over,
        (Some(budget), 0) => (card.total().saturating_sub(budget) / step * step).min(card_over),
        (Some(budget), limit) => {
            (card.total().saturating_sub(budget.min(limit)) / step * step).min(card_over)
        }
    }
}

pub(super) fn texture_room(limit: u64, usage: u64) -> u64 {
    if limit == 0 {
        u64::MAX
    } else {
        (limit - limit / 100 * 15).saturating_sub(usage)
    }
}

pub(super) fn texture_restore_room(
    limit: u64,
    usage: u64,
    video_limit: u64,
    card: omsi_render::GpuMemory,
) -> u64 {
    texture_room(limit, usage).min(card_restore_room(video_limit, card))
}

pub(super) fn texture_restore_cost(e: &TexEntry) -> u64 {
    if e.bytes == 0 {
        e.full_bytes.min(128 * 128 * 4 * 4 / 3)
    } else {
        e.full_bytes.saturating_sub(e.bytes)
    }
}

pub(super) fn reserved_restore_bytes(gpu: &GpuCache, pending: &hashbrown::HashSet<PathBuf>) -> u64 {
    gpu.textures
        .iter()
        .filter(|(p, _)| pending.contains(*p) || gpu.wants_restore.contains(*p))
        .map(|(_, e)| texture_restore_cost(e))
        .sum()
}

/// Queue nearby restores within the card's free memory. Textures shrunk while
/// far away take priority over distant scenery when they come near the camera.
/// Return the number queued and whether some textures still need more headroom.
pub(super) fn queue_near_restores(
    gpu: &mut GpuCache,
    pending: &hashbrown::HashSet<PathBuf>,
    entries: &[(f64, PathBuf)],
    near: f64,
    (mut card_room, mut squeezed_room): (u64, u64),
    t0: std::time::Instant,
) -> (usize, bool) {
    let mut queued = 0;
    let mut blocked = false;
    for (d, p) in entries {
        if *d >= near || queued >= 24 || pending.contains(p) || gpu.wants_restore.contains(p) {
            continue;
        }
        let Some(e) = gpu.textures.get_mut(p) else {
            continue;
        };
        if e.dropped == 0 || e.retry_after.is_some_and(|t| t > t0) {
            continue;
        }
        let needed = texture_restore_cost(e);
        let peak = e.full_bytes.max(needed);
        if peak > card_room || (e.squeezed && needed > squeezed_room) {
            blocked = true;
            continue;
        }
        card_room -= peak;
        if e.squeezed {
            squeezed_room -= needed;
        }
        e.retry_after = None;
        e.near_restore = true;
        gpu.wants_restore.push(p.clone());
        queued += 1;
    }
    // A nearby texture may not fit back at full resolution although a smaller
    // GPU upload could fit. This fallback offers the closest reduced texture to
    // the existing fit_upgrade path when no full restore has been queued.
    //
    // This is an admission estimate, not a one-mip upload cap: fit_upgrade may
    // restore multiple levels if the upload-time GPU budget permits them.
    // replace_texture briefly holds both allocations. Limit the fallback to
    // one candidate per pass to avoid bursts near the memory limit.
    if queued == 0 {
        let partial = entries
            .iter()
            .filter(|(d, p)| *d < near && !pending.contains(p) && !gpu.wants_restore.contains(p))
            .filter_map(|(d, p)| {
                let e = gpu.textures.get(p)?;
                if e.dropped == 0 || e.retry_after.is_some_and(|t| t > t0) {
                    return None;
                }
                // One additional mip roughly quadruples the stored area. Use a
                // conservative bound (with room for mip/block rounding); the upload size is checked again by
                // fit_upgrade and the renderer before replacing any resource.
                let peak = if e.bytes == 0 {
                    e.full_bytes.min(128 * 128 * 4 * 4 / 3)
                } else {
                    e.full_bytes
                        .min(e.bytes.saturating_mul(4).saturating_add(4096))
                };
                (peak > e.bytes
                    && peak <= card_room
                    && (!e.squeezed || peak.saturating_sub(e.bytes) <= squeezed_room))
                    .then_some((*d, p))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p)) = partial {
            if let Some(e) = gpu.textures.get_mut(p) {
                e.retry_after = None;
                e.near_restore = true;
                gpu.wants_restore.push(p.clone());
                queued = 1;
            }
        }
    }
    (queued, blocked)
}

fn budget_usage(texture_usage: u64, limit: u64, card_bytes: u64, card_cap: u64) -> (u64, u64) {
    // (in steps of 32 MiB: the driver's count wanders by a few MB from second to second,
    // and every step up failed the idle look's check - the whole scan and its sort every
    // second while over)
    let card_over = if card_cap > 0 {
        card_bytes.saturating_sub(card_cap) / CARD_STEP * CARD_STEP
    } else {
        0
    };
    let usage = if card_over > 0 {
        texture_usage.max(limit.saturating_add(card_over))
    } else {
        texture_usage
    };
    (usage, card_over)
}

fn texture_usage(
    gpu: &GpuCache,
    scene_bytes: u64,
    vehicle_bytes: u64,
    t0: std::time::Instant,
    last: Option<std::time::Instant>,
) -> (u64, bool, bool) {
    let (scenery_bytes, any_dropped, retry_due) =
        gpu.textures
            .values()
            .fold((0u64, false, false), |(b, d, r), e| {
                let due = e
                    .retry_after
                    .is_some_and(|t| t <= t0 && last.is_none_or(|c| t > c));
                (b + e.bytes, d || e.dropped > 0, r || due)
            });
    // Every texture of the scene counts against the card as well (the people, the
    // script and text textures, the sky's and the terrain's): past half of the card's own
    // memory, what they hold beyond it is taken from the scenery and the vehicles. The
    // budget counted the scenery and the vehicles alone, and on an 8 GB card with a
    // 4000 MB budget the textures grew to 5.4 GB over a drive; with the picture's targets
    // the card paged to the system's memory and the frame rate fell from 140 to 25 (#2295).
    let texture_usage = scenery_bytes + vehicle_bytes + texture_overdraft(scene_bytes);
    (texture_usage, any_dropped, retry_due)
}

fn queue_distant_restores(
    gpu: &mut GpuCache,
    pending: &hashbrown::HashSet<PathBuf>,
    entries: &mut [(f64, PathBuf)],
    mut room: u64,
    limit: usize,
    t0: std::time::Instant,
) -> usize {
    // Room for the textures farther out to come back, with 15 % kept free: a texture
    // restored only into the room under 85 % of the budget is not shrunk again
    // before the textures have grown by that much (with a tenth, textures at
    // 150-400 m went back and forth as the tiles around came and went).
    let mut queued = 0;
    entries.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (d, p) in entries {
        if *d > 400.0 || queued >= limit {
            break;
        }
        if pending.contains(p) || gpu.wants_restore.contains(p) {
            continue;
        }
        let Some(e) = gpu.textures.get_mut(p) else {
            continue;
        };
        if e.dropped == 0 || e.retry_after.is_some_and(|t| t > t0) {
            continue;
        }
        e.retry_after = None;
        let needed = texture_restore_cost(e);
        if needed > room {
            continue;
        }
        room -= needed;
        gpu.wants_restore.push(p.clone());
        queued += 1;
    }
    queued
}

pub(super) fn ground_is_near(gpu: &GpuCache, near: &mut hashbrown::HashMap<TextureId, f64>) {
    if let Some(g) = &gpu.ground {
        for id in g.ground_id.into_iter().chain(g.ground_detail.map(|d| d.0)) {
            near.insert(id, 0.0);
        }
    }
}

pub(super) fn budget_level_drop(side: u32, levels: u32, floor: u32, bytes: u64, over: u64) -> u32 {
    let mut n = 0;
    while n + 1 < levels && side.checked_shr(n + 1).unwrap_or(0) >= floor {
        n += 1;
        if bytes.saturating_sub(bytes >> (2 * n).min(63)) >= over {
            break;
        }
    }
    n
}

impl World {
    /// OMSI's `[texmemlimit]`: the scenery and vehicle textures may take `bytes` on the GPU
    /// (0 = no limit), see [`World::update_texture_budget`].
    pub fn set_texture_budget(&self, bytes: u64) {
        self.texture_limit
            .store(bytes, std::sync::atomic::Ordering::Relaxed);
    }

    /// What the game may hold on the graphics card all together (bytes; 0 = no more than
    /// the driver's budget): over it, the textures give way as over their own budget (see
    /// [`World::update_texture_budget`]), since they are what can.
    pub fn set_video_memory_limit(&self, bytes: u64) {
        self.video_memory_limit
            .store(bytes, std::sync::atomic::Ordering::Relaxed);
    }

    /// The textures' budget now (bytes, 0 = none).
    pub fn texture_budget_bytes(&self) -> u64 {
        self.texture_limit
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether the textures were over their budget, or the card over the video memory, at the
    /// budget's last look (see [`World::update_texture_budget`]): the time to let go of what
    /// nobody draws rather than make the rest smaller.
    pub fn textures_over_budget(&self) -> bool {
        let limit = self.texture_budget_bytes();
        let in_use = self.budget_checked.lock().in_use;
        limit > 0 && in_use > limit
    }
}

impl World {
    /// Keep the textures within their budget, once a second (`force`: now, whatever was
    /// looked at last - the offscreen pictures): while they take
    /// more, the scenery textures only far tiles use lose their finest mip level, the
    /// farthest first, down to 64 texels a side. If that is insufficient, nearby scenery and
    /// roads also give way, down to a 256-texel longest side, and vehicle textures down to
    /// 1024. Scenery shrunk while far that is within 150 m of `centers` (the camera, the
    /// player's bus) comes back at once while the card has room; when there is room again,
    /// the rest within 400 m is read again on a worker and swapped back. Returns the textures
    /// shrunk or sent to be read again.
    ///
    /// A look that found nothing to do is not repeated while the tiles, the bytes in use
    /// and the centers (within 25 m) stay the same: once a texture had lost a level, the
    /// whole scan, its sort and the restores' search ran every second for good, standing
    /// still included. The shrinking stops after 2 ms of it and goes on next frame.
    pub fn update_texture_budget(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        centers: &[DVec3],
        force: bool,
    ) -> usize {
        self.texture_budget_pass(
            renderer,
            scene,
            centers,
            if force {
                BudgetPass::Now
            } else {
                BudgetPass::Timed
            },
        )
    }

    /// The texture budget after a streaming frame that changed the tiles: uploads can take
    /// the textures over their budget before the next second's look, so the far ones are
    /// made smaller at once - but only when they are over it (a tenth of a second apart at
    /// most). This was a whole look at every texture in every such frame.
    pub fn texture_budget_after_streaming(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        centers: &[DVec3],
    ) -> usize {
        self.texture_budget_pass(renderer, scene, centers, BudgetPass::AfterStreaming)
    }

    fn texture_budget_pass(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        centers: &[DVec3],
        pass: BudgetPass,
    ) -> usize {
        const NEAR: f64 = 150.0;
        const RESTORE: f64 = 400.0;
        const MIN_SIDE: u32 = 64;
        let limit = self
            .texture_limit
            .load(std::sync::atomic::Ordering::Relaxed);
        let video_limit = self
            .video_memory_limit
            .load(std::sync::atomic::Ordering::Relaxed);
        if (limit == 0 && video_limit == 0) || centers.is_empty() {
            return 0;
        }
        // (no texture budget of their own: the card's alone bounds them)
        let limit = if limit == 0 { u64::MAX / 4 } else { limit };
        let mut memo = self.budget_checked.lock();
        {
            let since = memo
                .checked
                .map(|t| t.elapsed().as_secs_f32())
                .unwrap_or(f32::MAX);
            // (a pass cut short by its time goes on in the next frame; one after streaming
            // waits a tenth of a second after the last)
            let due = memo.resume
                || match pass {
                    BudgetPass::Now => true,
                    BudgetPass::AfterStreaming => since >= 0.1,
                    BudgetPass::Timed => since >= 1.0,
                };
            if !due {
                return 0;
            }
        }
        let t0 = std::time::Instant::now();
        let player = self.player_textures.lock();
        let vehicle_bytes: u64 = self
            .vehicle_textures
            .lock()
            .values()
            .map(|(t, _)| scene.texture_bytes_of(*t))
            .sum();
        let generation = self
            .tiles_generation
            .load(std::sync::atomic::Ordering::Relaxed);
        let states = self.tile_state.lock();
        let mut gpu = self.gpu.lock();
        let last = memo.checked;
        let (scenery_bytes, any_dropped, retry_due) =
            gpu.textures
                .values()
                .fold((0u64, false, false), |(b, d, r), e| {
                    let due = e
                        .retry_after
                        .is_some_and(|t| t <= t0 && last.is_none_or(|c| t > c));
                    (b + e.bytes, d || e.dropped > 0, r || due)
                });
        // Every texture of the scene counts against the card as well (the people, the
        // script and text textures, the sky's and the terrain's): past half of the card's own
        // memory, what they hold beyond it is taken from the scenery and the vehicles. The
        // budget counted the scenery and the vehicles alone, and on an 8 GB card with a
        // 4000 MB budget the textures grew to 5.4 GB over a drive; with the picture's targets
        // the card paged to the system's memory and the frame rate fell from 140 to 25 (#2295).
        let texture_usage =
            scenery_bytes + vehicle_bytes + texture_overdraft(renderer.texture_bytes(scene));
        // And everything the game holds on the card - the meshes, the render targets and
        // shadow maps, the driver's own - stays within the video memory setting and the
        // driver's budget: the textures counted against their budget alone, the game took
        // gigabytes more than it was set to, and an 8 GB card paged to the system's memory
        // past its budget. What is over goes from the textures, the only part that can give.
        let card = renderer.gpu_memory(scene);
        let card_cap = card.cap(video_limit);
        // (in steps of 32 MiB: the driver's count wanders by a few MB from second to second,
        // and every step up failed the idle look's check - the whole scan and its sort every
        // second while over)
        const STEP: u64 = 32 << 20;
        let card_over = if card_cap > 0 {
            card.total().saturating_sub(card_cap) / STEP * STEP
        } else {
            0
        };
        let usage = if card_over > 0 {
            texture_usage.max(limit.saturating_add(card_over))
        } else {
            texture_usage
        };
        memo.in_use = usage;
        if pass == BudgetPass::AfterStreaming && usage <= limit && !memo.resume {
            // (the bytes are all a forced look is for: the restores wait for the second's)
            return 0;
        }
        memo.checked = Some(t0);
        if usage <= limit && !any_dropped {
            memo.resume = false;
            memo.near_retry_at = None;
            return 0;
        }
        if pass != BudgetPass::Now
            && !memo.resume
            && !retry_due
            && !memo.near_retry_at.is_some_and(|at| at <= t0)
            && memo.unchanged(
                generation,
                usage,
                limit,
                card_restore_room(video_limit, card),
                centers,
            )
        {
            return 0;
        }
        let ts = tile_size();
        let tile_distance = |k: &(i32, i32)| -> f64 {
            centers
                .iter()
                .map(|c| {
                    let (x0, y0) = (k.0 as f64 * ts, k.1 as f64 * ts);
                    let dx = (x0 - c.x).max(c.x - (x0 + ts)).max(0.0);
                    let dy = (y0 - c.y).max(c.y - (y0 + ts)).max(0.0);
                    (dx * dx + dy * dy).sqrt()
                })
                .fold(f64::MAX, f64::min)
        };
        // how near each scenery texture is: the nearest tile that uses it
        let mut near: hashbrown::HashMap<TextureId, f64> = hashbrown::HashMap::new();
        let mut spline_textures = hashbrown::HashSet::new();
        let (mut types, mut splines): (
            hashbrown::HashMap<usize, f64>,
            hashbrown::HashMap<usize, f64>,
        ) = Default::default();
        let (mut trees, mut shared): (
            hashbrown::HashMap<&str, f64>,
            hashbrown::HashMap<&PathBuf, f64>,
        ) = Default::default();
        for (key, st) in states.iter() {
            let d = tile_distance(key);
            let nearer = |e: &mut f64| *e = e.min(d);
            st.gpu
                .types
                .iter()
                .for_each(|t| nearer(types.entry(*t).or_insert(f64::MAX)));
            st.gpu
                .spline_types
                .iter()
                .for_each(|t| nearer(splines.entry(*t).or_insert(f64::MAX)));
            st.gpu
                .trees
                .iter()
                .for_each(|t| nearer(trees.entry(t.as_str()).or_insert(f64::MAX)));
            st.gpu
                .shared_textures
                .iter()
                .for_each(|p| nearer(shared.entry(p).or_insert(f64::MAX)));
        }
        {
            let g = &*gpu;
            let mut see = |p: &PathBuf, d: f64| {
                if let Some(e) = g.textures.get(p) {
                    let n = near.entry(e.id).or_insert(f64::MAX);
                    *n = n.min(d);
                }
            };
            for (t, d) in &types {
                g.types
                    .get(t)
                    .into_iter()
                    .flat_map(|t| t.textures.iter())
                    .for_each(|p| see(p, *d));
            }
            for (t, d) in &splines {
                for p in g.splines.get(t).into_iter().flat_map(|s| s.textures.iter()) {
                    see(p, *d);
                    spline_textures.insert(p.clone());
                }
            }
            for (t, d) in &trees {
                g.trees
                    .get(*t)
                    .and_then(|t| t.texture.as_ref())
                    .into_iter()
                    .for_each(|p| see(p, *d));
            }
            for (p, d) in &shared {
                see(p, *d);
            }
        }
        ground_is_near(&gpu, &mut near);
        drop((trees, shared));
        drop(states);
        let mut entries: Vec<(f64, PathBuf)> = gpu
            .textures
            .iter()
            .map(|(p, e)| (near.get(&e.id).copied().unwrap_or(f64::MAX), p.clone()))
            .collect();
        let mut shrunk: Vec<TextureId> = Vec::new();
        let mut restoring = 0usize;
        let mut cut_short = false;
        // (2 ms of shrinking a frame, the scan's own time not counted; a picture takes
        // what it takes)
        let budget_end = std::time::Instant::now()
            + if pass == BudgetPass::Now {
                std::time::Duration::from_secs(3600)
            } else {
                std::time::Duration::from_millis(2)
            };
        let pending = self.upgrades_pending.lock();
        let reserved = if card_cap == 0 && usage > limit {
            0
        } else {
            reserved_restore_bytes(&gpu, &pending)
        };
        let rooms = (
            card_near_room(video_limit, card).saturating_sub(reserved),
            texture_room(limit, texture_usage).saturating_sub(reserved),
        );
        let (near_restores, blocked) =
            queue_near_restores(&mut gpu, &pending, &entries, NEAR, rooms, t0);
        restoring += near_restores;
        memo.near_retry_at = blocked.then(|| t0 + std::time::Duration::from_secs(2));
        if usage > limit {
            entries.sort_by(|a, b| b.0.total_cmp(&a.0));
            let mut over = usage - limit;
            let card_part = (usage - texture_usage.max(limit)).saturating_sub(card_hard_over(
                card,
                video_limit,
                card_over,
                STEP,
            ));
            let mut drops = renderer.begin_level_drops();
            for (d, p) in &entries {
                // (96 a second: at 16 a map's first tiles stayed over a small card's budget
                // for a minute and a half)
                if over == 0 || shrunk.len() >= 96 || *d < NEAR {
                    break;
                }
                if std::time::Instant::now() >= budget_end {
                    cut_short = true;
                    break;
                }
                if spline_textures.contains(p) {
                    continue;
                }
                let Some(e) = gpu.textures.get_mut(p) else {
                    continue;
                };
                let Some((w, h, levels)) = renderer.texture_levels(scene, e.id) else {
                    continue;
                };
                if w.min(h) / 2 < MIN_SIDE || levels < 2 {
                    continue;
                }
                let before = e.bytes;
                let n = budget_level_drop(w.min(h), levels, MIN_SIDE, before, over);
                if renderer.drop_top_levels_in(&mut drops, scene, e.id, n) {
                    e.bytes = scene.texture_bytes_of(e.id);
                    e.dropped += n;
                    over = over.saturating_sub(before - e.bytes);
                    shrunk.push(e.id);
                }
            }
            // Still over: the vehicles' pictures give way as well, the biggest first, down
            // to 1024 pixels a side. Counted but never made smaller, the timetable's fleet
            // filled 5 GB of textures on a graphics chip with 0.5 GB of its own until the
            // device was lost (#1463).
            if over > 0 && shrunk.len() < 96 && !cut_short {
                let mut own: Vec<(TextureId, u64)> = self
                    .vehicle_textures
                    .lock()
                    .iter()
                    .filter(|(p, _)| !player.contains(*p))
                    .map(|(_, (t, _))| (*t, scene.texture_bytes_of(*t)))
                    .collect();
                own.sort_by(|a, b| b.1.cmp(&a.1));
                for (id, before) in own {
                    if over == 0 || shrunk.len() >= 96 {
                        break;
                    }
                    if std::time::Instant::now() >= budget_end {
                        cut_short = true;
                        break;
                    }
                    let Some((w, h, levels)) = renderer.texture_levels(scene, id) else {
                        continue;
                    };
                    if w.min(h) / 2 < 1024 || levels < 2 {
                        continue;
                    }
                    if renderer.drop_top_levels_in(&mut drops, scene, id, 1) {
                        over =
                            over.saturating_sub(before.saturating_sub(scene.texture_bytes_of(id)));
                        shrunk.push(id);
                    }
                }
            }
            if over > card_part && shrunk.len() < 96 && !cut_short {
                let mut emergency: Vec<(TextureId, Option<PathBuf>)> = entries
                    .iter()
                    .filter_map(|(_, p)| gpu.textures.get(p).map(|e| (e.id, Some(p.clone()))))
                    .collect();
                emergency.extend(
                    self.vehicle_textures
                        .lock()
                        .iter()
                        .filter(|(p, _)| !player.contains(*p))
                        .map(|(_, (id, _))| (*id, None)),
                );
                for (id, path) in emergency {
                    if over <= card_part || shrunk.len() >= 96 {
                        break;
                    }
                    if std::time::Instant::now() >= budget_end {
                        cut_short = true;
                        break;
                    }
                    let Some((w, h, levels)) = renderer.texture_levels(scene, id) else {
                        continue;
                    };
                    let before = scene.texture_bytes_of(id);
                    let floor = if path.is_some() { 256 } else { 1024 };
                    let n = budget_level_drop(w.max(h), levels, floor, before, over - card_part);
                    if n > 0 && renderer.drop_top_levels_in(&mut drops, scene, id, n) {
                        let after = scene.texture_bytes_of(id);
                        over = over.saturating_sub(before.saturating_sub(after));
                        if let Some(e) = path.as_ref().and_then(|p| gpu.textures.get_mut(p)) {
                            e.bytes = after;
                            e.dropped += n;
                            e.squeezed = true;
                        }
                        shrunk.push(id);
                    }
                }
            }
            renderer.finish_level_drops(drops);
        } else {
            // Room for the near ones to come back, with 15 % kept free: a texture
            // restored only into the room under 85 % of the budget is not shrunk again
            // before the textures have grown by that much (with a tenth, textures at
            // 150-400 m went back and forth as the tiles around came and went).
            entries.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut room =
                texture_restore_room(limit, usage, video_limit, card).saturating_sub(reserved);
            for (d, p) in &entries {
                if *d > RESTORE || restoring >= 16 {
                    break;
                }
                if pending.contains(p) || gpu.wants_restore.contains(p) {
                    continue;
                }
                let Some(e) = gpu.textures.get_mut(p) else {
                    continue;
                };
                if e.dropped == 0 || e.retry_after.is_some_and(|t| t > t0) {
                    continue;
                }
                e.retry_after = None;
                let needed = texture_restore_cost(e);
                if needed > room {
                    continue;
                }
                room -= needed;
                gpu.wants_restore.push(p.clone());
                restoring += 1;
            }
        }
        drop(pending);
        drop(gpu);
        // Nothing left to do for now: not looked at again before something changed. (A pass
        // that shrank or sent back textures changes the bytes in use, now or when they come
        // back, and looks again.)
        memo.remember(
            generation,
            usage,
            limit,
            card_restore_room(video_limit, card),
            centers,
            shrunk.is_empty() && restoring == 0,
        );
        memo.resume = cut_short;
        // (the swapped-in textures waiting for theirs go along)
        let shrunk_n = shrunk.len();
        shrunk.append(&mut memo.rebind);
        drop(memo);
        let rebound = renderer.rebind_textures(scene, &shrunk);
        if omsi_cfg::flags::OMSI_PROFILE.is_set() {
            log::info!(
                "texture budget: {:.0} of {:.0} MB in use; on the card {:.0} of {:.0} MB (textures {:.0}, meshes {:.0}, the rest {:.0}{}); {} textures lost a level ({} materials rebound), {} coming back{}, in {:.1} ms",
                usage as f64 / 1e6,
                limit as f64 / 1e6,
                card.total() as f64 / 1e6,
                card_cap as f64 / 1e6,
                card.textures as f64 / 1e6,
                card.meshes as f64 / 1e6,
                card.other as f64 / 1e6,
                if card.measured {
                    " as the driver counts"
                } else {
                    " worked out"
                },
                shrunk_n,
                rebound,
                restoring,
                if cut_short { ", more next frame" } else { "" },
                t0.elapsed().as_secs_f64() * 1000.0
            );
        }
        shrunk_n + restoring
    }

}
