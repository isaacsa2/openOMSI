/// A new bus starts with a full tank unless the player turned it off; the launcher's own
/// save keeps the choice (it does not know the key).
#[test]
fn the_full_tank_is_on_until_turned_off() {
    assert!(super::Settings::default().full_tank);
    assert!(super::Settings::from_text("version=4\n").full_tank);
    assert!(!super::Settings::from_text("full_tank=0\n").full_tank);
    let launcher = omsi_launcher_lib::settings_from_text(Some("full_tank=0\n"));
    let saved = omsi_launcher_lib::settings_to_text(&launcher, Some("full_tank=0\n"));
    assert!(!super::Settings::from_text(&saved).full_tank, "{saved}");
}
