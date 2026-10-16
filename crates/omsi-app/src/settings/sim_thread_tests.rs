/// The simulation goes beside the picture unless the player turned it off; the launcher's
/// own save keeps the choice (it does not know the key).
#[test]
fn the_sim_thread_is_on_until_turned_off() {
    assert!(super::Settings::default().sim_thread);
    assert!(super::Settings::from_text("version=4\n").sim_thread);
    assert!(!super::Settings::from_text("sim_thread=0\n").sim_thread);
    let launcher = omsi_launcher_lib::settings_from_text(Some("sim_thread=0\n"));
    let saved = omsi_launcher_lib::settings_to_text(&launcher, Some("sim_thread=0\n"));
    assert!(!super::Settings::from_text(&saved).sim_thread, "{saved}");
}
