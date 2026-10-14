//! A subscription channel, independent of the game's Alpha/Beta development stage.

pub(crate) const INSTALLED: &str = "openomsi-release.json";
pub(crate) const PREVIOUS_CHANNEL: &str = ".openomsi-previous-channel";
pub(crate) const LAST_LEGACY_STABLE: &str = "0.2.48";

/// Pre-channel releases were all normal releases. New Beta archives carry this marker
/// also when extracted by hand, so switching subscriptions cannot restore a Beta.
pub(crate) fn installed_at(dir: &std::path::Path, version: &str) -> Channel {
    match std::fs::read_to_string(dir.join(INSTALLED)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound =>
            if crate::updater::newer(version, LAST_LEGACY_STABLE) { Channel::Beta } else { Channel::Stable },
        Err(_) => Channel::Beta,
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) if value["version"].as_str() == Some(version) && value["channel"].as_str() == Some("stable") => Channel::Stable,
            _ => Channel::Beta,
        },
    }
}

pub(crate) fn remember_installed(dir: &std::path::Path, release: &crate::updater::Release) {
    let metadata = serde_json::json!({"version": release.version, "channel": release.channel.as_str()});
    if let Err(error) = crate::recovery::write_atomic(&dir.join(INSTALLED), &metadata.to_string()) {
        log::warn!("could not remember the installed release channel: {error}");
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

impl Channel {
    pub fn from_setting(value: &str) -> Self {
        if value.trim().eq_ignore_ascii_case("beta") { Self::Beta } else { Self::Stable }
    }

    pub fn as_str(self) -> &'static str {
        match self { Self::Stable => "stable", Self::Beta => "beta" }
    }

    pub fn label(self) -> &'static str {
        match self { Self::Stable => "Stable", Self::Beta => "Beta" }
    }

    pub fn allows(self, release: Self) -> bool {
        self == Self::Beta || release == Self::Stable
    }

    pub fn accepts(self, release: &serde_json::Value) -> bool {
        release["draft"].as_bool() != Some(true)
            && self.allows(if release["prerelease"].as_bool() == Some(true) { Self::Beta } else { Self::Stable })
    }

    /// Read on each check/install so the game, launcher and start guard agree.
    pub fn configured() -> Self {
        let text = std::fs::read_to_string(omsi_launcher_lib::data_dir().join("settings.cfg")).ok();
        let settings = omsi_launcher_lib::settings_from_text(text.as_deref());
        Self::from_setting(settings["update_channel"].as_str().unwrap_or("stable"))
    }

    pub fn downloads_page(self) -> String {
        format!("{}/releases{}", crate::updater::REPO_URL, if self == Self::Stable { "/latest" } else { "" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_invalid_settings_keep_stable() {
        for value in ["", "stable", "preview", "beta-extra"] {
            assert_eq!(Channel::from_setting(value), Channel::Stable);
        }
        assert_eq!(Channel::from_setting(" BETA "), Channel::Beta);
    }

    #[test]
    fn beta_includes_promoted_builds_but_neither_channel_accepts_drafts() {
        let mut release = serde_json::json!({"draft": false, "prerelease": true});
        assert!(!Channel::Stable.accepts(&release));
        assert!(Channel::Beta.accepts(&release));
        release["prerelease"] = false.into();
        assert!(Channel::Stable.accepts(&release));
        assert!(Channel::Beta.accepts(&release));
        release["draft"] = true.into();
        assert!(!Channel::Stable.accepts(&release));
        assert!(!Channel::Beta.accepts(&release));
    }

    #[test]
    fn unmarked_future_builds_and_invalid_markers_are_conservatively_beta() {
        let dir = std::env::temp_dir().join(format!("openomsi-channel-marker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(installed_at(&dir, "0.2.48"), Channel::Stable);
        assert_eq!(installed_at(&dir, "0.2.49"), Channel::Beta);
        std::fs::write(dir.join(INSTALLED), "not JSON").unwrap();
        assert_eq!(installed_at(&dir, "0.2.49"), Channel::Beta);
        std::fs::write(dir.join(INSTALLED), r#"{"version":"0.2.49","channel":"stable"}"#).unwrap();
        assert_eq!(installed_at(&dir, "0.2.49"), Channel::Stable);
        assert_eq!(installed_at(&dir, "0.2.50"), Channel::Beta);
        let _ = std::fs::remove_dir_all(dir);
    }
}
