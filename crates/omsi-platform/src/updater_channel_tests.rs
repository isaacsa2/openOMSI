use super::*;

fn fixture(version: &str, beta: bool) -> serde_json::Value {
    serde_json::json!({
        "tag_name": format!("v{version}"), "draft": false, "prerelease": beta,
        "assets": [{"name": asset_name(version).unwrap(), "browser_download_url": "https://example.test/package.zip", "size": 42, "digest": format!("sha256:{}", "a".repeat(64))}]
    })
}

#[test]
fn channel_selection_skips_drafts_missing_platforms_and_pr_builds() {
    let stable = fixture("0.2.49", false);
    let beta = fixture("0.2.51", true);
    let mut missing = fixture("0.2.52", true);
    missing["assets"] = serde_json::json!([]);
    let mut draft = fixture("0.2.53", true);
    draft["draft"] = true.into();
    let catalogue = serde_json::json!([missing, stable, draft, beta]);
    assert_eq!(newest_release(&catalogue, "0.2.48", Channel::Stable).unwrap().unwrap().version, "0.2.49");
    assert_eq!(newest_release(&catalogue, "0.2.48", Channel::Beta).unwrap().unwrap().version, "0.2.51");
    assert!(newest_release(&catalogue, "0.2.51", Channel::Stable).unwrap().is_none());
    for channel in [Channel::Stable, Channel::Beta] {
        assert!(newest_release(&catalogue, "0.2.48-pr123", channel).unwrap().is_none());
    }
    assert_eq!(releases_on(&catalogue, Channel::Stable).len(), 1);
    assert_eq!(releases_on(&catalogue, Channel::Beta).len(), 2);
}

#[test]
fn promotion_keeps_the_same_version_url_and_checksum() {
    let mut value = fixture("0.2.51", true);
    let beta = parse_release_on(&value, "0.2.48", Channel::Beta).unwrap().unwrap();
    value["prerelease"] = false.into();
    let stable = parse_release_on(&value, "0.2.48", Channel::Stable).unwrap().unwrap();
    assert_eq!(beta.version, stable.version);
    assert_eq!(beta.asset_url, stable.asset_url);
    assert_eq!(beta.sha256, stable.sha256);
    assert!(parse_release_on(&value, "0.2.51", Channel::Stable).unwrap().is_none());
    assert!(parse_release_on(&value, "0.2.48", Channel::Beta).unwrap().is_some());
}

#[test]
fn an_old_channels_worker_cannot_restore_its_offer_after_switching() {
    let mut updater = Updater::default();
    updater.sync_to(Channel::Beta);
    let old_status = updater.status.clone();
    let release = parse_release_on(&fixture("0.2.51", true), "0.2.48", Channel::Beta).unwrap().unwrap();
    updater.sync_to(Channel::Stable);
    *lock(&old_status) = Status::Available(release.clone());
    assert_eq!(updater.status(), Status::Idle);
    assert!(!updater.channel.allows(release.channel));
}

#[test]
fn an_installation_in_progress_finishes_before_a_subscription_switch() {
    let mut updater = Updater::default();
    updater.sync_to(Channel::Beta);
    let release = parse_release_on(&fixture("0.2.51", true), "0.2.48", Channel::Beta).unwrap().unwrap();
    updater.set(Status::Installing(release));
    updater.sync_to(Channel::Stable);
    assert_eq!(updater.channel, Channel::Beta);
    assert!(matches!(updater.status(), Status::Installing(_)));
}

#[test]
fn a_download_finishes_before_another_channel_starts_one() {
    let mut updater = Updater::default();
    updater.sync_to(Channel::Beta);
    let release = parse_release_on(&fixture("0.2.51", true), "0.2.48", Channel::Beta).unwrap().unwrap();
    updater.set(Status::Downloading { release: release.clone(), done: 0, total: 42 });
    updater.sync_to(Channel::Stable);
    assert_eq!(updater.channel, Channel::Beta);
    updater.set(Status::Downloaded(release));
    updater.sync_to(Channel::Stable);
    assert_eq!(updater.channel, Channel::Stable);
    assert_eq!(updater.status(), Status::Idle);
}

#[test]
fn beta_catalogue_checks_all_pages_before_selecting_the_highest_version() {
    let fetch = |page| Ok(if page == 1 {
        serde_json::Value::Array((0..100).map(|_| fixture("0.2.49", true)).collect()).to_string()
    } else {
        serde_json::json!([fixture("0.2.51", false)]).to_string()
    });
    let list = releases_paged_on(fetch, Channel::Beta).unwrap();
    assert_eq!(list.first().unwrap().release.version, "0.2.51");
}
