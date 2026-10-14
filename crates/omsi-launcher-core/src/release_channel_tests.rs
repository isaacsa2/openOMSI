use super::*;

#[test]
fn subscription_defaults_and_round_trip() {
    assert_eq!(settings_from_text(None)["update_channel"], "stable");
    for (value, expected) in [("beta", "beta"), (" BETA ", "beta"), ("stable", "stable"), ("unknown", "stable")] {
        let input = format!("update_channel={value}\nfuture_setting=keep\n");
        let settings = settings_from_text(Some(&input));
        assert_eq!(settings["update_channel"], expected);
        let output = settings_to_text(&settings, Some(&input));
        assert_eq!(settings_from_text(Some(&output))["update_channel"], expected);
        assert!(output.lines().any(|line| line == "future_setting=keep"));
        assert_eq!(output.lines().filter(|line| line.starts_with("update_channel=")).count(), 1);
    }
}
