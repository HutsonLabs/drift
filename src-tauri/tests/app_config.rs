//! M1-6 / M6-2 / M9-3 Red: launch options and bundle configuration.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test fixtures

use std::path::PathBuf;

use drift_app::RunOptions;

fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
}

#[test]
fn autoconnect_and_config_dir_come_from_the_environment() {
    let o = RunOptions::from_vars(vars(&[
        ("DRIFT_AUTOCONNECT", "Homelab headless"),
        ("DRIFT_CONFIG_DIR", "/tmp/drift-smoke"),
        ("HOME", "/Users/x"),
    ]));
    assert_eq!(o.autoconnect.as_deref(), Some("Homelab headless"));
    assert_eq!(o.config_dir, Some(PathBuf::from("/tmp/drift-smoke")));
    assert!(!o.memory_secrets, "the environment can never switch off the Keychain");
    assert!(o.on_ready.is_none());
}

#[test]
fn blank_or_missing_variables_are_ignored() {
    let o = RunOptions::from_vars(vars(&[("DRIFT_AUTOCONNECT", "  "), ("DRIFT_CONFIG_DIR", "")]));
    assert!(o.autoconnect.is_none() && o.config_dir.is_none());
    let o = RunOptions::from_vars(vars(&[("DRIFT_MEMORY_SECRETS", "1")]));
    assert!(!o.memory_secrets);
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).unwrap()
}

#[test]
fn info_plist_declares_local_network_usage() {
    // Plan §1.8 / M9-3: without it macOS denies LAN connections (errno 65) with no prompt.
    let plist = read("Info.plist");
    let key = plist.find("<key>NSLocalNetworkUsageDescription</key>").expect("key present");
    let rest = &plist[key..];
    let value = rest.split("<string>").nth(1).and_then(|s| s.split("</string>").next()).unwrap();
    assert!(value.len() > 20, "a real explanation: {value}");
}

#[test]
fn session_windows_are_created_by_the_app_not_the_config() {
    // Every tab is created hidden, joined to the group and then shown (M6-2), so no window is
    // declared statically; capabilities cover the generated labels.
    let conf: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    let windows = conf["app"]["windows"].as_array().map_or(0, Vec::len);
    assert_eq!(windows, 0, "no static windows");
    assert_eq!(conf["bundle"]["macOS"]["minimumSystemVersion"], "14.0");
    let caps: serde_json::Value = serde_json::from_str(&read("capabilities/default.json")).unwrap();
    let labels: Vec<_> = caps["windows"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
    assert!(labels.contains(&"session-*"), "{labels:?}");
    assert_eq!(drift_app::windows::window_label(7), "session-7");
}
