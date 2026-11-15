//! Bounded parameters shared by the optional gameplay converters.
use crate::Ini;
use anyhow::{ensure, Result};

pub(crate) fn value<'a>(cfg: &'a Ini, section: &str, key: &str) -> Option<&'a str> {
    cfg.get(section)
        .and_then(|s| s.get(key))
        .map(String::as_str)
        .filter(|v| !v.is_empty())
}

pub(crate) fn number(cfg: &Ini, section: &str, key: &str, default: f32) -> Result<f32> {
    let n = value(cfg, section, key)
        .map(str::parse::<f32>)
        .transpose()?
        .unwrap_or(default);
    ensure!(n.is_finite(), "invalid Proton parameter {section}.{key}");
    Ok(n)
}

pub(crate) fn position(cfg: &Ini, section: &str) -> Result<[f32; 3]> {
    let p = [
        number(cfg, section, "posx", 0.0)?,
        number(cfg, section, "posz", 0.0)?,
        number(cfg, section, "posy", 0.0)?,
    ];
    ensure!(
        p.iter().all(|v| v.abs() < 1_000_000.0),
        "invalid Proton coordinate"
    );
    Ok(p)
}

pub(crate) fn label(s: &str) -> Result<String> {
    ensure!(
        s.len() <= 1024 && !s.contains(['\n', '\r', '\0']),
        "invalid Proton label"
    );
    Ok(s.to_owned())
}
