use super::*;

enum StorageEvent {
    Progress(String),
    Done(std::result::Result<crate::asset_storage::StorageReport, String>),
}

#[derive(Default)]
pub struct StorageView {
    root: String,
    report: Option<crate::asset_storage::StorageReport>,
    message: String,
    job: Option<std::sync::mpsc::Receiver<StorageEvent>>,
    confirm_restore: bool,
}

impl StorageView {
    pub(super) fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub(super) fn refresh_root(&mut self, root: &std::path::Path) {
        let key = root.to_string_lossy().to_string();
        if self.root == key || self.busy() {
            return;
        }
        self.root = key;
        self.report = if root.as_os_str().is_empty() { None } else { Some(crate::asset_storage::quick_status(root)) };
        self.message.clear();
        self.confirm_restore = false;
    }

    pub(super) fn poll(&mut self) {
        let mut finished = false;
        if let Some(rx) = self.job.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(StorageEvent::Progress(m)) => self.message = m,
                    Ok(StorageEvent::Done(Ok(r))) => {
                        self.report = Some(r);
                        self.message = "Done.".into();
                        self.confirm_restore = false;
                        finished = true;
                        break;
                    }
                    Ok(StorageEvent::Done(Err(e))) => {
                        self.message = e;
                        finished = true;
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.message = "The storage task stopped unexpectedly.".into();
                        finished = true;
                        break;
                    }
                }
            }
        }
        if finished {
            self.job = None;
        }
    }

    pub(super) fn start(&mut self, root: PathBuf, mode: crate::asset_storage::Mode) {
        if self.busy() || root.as_os_str().is_empty() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.job = Some(rx);
        self.confirm_restore = false;
        self.message = match mode {
            crate::asset_storage::Mode::Analyze => "Analyzing content…",
            crate::asset_storage::Mode::Compress => "Compressing content…",
            crate::asset_storage::Mode::Restore => "Restoring original files…",
        }
        .into();
        let _ = std::thread::Builder::new()
            .name("content compression".into())
            .spawn(move || {
                let progress_tx = tx.clone();
                let result = crate::asset_storage::execute(&root, mode, |m| {
                    let _ = progress_tx.send(StorageEvent::Progress(m));
                })
                .map_err(|e| format!("{e:#}"));
                let _ = tx.send(StorageEvent::Done(result));
            });
    }
}

