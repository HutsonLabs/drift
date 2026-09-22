//! `DRIFT_E2E_*` mapping tests (synthetic values only).

use xtask::e2e_env::{Redactor, parse_credentials, vars_from_dir};

#[test]
fn redactor_hides_credential_values_but_not_ports_or_hosts() {
    let vars = vec![
        ("DRIFT_E2E_SYS_USER".to_owned(), "fake-sys".to_owned()),
        ("DRIFT_E2E_SYS_PASS".to_owned(), "Fake9-sys-pass".to_owned()),
        ("DRIFT_E2E_HL_PORT".to_owned(), "13392".to_owned()),
        ("DRIFT_E2E_LOGIN_PASS".to_owned(), String::new()),
    ];
    let r = Redactor::from_vars(&vars);
    assert_eq!(
        r.redact("user fake-sys pw Fake9-sys-pass port 13392"),
        "user <redacted> pw <redacted> port 13392"
    );
    assert_eq!(r.redact("clean line"), "clean line");
}

#[test]
fn parses_key_value_and_bare_credentials() {
    assert_eq!(
        parse_credentials("\tUsername: sys\n\tPassword: Fake9Pw\n"),
        Some(("sys".into(), "Fake9Pw".into()))
    );
    assert_eq!(parse_credentials("user2\nFake9Pw2\n"), Some(("user2".into(), "Fake9Pw2".into())));
    assert_eq!(parse_credentials("onlyone\n"), None);
    assert_eq!(parse_credentials(""), None);
}

#[test]
fn maps_files_to_variables() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("system.txt"), "Username: s\nPassword: sp\n").unwrap();
    std::fs::write(d.path().join("testuser.txt"), "l\nlp\n").unwrap();
    std::fs::write(d.path().join("headless2.txt"), "h\nhp\n").unwrap();
    let vars = vars_from_dir(d.path());
    let get = |k: &str| vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    assert_eq!(get("DRIFT_E2E_SYS_USER"), Some("s"));
    assert_eq!(get("DRIFT_E2E_SYS_PASS"), Some("sp"));
    assert_eq!(get("DRIFT_E2E_LOGIN_USER"), Some("l"));
    assert_eq!(get("DRIFT_E2E_LOGIN_PASS"), Some("lp"));
    assert_eq!(get("DRIFT_E2E_HL_USER"), Some("h"));
    assert_eq!(get("DRIFT_E2E_HL_PASS"), Some("hp"));
    assert_eq!(get("DRIFT_E2E_HL_PORT"), Some("13392"));
    assert_eq!(get("DRIFT_E2E_SHARE_USER"), None);
}

#[test]
fn a_port_already_bound_is_reported_as_occupied() {
    // A leftover `ssh -N -L` from an interrupted run keeps the forward ports bound. The e2e
    // harness must notice, because `ssh -o ExitOnForwardFailure=yes` then exits and the suite
    // would otherwise silently run through the *stale* forwards.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let busy = listener.local_addr().unwrap().port();
    let free = {
        let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        probe.local_addr().unwrap().port()
    };
    assert_eq!(xtask::e2e_env::ports_in_use(&[busy, free]), vec![busy]);
    drop(listener);
    assert!(xtask::e2e_env::ports_in_use(&[busy]).is_empty());
}
