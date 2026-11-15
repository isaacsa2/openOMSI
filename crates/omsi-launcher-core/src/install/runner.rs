use super::*;

pub(super) fn run(job: &Job, content: &Path, root: Option<&Path>) -> Result<()> {
    // (the unit tests leave the user's own data folder alone)
    let data_dir = if cfg!(test) {
        content.join(".test-data")
    } else {
        crate::data_dir()
    };
    for line in cleanup_stale(&data_dir, Some(content)) {
        job.set(|p| p.report.push(line.clone()));
    }
    let src = job.source.clone();
    let archive = archive_kind(&src);
    let is_zip = archive == Some(ArchiveKind::Zip);
    let staged_archive = matches!(archive, Some(ArchiveKind::SevenZip | ArchiveKind::Rar));
    if archive.is_none() && !src.is_dir() {
        return Err(anyhow!(trf(
            "{name} is neither a folder nor a supported mod archive (.zip, .7z, .rar)",
            &[("name", &src.display())]
        )));
    }
    if src.starts_with(content) && !job.from_inbox && !src.starts_with(content.join("Mods")) {
        return Err(anyhow!(trf(
            "{name} is already inside the content folder",
            &[("name", &src.display())]
        )));
    }
    // (the name becomes a folder name: `...zip` has the stem `..`)
    let source_name = if archive.is_some() {
        src.file_stem()
    } else {
        src.file_name()
    }
    .map(|s| s.to_string_lossy().to_string())
    .filter(|n| safe_rel(n).as_deref() == Some(n.as_str()))
    .unwrap_or_else(|| "mod".into());
    let (mut source, mut entries) = if staged_archive {
        let unpacked = staging_dir(content, job.id).join("source");
        job.state(
            "unpacking",
            tr("checking the archive and unpacking it into staging"),
        );
        unpack_archive(job, content, &src, &unpacked, archive.unwrap())?;
        let (entries, paths, refused) = list_folder(&unpacked, job)?;
        if refused > 0 {
            job.set(|p| {
                p.warnings.push(trf(
                    "{refused} file(s) whose names cannot be installed safely were left out",
                    &[("refused", &refused)],
                ))
            });
        }
        (Source::Folder(paths), entries)
    } else if is_zip {
        job.state("planning", tr("reading the archive's table of contents"));
        let f = std::fs::File::open(&src).with_context(|| format!("opening {}", src.display()))?;
        let mut z = zip::ZipArchive::new(std::io::BufReader::with_capacity(ZIP_BUFFER, f))
            .with_context(|| {
                trf(
                    "{name} is not a readable zip archive",
                    &[("name", &src.display())],
                )
            })?;
        let (entries, problems) = list_zip(&mut z, job)?;
        if !problems.is_empty() {
            return Err(anyhow!("{}: {}", src.display(), problems.join("; ")));
        }
        (Source::Zip(z), entries)
    } else {
        job.state("planning", tr("listing the folder"));
        let (entries, paths, refused) = list_folder(&src, job)?;
        if refused > 0 {
            job.set(|p| p.warnings.push(trf("{refused} file(s) whose names cannot be installed safely (a '..', '\\' or ':' in them) were left out", &[("refused", &refused)])));
        }
        (Source::Folder(paths), entries)
    };
    if entries.is_empty() {
        return Err(anyhow!(trf("{name} is empty", &[("name", &src.display())])));
    }
    proton::read_layouts(&mut source, &mut entries, job)?;
    let installed = Installed { content, root };
    let mut plan = plan(&entries, &source_name, &installed);
    proton::validate_selected(&plan, &entries)?;
    if plan.maps.is_empty() {
        return Err(anyhow!(trf(
            "could not tell what {name} is: no Vehicles / maps / Sceneryobjects ... folders and no .bus / .sco / .sli / global.cfg files in it",
            &[("name", &src.display())]
        )));
    }
    for m in &plan.maps {
        let line = format!(
            "{} -> {}{}/ ({} files, {}){}",
            if m.src.is_empty() {
                source_name.as_str()
            } else {
                m.src.as_str()
            },
            if m.aside {
                format!("Mods/{WAITING}/{source_name}/")
            } else {
                String::new()
            },
            m.dest,
            m.files.len(),
            gb(m.bytes),
            if m.what.is_empty() {
                String::new()
            } else {
                format!(" - {}", m.what)
            }
        );
        job.set(|p| p.report.push(line.clone()));
    }
    job.set(|p| {
        p.report.extend(plan.notes.iter().cloned());
        p.warnings.extend(plan.warnings.iter().cloned());
    });
    // 2. does it fit?
    job.state("checking", tr("checking the free disk space"));
    let total_files: u64 = plan.maps.iter().map(|m| m.files.len() as u64).sum();
    let total_bytes: u64 = plan.maps.iter().map(|m| m.bytes).sum();
    // an inbox folder's files are hard-linked into the staging folder (no space, and the
    // originals stay until the install is in place); everything else is written anew
    let moving = (job.from_inbox && !is_zip) || staged_archive;
    let needed = (if moving {
        0
    } else {
        total_bytes + (total_bytes / 20).max(MIN_MARGIN)
    })
    .saturating_add(proton::extra_space(&plan));
    let free = free_space(content).unwrap_or(u64::MAX);
    job.set(|p| {
        p.files_total = total_files;
        p.bytes_total = total_bytes;
        p.free_bytes = free;
        p.needed_bytes = needed;
    });
    // unpack, or use the archive in place?
    let layout = if is_zip {
        in_place_layout(&src, &plan)
    } else {
        Err(anyhow!(tr(
            "only a .zip archive can be used in place; this archive must be unpacked"
        )))
    };
    let mode = if staged_archive {
        InstallMode::Extract
    } else {
        match job.mode {
            InstallMode::Auto if is_zip && needed > free => match &layout {
                Ok(()) => {
                    let line = format!(
                        "{} unpacks to {}, which does not fit ({} free): using the archive in place instead",
                        source_name,
                        gb(total_bytes),
                        gb(free)
                    );
                    job.set(|p| p.report.push(line.clone()));
                    InstallMode::InPlace
                }
                Err(e) => {
                    let line = trf(
                        "the archive cannot be used in place either: {e}",
                        &[("e", &format!("{e:#}"))],
                    );
                    job.set(|p| p.warnings.push(line.clone()));
                    InstallMode::Extract
                }
            },
            InstallMode::Auto => InstallMode::Extract,
            m => m,
        }
    };
    job.set(|p| p.mode = mode);
    proton::check_mode(&plan, mode)?;
    if mode == InstallMode::InPlace {
        layout?;
        drop(source);
        return place_archive(job, content, &src, &plan);
    }
    if needed > free {
        return Err(anyhow!(
            "not enough disk space: {} unpacks to {} ({} files), and with {} kept free that needs {}, but only {} is free on the disk of {}. Nothing was unpacked - free some space and try again.",
            source_name,
            gb(total_bytes),
            total_files,
            gb(needed - total_bytes),
            gb(needed),
            gb(free),
            content.display()
        ));
    }
    // 3. unpack / copy into the staging folder
    let staging = staging_dir(content, job.id);
    std::fs::create_dir_all(&staging).with_context(|| format!("creating {}", staging.display()))?;
    job.state(
        if is_zip {
            "unpacking"
        } else if moving {
            "moving"
        } else {
            "copying"
        },
        trf(
            "{files} files, {size}",
            &[("files", &total_files), ("size", &gb(total_bytes))],
        ),
    );
    unpack_staging(job, &mut source, &entries, &plan, &staging, content, moving)?;
    drop(source);
    proton::convert(&staging, &mut plan, job)?;
    if job.cancelled() {
        return Err(anyhow!(Cancelled));
    }
    commit_staging(job, content, &src, &source_name, &plan, &staging)
}