pub(super) fn storage_tab(ui: &mut Ui, storage: &mut StorageView, out: &mut Outside, cols: [Rect; 2]) -> [f32; 2] {
    let report = storage.report.clone().unwrap_or_default();

    let mut c = Col::new(ui, cols[0], "Content compression");
    let mut enabled = report.active && !storage.confirm_restore;
    let before = enabled;
    if ui.toggle("storage-compression", c.row(), &mut enabled, "Compress supported content") && !storage.busy() {
        if storage.confirm_restore && enabled {
            storage.confirm_restore = false;
        } else if !report.active && enabled {
            out.storage_action = Some(crate::asset_storage::Mode::Compress);
        } else if report.active && !enabled {
            storage.confirm_restore = true;
        }
    }
    c.y += ui.paragraph(
        "Off by default. When enabled, supported files are compressed losslessly on disk; the game still opens them by their original names.",
        Vec2::new(c.inner.x, c.y),
        c.inner.w,
        12.5,
        Weight::Regular,
        TEXT_DIM,
    ) + 8.0;

    if storage.busy() {
        c.y += ui.paragraph(
            if storage.message.is_empty() { "Working…" } else { &storage.message },
            Vec2::new(c.inner.x, c.y),
            c.inner.w,
            13.0,
            Weight::Bold,
            ACCENT,
        ) + 10.0;
    } else {
        if ui.button("storage-analyze", c.row(), "Analyze installation", Some("search"), ButtonKind::Normal) {
            out.storage_action = Some(crate::asset_storage::Mode::Analyze);
        }
        if !storage.message.is_empty() && storage.message != "Done." {
            c.y += ui.paragraph(&storage.message, Vec2::new(c.inner.x, c.y), c.inner.w, 12.5, Weight::Regular, TEXT_DIM) + 8.0;
        }
    }

    if storage.confirm_restore {
        c.section(ui, "Restore original files");
        let needed = report.restore_required_bytes;
        let free = report.free_bytes;
        let text = match free {
            Some(f) => format!(
                "This reverses the operation and removes the compressed sidecars. Up to {} of free space is needed while restoring; {} is available.",
                fmt_bytes(needed),
                fmt_bytes(f)
            ),
            None => format!(
                "This reverses the operation and removes the compressed sidecars. Up to {} of free space may be needed while restoring.",
                fmt_bytes(needed)
            ),
        };
        c.y += ui.paragraph(&text, Vec2::new(c.inner.x, c.y), c.inner.w, 12.5, Weight::Regular, TEXT_DIM) + 8.0;
        let r = c.row();
        let half = (r.w - GAP) * 0.5;
        if ui.button("storage-restore-cancel", Rect::new(r.x, r.y, half, r.h), "Keep compressed", None, ButtonKind::Normal) {
            storage.confirm_restore = false;
        }
        if ui.button("storage-restore", Rect::new(r.x + half + GAP, r.y, half, r.h), "Restore originals", Some("restart_alt"), ButtonKind::Danger) {
            out.storage_action = Some(crate::asset_storage::Mode::Restore);
        }
    }

    let left = c.used();
    let mut c = Col::new(ui, cols[1], "Storage");
    if report.active {
        c.y += ui.paragraph(
            &format!(
                "Active · {} files\n{} → {} · {} saved",
                report.compressed_files,
                fmt_bytes(report.current_original_bytes),
                fmt_bytes(report.current_stored_bytes),
                fmt_bytes(report.current_saved_bytes())
            ),
            Vec2::new(c.inner.x, c.y),
            c.inner.w,
            13.0,
            Weight::Bold,
            TEXT,
        ) + 10.0;
    } else {
        c.y += ui.paragraph(
            "Disabled · the installation is stored normally.",
            Vec2::new(c.inner.x, c.y),
            c.inner.w,
            13.0,
            Weight::Bold,
            TEXT,
        ) + 10.0;
    }

    if report.eligible_files > 0 {
        c.section(ui, "Last analysis");
        c.y += ui.paragraph(
            &format!(
                "{} eligible files · {}\nEstimated stored size: {}\nEstimated saving: {}",
                report.eligible_files,
                fmt_bytes(report.eligible_bytes),
                fmt_bytes(report.estimated_stored_bytes),
                fmt_bytes(report.estimated_saved_bytes())
            ),
            Vec2::new(c.inner.x, c.y),
            c.inner.w,
            12.5,
            Weight::Regular,
            TEXT_SOFT,
        ) + 8.0;
        for k in &report.kinds {
            c.y += ui.paragraph(
                &format!("{} · {} files · {}", k.name, k.files, fmt_bytes(k.original_bytes)),
                Vec2::new(c.inner.x, c.y),
                c.inner.w,
                11.5,
                Weight::Regular,
                TEXT_DIM,
            ) + 3.0;
        }
    }

    c.section(ui, "What is eligible");
    c.y += ui.paragraph(
        "WAV audio; O3D and X models; BMP and TGA textures; and only uncompressed RGB/RGBA DDS. DXT/BC DDS, PNG, JPG, OGG, MP3, ZIP, RAR and 7z stay untouched.",
        Vec2::new(c.inner.x, c.y),
        c.inner.w,
        12.5,
        Weight::Regular,
        TEXT_DIM,
    ) + 8.0;
    c.y += ui.paragraph(
        &format!(
            "Files below {} are skipped. A file is kept raw unless compression saves at least {}%. There is no quality loss.",
            fmt_bytes(crate::asset_storage::MIN_FILE_BYTES),
            crate::asset_storage::MIN_SAVING_PERCENT
        ),
        Vec2::new(c.inner.x, c.y),
        c.inner.w,
        12.5,
        Weight::Regular,
        TEXT_DIM,
    ) + 8.0;
    c.y += ui.paragraph(
        "The original OMSI 2 executable cannot read .omc sidecars. Use Restore originals before opening the same installation in OMSI 2.",
        Vec2::new(c.inner.x, c.y),
        c.inner.w,
        12.5,
        Weight::Medium,
        DANGER.lighten(0.25),
    ) + 8.0;

    [left, c.used()]
}
