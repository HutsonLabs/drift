//! M1-6 Red: the fingerprint Drift shows equals what `grdctl status` prints for the same cert.
//!
//! `data/grd_system_tls.der` is the (public) TLS certificate of the homelab's system daemon,
//! and the expected line was captured from `sudo grdctl --system status` on that host.

use drift_core::CertFingerprint;

const GRDCTL_STATUS_SAMPLE: &str = "\
RDP:
\tStatus: enabled
\tPort: 3389
\tAuthentication methods: credentials
\tTLS certificate: /var/lib/gnome-remote-desktop/.local/share/gnome-remote-desktop/certificates/rdp-tls.crt
\tTLS fingerprint: f3:e7:a2:68:ee:a5:64:38:f8:14:03:12:50:4d:44:97:6c:dc:c3:ae:f2:4a:98:92:da:97:72:fe:b6:e6:fe:81
\tTLS key: /var/lib/gnome-remote-desktop/.local/share/gnome-remote-desktop/certificates/rdp-tls.key
";

fn grdctl_line() -> &'static str {
    GRDCTL_STATUS_SAMPLE
        .lines()
        .find_map(|l| l.trim().strip_prefix("TLS fingerprint: "))
        .expect("sample has a fingerprint line")
}

#[test]
fn fingerprint_of_leaf_der_matches_grdctl_status() {
    let der = include_bytes!("data/grd_system_tls.der");
    let fp = CertFingerprint::of_der(der);
    assert_eq!(fp.to_string(), grdctl_line());
    assert!(fp.to_string().starts_with("f3:e7:a2:"));
}

#[test]
fn grdctl_line_parses_back_to_the_same_fingerprint() {
    let der = include_bytes!("data/grd_system_tls.der");
    assert_eq!(grdctl_line().parse::<CertFingerprint>().unwrap(), CertFingerprint::of_der(der));
}

#[test]
fn from_grdctl_status_extracts_the_fingerprint() {
    let fp = CertFingerprint::from_grdctl_status(GRDCTL_STATUS_SAMPLE).expect("found");
    assert_eq!(fp.to_string(), grdctl_line());
    assert_eq!(CertFingerprint::from_grdctl_status("RDP:\n\tStatus: enabled\n"), None);
    assert_eq!(CertFingerprint::from_grdctl_status("\tTLS fingerprint: (null)\n"), None);
}