/// Copy selected source files into an isolated staging directory, without publishing them.
fn unpack_staging(job: &Job, source: &mut Source, entries: &[Entry], plan: &Plan, staging: &Path, content: &Path, moving: bool) -> Result<()> {
    let mut buf = vec![0u8; 1 << 20];
    let mut since_check = 0u64;
    for (mi, m) in plan.maps.iter().enumerate() {
        let base = staging.join(format!("{mi}"));
        for (idx, rel) in &m.files {
            if job.cancelled() {
                return Err(anyhow!(Cancelled));
            }
            check_rel(rel)?;
            let out = base.join(rel);
            if let Some(d) = out.parent() {
                std::fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
            }
            // two entries of the same name: the later one wins, and it must not write
            // through a link to an inbox file
            if std::fs::symlink_metadata(&out).is_ok() {
                std::fs::remove_file(&out)
                    .with_context(|| format!("replacing {}", out.display()))?;
            }
            let e = &entries[*idx];
            match source {
                Source::Zip(z) => {
                    let mut f = z
                        .by_index(e.index)
                        .with_context(|| format!("reading {} from the archive", e.rel))?;
                    let mut w = std::fs::File::create(&out)
                        .with_context(|| format!("writing {}", out.display()))?;
                    loop {
                        let n = f.read(&mut buf).with_context(|| {
                            trf(
                                "unpacking {name} (the archive may be damaged)",
                                &[("name", &e.rel)],
                            )
                        })?;
                        if n == 0 {
                            break;
                        }
                        w.write_all(&buf[..n])
                            .with_context(|| format!("writing {}", out.display()))?;
                        job.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
                        since_check += n as u64;
                        if since_check > 64 << 20 {
                            since_check = 0;
                            if job.cancelled() {
                                return Err(anyhow!(Cancelled));
                            }
                            check_space(content)?;
                        }
                    }
                }
                Source::Folder(paths) => {
                    let from = &paths[e.index];
                    if moving && std::fs::hard_link(from, &out).is_ok() {
                        job.linked.lock().unwrap().push(from.clone());
                        job.bytes_done.fetch_add(e.size, Ordering::Relaxed);
                    } else {
                        let mut r = std::fs::File::open(from)
                            .with_context(|| format!("reading {}", from.display()))?;
                        let mut w = std::fs::File::create(&out)
                            .with_context(|| format!("writing {}", out.display()))?;
                        loop {
                            let n = r.read(&mut buf)?;
                            if n == 0 {
                                break;
                            }
                            w.write_all(&buf[..n])
                                .with_context(|| format!("writing {}", out.display()))?;
                            job.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
                            since_check += n as u64;
                            if since_check > 64 << 20 {
                                since_check = 0;
                                if job.cancelled() {
                                    return Err(anyhow!(Cancelled));
                                }
                                check_space(content)?;
                            }
                        }
                    }
                }
            }
            if job.files_done.fetch_add(1, Ordering::Relaxed) + 1 == job.stall_at {
                while !job.cancelled() {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        }
    }
    Ok(())
}

/// Atomically integrate converted assets with the ordinary Mods ownership records.
fn commit_staging(job: &Job, content: &Path, src: &Path, source_name: &str, plan: &Plan, staging: &Path) -> Result<()> {
    // 4. into place
    job.state("moving", tr("moving the files into the content folder"));
    let mut installed_items = Vec::new();
    let mut aside_items = Vec::new();
    // (the folders this install made: they go again when the mod is taken out of
    // Mods/installed, see `uninstall_removed`)
    let mut created = Vec::new();
    for (mi, m) in plan.maps.iter().enumerate() {
        let from = staging.join(format!("{mi}"));
        if !from.exists() {
            continue;
        }
        check_rel(&m.dest)?;
        let dest = if m.aside {
            content
                .join("Mods")
                .join(WAITING)
                .join(&source_name)
                .join(&m.dest)
        } else {
            case_path(content, &m.dest)
        };
        if !m.aside && !dest.exists() && m.dest.contains('/') {
            if let Ok(rel) = dest.strip_prefix(content) {
                created.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        // (into a folder all mods share - Fonts, Texture, Sounds: what of it is this mod's,
        // each new file and folder at its top. Noted as the folder alone they were nobody's:
        // the list showed no fonts or textures of a mod, and they stayed when it went)
        if !m.aside && !m.dest.contains('/') {
            let mut tops: Vec<String> = m
                .files
                .iter()
                .filter_map(|(_, rel)| rel.split(['/', '\\']).next().map(str::to_string))
                .filter(|t| !t.is_empty())
                .collect();
            tops.sort_by_key(|t| t.to_ascii_lowercase());
            tops.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            for t in tops {
                if !case_path(&dest, &t).exists() {
                    if let Ok(rel) = case_path(&dest, &t).strip_prefix(content) {
                        created.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        let replaced =
            move_into(&from, &dest).with_context(|| format!("moving into {}", dest.display()))?;
        if replaced > 0 {
            let line = format!("{}: {replaced} existing file(s) replaced", m.dest);
            job.set(|p| p.report.push(line.clone()));
        }
        if m.aside {
            aside_items.push(format!("{source_name} ({})", m.dest));
        } else {
            installed_items.push(m.dest.clone());
        }
    }
    // the linked inbox files are installed now: they leave the inbox, and what is left of
    // it goes to Mods/installed
    for original in std::mem::take(&mut *job.linked.lock().unwrap()) {
        let _ = std::fs::remove_file(&original);
    }
    if job.from_inbox {
        let done = content.join("Mods").join("installed");
        let _ = std::fs::create_dir_all(&done);
        let target = done.join(src.file_name().unwrap_or_default());
        let _ = std::fs::remove_dir_all(&target);
        let _ = std::fs::remove_file(&target);
        if std::fs::rename(&src, &target).is_err() {
            if src.is_dir() {
                let _ = std::fs::remove_dir_all(&src);
            }
        } else {
            note_installed(
                content,
                &src.file_name().unwrap_or_default().to_string_lossy(),
                &created,
            );
        }
    }
    // (noted as one mod: its own folders, see `mods`)
    crate::mods::record(content, &source_name, &created);
    let summary = match (installed_items.is_empty(), aside_items.is_empty()) {
        (false, true) => trf(
            "installed {items} - it is in the lists now",
            &[("items", &installed_items.join(", "))],
        ),
        (false, false) => trf(
            "installed {items}; kept aside: {aside}",
            &[
                ("items", &installed_items.join(", ")),
                ("aside", &aside_items.join(", ")),
            ],
        ),
        (true, _) => trf(
            "nothing installed; kept aside: {aside}",
            &[("aside", &aside_items.join(", "))],
        ),
    };
    job.set(|p| {
        p.installed = installed_items.clone();
        p.kept_aside = aside_items.clone();
        p.message = summary.clone();
        p.report.push(summary.clone());
    });
    Ok(())
}
