//! A small native recovery launcher. It deliberately uses the same validated duty
//! and process launch path as the web frontend, without loading a webview or renderer.

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::{Duty, diagnostic_log};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::path::{Path, PathBuf};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn relative_content(path: &Path, roots: &[PathBuf], folder: &str) -> anyhow::Result<String> {
    let path = path.canonicalize()?;
    for root in roots {
        let Ok(root) = root.canonicalize() else { continue };
        let Ok(rel) = path.strip_prefix(root) else { continue };
        if rel.components().next().is_some_and(|c| c.as_os_str().eq_ignore_ascii_case(folder)) {
            return Ok(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    anyhow::bail!("Choose a file from the original OMSI or openOMSI content folder ({folder}).")
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn open() -> anyhow::Result<()> {
    use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult};

    diagnostic_log::launcher("INFO", "classic", "native recovery launcher opened");
    let root = match crate::root_dir() {
        Ok(root) => root,
        Err(_) => {
            let Some(root) = FileDialog::new().set_title("Select your original OMSI 2 installation").pick_folder() else {
                return Ok(());
            };
            if !crate::is_omsi_root(&root) {
                anyhow::bail!("This folder is not a complete original OMSI 2 installation.");
            }
            let mut config = crate::load_config();
            config.root = root.to_string_lossy().into_owned();
            crate::save_config(&config)?;
            root
        }
    };
    let mut roots = vec![root.clone()];
    if let Some(content) = crate::content_dir() {
        roots.push(content);
    }
    let Some(map) = FileDialog::new()
        .set_title("Select a map (global.cfg)")
        .set_directory(root.join("maps"))
        .add_filter("OMSI map", &["cfg"])
        .pick_file() else { return Ok(()) };
    if !map.file_name().is_some_and(|n| n.eq_ignore_ascii_case("global.cfg")) {
        anyhow::bail!("Select the map's global.cfg file.");
    }
    let map = relative_content(&map, &roots, "maps")?;
    let Some(bus) = FileDialog::new()
        .set_title("Select a bus")
        .set_directory(root.join("Vehicles"))
        .add_filter("OMSI bus", &["bus"])
        .pick_file() else { return Ok(()) };
    if !bus.extension().is_some_and(|n| n.eq_ignore_ascii_case("bus")) {
        anyhow::bail!("Select an OMSI .bus file.");
    }
    let bus = relative_content(&bus, &roots, "Vehicles")?;
    if MessageDialog::new()
        .set_title("openOMSI - Classic launcher")
        .set_description(format!("Start free drive?\n\nMap: {map}\nBus: {bus}\n\nThe existing game settings and driver profile will be used."))
        .set_buttons(MessageButtons::YesNo)
        .show() != MessageDialogResult::Yes {
        return Ok(());
    }
    let duty = Duty {
        map,
        bus,
        time: "09:00".into(),
        profile: Some(crate::load_config().profile),
        ..Default::default()
    };
    let launched = crate::launch(&duty)?;
    diagnostic_log::launcher("INFO", "classic", &format!("game pid={} started", launched.pid));
    Ok(())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn open() -> anyhow::Result<()> {
    anyhow::bail!("The native recovery launcher is available on desktop only.")
}
