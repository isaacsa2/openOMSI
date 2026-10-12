//! Bounded native sound loops using authored files, locations and volume/pitch curves.
use crate::{
    child, ini,
    settings::{number, position, value},
    vehicle::Inspection,
    vehicle_package::Report,
};
use anyhow::{ensure, Result};
use std::path::Path;
pub(crate) fn export(info: &Inspection, target: &Path, report: &mut Report) -> Result<String> {
    let mut out = String::new();
    std::fs::create_dir_all(target.join("Sound"))?;
    if let Some(c) = info
        .components
        .iter()
        .find(|c| c.role == "sounds1" && c.present)
    {
        let cfg = ini(&c.file)?;
        for (section, _) in cfg.iter().filter(|(n, _)| {
            n.strip_prefix("sound")
                .is_some_and(|s| s.parse::<usize>().is_ok())
        }) {
            ensure!(report.sounds < 256, "too many vehicle sounds");
            let Some(file) = value(&cfg, section, "file") else {
                continue;
            };
            // PBSU paths can name a sound subfolder relative to baseDir or sounds folder.
            let source = resolve(info, file)?;
            if !source.is_file() {
                report
                    .warnings
                    .push(format!("missing vehicle sound {file}"));
                continue;
            }
            let ext = source
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            if !["wav", "ogg"].contains(&ext.as_str()) {
                report.warnings.push(format!("unsupported sound {file}"));
                continue;
            }
            let name = format!("loop_{}.{}", report.sounds, ext);
            std::fs::copy(&source, target.join("Sound").join(&name))?;
            let pos = if number(&cfg, section, "useposengine", 0.)? != 0. {
                position(&cfg, "engine_location")?
            } else {
                position(&cfg, section)?
            };
            let volume = number(&cfg, section, "volumemultiplier", 1.)?.clamp(0., 10.);
            let pitch = number(&cfg, section, "pitchmultiplier", 1.)?.clamp(0.1, 10.);
            let maxrpm = number(&cfg, "sound_manager", "max_rpm", 2600.)?.clamp(500., 6000.);
            out.push_str(&format!("[loopsound]\n{name}\n44100\nengine_n\n{}\n{volume}\n[3d]\n{}\n{}\n{}\n{}\n[conditionBool]\nengine_on\n1\n",maxrpm/pitch,pos[0],pos[1],pos[2],number(&cfg,section,"maxdist",30.)?.clamp(1.,200.)));
            let view = match value(&cfg, section, "whentoplay").unwrap_or("allCams") {
                "driverCam" => 2,
                "externalCam" => 5,
                "passengerCam" => 2,
                _ => 0,
            };
            out.push_str(&format!("[viewpoint]\n{view}\n"));
            for (prefix, var) in [("v", "oi_rpm_norm"), ("p", "oi_rpm_norm")] {
                if prefix == "p" {
                    continue;
                } // Native loop pitch uses engine_n; custom pitch curves remain an approximation.
                let mut curve = Vec::new();
                for i in 1..=16 {
                    let x = format!("{prefix}{i}x");
                    let y = format!("{prefix}{i}y");
                    if value(&cfg, section, &x).is_none() || value(&cfg, section, &y).is_none() {
                        break;
                    }
                    curve.push((
                        number(&cfg, section, &x, 0.)?,
                        number(&cfg, section, &y, 0.)?.clamp(0., 10.),
                    ));
                }
                curve.sort_by(|a, b| a.0.total_cmp(&b.0));
                curve.dedup_by(|a, b| a.0 == b.0);
                if !curve.is_empty() {
                    out.push_str(&format!("[volcurve]\n{var}\n"));
                    for (x, y) in curve {
                        out.push_str(&format!("[pnt]\n{x}\n{y}\n"));
                    }
                }
            }
            if number(&cfg, section, "playonlywhenaccelerating", 0.)? != 0. {
                out.push_str("[conditionSingle]\nThrottle\n0.05\n3\n");
            }
            if number(&cfg, section, "playonlyonselectedgears", 0.)? != 0. {
                out.push_str(&format!(
                    "[conditionSingle]\noi_gear\n{}\n5\n[conditionSingle]\noi_gear\n{}\n4\n",
                    number(&cfg, section, "mingeartoplay", 1.)?,
                    number(&cfg, section, "maxgeartoplay", 16.)?
                ));
            }
            report.sounds += 1;
        }
    }
    if let Some(c) = info
        .components
        .iter()
        .find(|c| c.role == "sounds1" && c.present)
    {
        let cfg = ini(&c.file)?;
        sound_loop(info, target, &cfg, "horn", "horn", 1., report, &mut out)?;
    }
    for (i, door) in info.doors.iter().enumerate() {
        let c = info
            .components
            .iter()
            .find(|c| c.role == door.name)
            .unwrap();
        let cfg = ini(&c.file)?;
        if number(&cfg, "doorsound_air", "enabled", 0.)? != 0. {
            sound_loop(
                info,
                target,
                &cfg,
                "doorsound_air",
                &format!("oi_door_{i}"),
                0.95,
                report,
                &mut out,
            )?;
        }
    }
    report.warnings.push("Sound volume/RPM curves, camera masks and gear/throttle conditions imported; custom pitch curves, retarder/turbo filters and event-specific sound semantics remain approximate".into());
    Ok(out)
}

fn resolve(info: &Inspection, file: &str) -> Result<std::path::PathBuf> {
    let source = child(&info.base, file)?;
    if source.is_file() {
        return Ok(source);
    }
    for folder in ["sons", "sounds", "sound"] {
        let dir = child(&info.base, folder)?;
        if dir.is_dir() {
            let path = child(&dir, file)?;
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Ok(source)
}
fn sound_loop(
    info: &Inspection,
    target: &Path,
    cfg: &crate::Ini,
    section: &str,
    var: &str,
    threshold: f32,
    report: &mut Report,
    out: &mut String,
) -> Result<()> {
    let Some(file) = value(cfg, section, "file") else {
        return Ok(());
    };
    let source = resolve(info, file)?;
    if !source.is_file() {
        report
            .warnings
            .push(format!("missing vehicle {section} sound {file}"));
        return Ok(());
    }
    let ext = source
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if !["wav", "ogg"].contains(&ext.as_str()) {
        return Ok(());
    }
    ensure!(report.sounds < 256, "too many vehicle sounds");
    let name = format!("loop_{}.{}", report.sounds, ext);
    std::fs::copy(&source, target.join("Sound").join(&name))?;
    let p = position(cfg, section)?;
    let volume = number(cfg, section, "volume", 1.)?.clamp(0., 10.);
    let pitch = number(cfg, section, "pitch", 1.)?.clamp(0.1, 10.);
    out.push_str(&format!("[loopsound]\n{name}\n44100\nelec_busbar_main\n{}\n{volume}\n[3d]\n{}\n{}\n{}\n{}\n[conditionSingle]\n{var}\n{threshold}\n5\n",1./pitch,p[0],p[1],p[2],number(cfg,section,"maxdist",30.)?.clamp(1.,200.)));
    report.sounds += 1;
    Ok(())
}
