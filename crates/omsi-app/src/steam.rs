//! Steamworks status integration. This is independent of Discord's presence.

use std::cell::RefCell;

pub struct Steam {
    pub client: steamworks::Client,
    last_status: RefCell<String>,
}

impl Steam {
    pub fn start() -> Option<Steam> {
        let client = steamworks::Client::init_app(252530).ok()?;
        log::info!("Steam: Connected");
        Some(Steam {
            client,
            last_status: RefCell::new(String::new()),
        })
    }

    /// The public text in Steam's friends list, without local file paths.
    fn status(map: Option<&str>, bus: Option<&str>, line: Option<&str>, multiplayer: bool) -> String {
        let mut status = match map {
            Some(map) if !map.trim().is_empty() => format!("Map: {}", map.trim()),
            _ => "In the launcher".to_string(),
        };
        if let Some(bus) = bus.filter(|name| !name.trim().is_empty()) {
            status.push_str(" | Bus: ");
            status.push_str(bus.trim());
        }
        if let Some(line) = line.filter(|line| !line.trim().is_empty()) {
            status.push_str(" | Line: ");
            status.push_str(line.trim());
        }
        if multiplayer {
            status.push_str(" | Multiplayer");
        }
        // Steamworks uses C strings and caps rich presence values at 256 bytes.
        status.retain(|c| c != '\0');
        while status.len() > 200 {
            status.pop();
        }
        status
    }

    /// Publish only when something meaningful changed, not on every game frame.
    pub fn set_presence(&self, map: Option<&str>, bus: Option<&str>, line: Option<&str>, multiplayer: bool) {
        let status = Self::status(map, bus, line, multiplayer);
        if *self.last_status.borrow() == status {
            return;
        }
        if self.client.friends().set_rich_presence("status", Some(&status)) {
            *self.last_status.borrow_mut() = status;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Steam;

    #[test]
    fn status_shows_the_game_and_lan_without_paths() {
        assert_eq!(
            Steam::status(Some("Berlin-Spandau"), Some("SD200"), Some("92"), true),
            "Map: Berlin-Spandau | Bus: SD200 | Line: 92 | Multiplayer"
        );
    }

    #[test]
    fn status_rejects_nul_and_respects_the_steam_limit() {
        let source = format!("Bus\0{}", "ä".repeat(200));
        let status = Steam::status(None, Some(&source), None, false);
        assert!(!status.contains('\0'));
        assert!(status.len() <= 200);
        assert_eq!(Steam::status(None, None, None, false), "In the launcher");
    }
}
