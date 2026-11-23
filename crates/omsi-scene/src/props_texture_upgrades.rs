use super::*;

impl World {
    /// Swap in the textures compressed on the workers since the last call (until
    /// `deadline`), and start compressing the ones uploaded as RGBA since. Returns how many
    /// were swapped.
    pub fn apply_texture_upgrades(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        deadline: Option<std::time::Instant>,
    ) -> usize {
        let (wanted, restores) = {
            let mut gpu = self.gpu.lock();
            let mut w = std::mem::take(&mut gpu.wants_upgrade);
            w.append(&mut self.freetex_upgrades.lock());
            let r = std::mem::take(&mut gpu.wants_restore);
            w.extend(r.iter().cloned());
            (w, r)
        };
        if !wanted.is_empty() {
            let mut pending = self.upgrades_pending.lock();
            for path in wanted {
                if !pending.insert(path.clone()) {
                    continue;
                }
                let done = self.upgrades_done.clone();
                let restore = restores.contains(&path);
                let reloads = self.freetex_reloads.clone();
                // (off the frame's pool: see `threads`)
                crate::threads::background_pool().spawn(move || {
                    let tracked = reloads.tracks(&path);
                    let before = tracked
                        .then(|| crate::freetex_reload::FileStamp::read(&path))
                        .flatten();
                    if let Some(t) = load_texture_key(&path, true).filter(|_| {
                        !tracked || crate::freetex_reload::FileStamp::read(&path) == before
                    }) {
                        // (a texture over the budget comes back whatever it is, and so
                        // does one put up at half its size meanwhile: see
                        // `omsi_texture::gpu::halved_for_now`)
                        if t.format.is_compressed()
                            || restore
                            || (t.width >= 256 && t.height >= 256)
                        {
                            done.lock().push((path, Arc::new(t), before));
                            return;
                        }
                    }
                    // nothing better to be had: it stays as it is
                    done.lock().push((
                        path,
                        Arc::new(TextureData {
                            width: 0,
                            height: 0,
                            format: omsi_texture::PixelFormat::Rgba8,
                            levels: Vec::new(),
                            has_alpha: false,
                            gpu_mips: false,
                        }),
                        before,
                    ));
                });
            }
        }
        let mut swapped: Vec<TextureId> = Vec::new();
        let mut refreshed = 0;
        let mut rebound_reload = 0;
        // One ready file at most per frame, under the existing upload deadline.
        // All cached material variants keep referring to this same texture id.
        if deadline.is_none_or(|d| std::time::Instant::now() < d) {
            if let Some(reload) = self.freetex_reloads.next() {
                let id = self.vehicle_textures.lock().get(&reload.path).map(|e| e.0);
                if id == Some(reload.id)
                    && self
                        .freetex_reloads
                        .accepts(&reload.path, Some(reload.stamp))
                {
                    self.textures.release(&reload.path);
                    self.freetex_reloads
                        .remember(reload.id, reload.data.clone());
                    if renderer.refresh_texture(scene, reload.id, &reload.data) {
                        swapped.push(reload.id);
                        rebound_reload = 1;
                    }
                    refreshed = 1;
                }
            }
        }
        let mut usage = self.budget_checked.lock().in_use;
        let video_limit = self
            .video_memory_limit
            .load(std::sync::atomic::Ordering::Relaxed);
        loop {
            if deadline
                .map(|d| std::time::Instant::now() >= d)
                .unwrap_or(false)
                && (!swapped.is_empty() || refreshed > 0)
            {
                break;
            }
            let Some((path, data, stamp)) = self.upgrades_done.lock().pop() else {
                break;
            };
            self.upgrades_pending.lock().remove(&path);
            if !self.freetex_reloads.accepts(&path, stamp) {
                continue;
            }
            // the texture is still up under that name (it may have gone meanwhile)
            let vid = self.vehicle_textures.lock().get(&path).map(|e| e.0);
            let found = match vid {
                Some(id) => Some((id, false, false)),
                None => self
                    .gpu
                    .lock()
                    .textures
                    .get(&path)
                    .map(|e| (e.id, e.near_restore, e.squeezed)),
            };
            let Some((id, near, squeezed)) = found else {
                continue;
            };
            if data.levels.is_empty() {
                // nothing better to be had (an upgrade that stays RGBA)
                continue;
            }
            let card = renderer.gpu_memory(scene);
            let before = scene.texture_bytes_of(id);
            let allowed =
                if near {
                    let room = card_near_room(video_limit, card);
                    if squeezed {
                        room.min(
                            before.saturating_add(texture_room(self.texture_budget_bytes(), usage)),
                        )
                    } else {
                        room
                    }
                } else {
                    card_near_room(video_limit, card).min(before.saturating_add(
                        texture_restore_room(self.texture_budget_bytes(), usage, video_limit, card),
                    ))
                };
            let upload = if self.player_textures.lock().contains(&path) {
                data.clone()
            } else {
                let Some(upload) = fit_upgrade(&data, allowed) else {
                    continue;
                };
                upload
            };
            let replaced = renderer.replace_texture(scene, id, &upload);
            usage = (usage + scene.texture_bytes_of(id)).saturating_sub(before);
            if self.freetex_reloads.snapshot(id).is_some() {
                self.freetex_reloads.remember(id, data.clone());
            }
            if let Some(e) = self.gpu.lock().textures.get_mut(&path) {
                e.record_upload(&data, scene);
                e.retry_after = (e.dropped > 0)
                    .then(|| std::time::Instant::now() + std::time::Duration::from_secs(5));
            }
            if replaced {
                swapped.push(id);
            }
        }
        let n = swapped.len();
        // The materials are pointed at the new textures a quarter of a second after the
        // first of them came in, all together: the rebinding looks through every material of
        // the map (half a millisecond on Berlin-Spandau), and while driving a texture or two
        // came in nearly every frame. Until then they draw the texture they had (the same
        // picture: uncompressed, or with fewer levels).
        let mut memo = self.budget_checked.lock();
        if memo.rebind.is_empty() && n > 0 {
            memo.rebind_since = Some(std::time::Instant::now());
        }
        memo.rebind.extend_from_slice(&swapped);
        // (without a deadline - a picture - at once)
        let due = deadline.is_none()
            || memo.rebind.len() >= 64
            || memo
                .rebind_since
                .is_some_and(|t| t.elapsed().as_secs_f32() >= 0.25);
        if due && !memo.rebind.is_empty() {
            let ids = std::mem::take(&mut memo.rebind);
            drop(memo);
            let t = std::time::Instant::now();
            let rebound = renderer.rebind_textures(scene, &ids);
            if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                log::info!(
                    "textures: {} compressed ones swapped in, {rebound} materials rebound in {:.1} ms",
                    ids.len(),
                    t.elapsed().as_secs_f64() * 1000.0
                );
            }
        }
        n + refreshed - rebound_reload
    }
}
