//! Regression tests for the optional single-player passenger connections setting.
use super::Settings;

#[test]
fn default_is_off_for_legacy_desktop_and_mobile() {
    assert!(!Settings::phone().passenger_connections);
    assert!(!Settings::desktop().passenger_connections);
    for src in ["", "version=1\n", "passenger_connections=0\n"] {
        let s = Settings::from_text(src);
        assert!(!s.passenger_connections);
        assert_eq!(s.passenger_transfer_percent, 20.0);
    }
}

#[test]
fn chance_clamps_and_never_implicitly_enables() {
    for (input, expected) in [
        ("0", 0.0),
        ("20", 20.0),
        ("100", 100.0),
        ("999", 100.0),
        ("-10", 0.0),
        ("nan", 20.0),
    ] {
        let s = Settings::from_text(&format!("passenger_transfer_percent={input}\n"));
        assert_eq!(s.passenger_transfer_percent, expected);
        assert!(!s.passenger_connections);
    }
}

#[test]
fn both_settings_writers_preserve_optional_preference_and_chance() {
    let key = "passenger_connections";
    for phone in [false, true] {
        assert_eq!(
            omsi_launcher_lib::settings_from_text_on(None, phone)[key],
            false
        );
        for on in [false, true, false] {
            let mut settings = Settings::default();
            settings.passenger_connections = on;
            settings.passenger_transfer_percent = 100.0;
            let native = settings.to_text();
            let mut launcher = omsi_launcher_lib::settings_from_text_on(Some(&native), phone);
            assert_eq!(launcher[key], on);
            assert_eq!(launcher["passenger_transfer_percent"], 100.0);
            launcher["passenger_transfer_percent"] = serde_json::json!(20.0);
            let text = omsi_launcher_lib::settings_to_text(&launcher, Some(&native));
            let restored = Settings::from_text(&text);
            assert_eq!(restored.passenger_connections, on);
            assert_eq!(restored.passenger_transfer_percent, 20.0);
            assert_eq!(
                text.lines()
                    .filter(|line| line.starts_with(&format!("{key}=")))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn all_passenger_options_remain_independent_across_both_writers() {
    for phone in [false, true] {
        for bits in 0..8 {
            let original = Settings {
                passenger_hailing: bits & 1 != 0,
                passenger_full_bus_reaction: bits & 2 != 0,
                passenger_connections: bits & 4 != 0,
                passenger_transfer_percent: 35.0,
                ..Default::default()
            };
            let native = original.to_text();
            let launcher = omsi_launcher_lib::settings_from_text_on(Some(&native), phone);
            let text = omsi_launcher_lib::settings_to_text(&launcher, Some(&native));
            let restored = Settings::from_text(&text);
            assert_eq!(restored.passenger_hailing, original.passenger_hailing);
            assert_eq!(
                restored.passenger_full_bus_reaction,
                original.passenger_full_bus_reaction
            );
            assert_eq!(
                restored.passenger_connections,
                original.passenger_connections
            );
            assert_eq!(restored.passenger_transfer_percent, 35.0);
            for key in [
                "passenger_hailing",
                "passenger_full_bus_reaction",
                "passenger_connections",
                "passenger_transfer_percent",
            ] {
                assert_eq!(
                    text.lines()
                        .filter(|line| line.starts_with(&format!("{key}=")))
                        .count(),
                    1
                );
            }
        }
    }
}

/// Patience and passengers by the hour: off by default (desktop, phone, an old file), read
/// and written by the game and by the launcher, each once, independently of each other.
#[test]
fn patience_and_by_hour_round_trip_through_both_writers() {
    for s in [Settings::desktop(), Settings::phone(), Settings::from_text("version=1\n")] {
        assert!(!s.pax_patience && !s.pax_by_hour);
    }
    for phone in [false, true] {
        for bits in 0..4 {
            let original = Settings { pax_patience: bits & 1 != 0, pax_by_hour: bits & 2 != 0, ..Default::default() };
            let native = original.to_text();
            let back = Settings::from_text(&native);
            assert_eq!((back.pax_patience, back.pax_by_hour), (original.pax_patience, original.pax_by_hour), "the game's own file");
            let launcher = omsi_launcher_lib::settings_from_text_on(Some(&native), phone);
            assert_eq!(launcher["pax_patience"].as_bool(), Some(original.pax_patience), "the launcher reads it");
            assert_eq!(launcher["pax_by_hour"].as_bool(), Some(original.pax_by_hour));
            let text = omsi_launcher_lib::settings_to_text(&launcher, Some(&native));
            let restored = Settings::from_text(&text);
            assert_eq!((restored.pax_patience, restored.pax_by_hour), (original.pax_patience, original.pax_by_hour), "through the launcher");
            for key in ["pax_patience", "pax_by_hour"] {
                assert_eq!(text.lines().filter(|l| l.starts_with(&format!("{key}="))).count(), 1, "{key}");
            }
        }
    }
}
