//! M2-2 / M2-3 e2e: scroll units, the horizontal wheel sign and Unicode typing against the
//! real GNOME host. Run through `cargo xtask e2e`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::time::Duration;

use drift_core::{ClipboardPrefs, ConnectMode, InputEvent, SessionState};
use drift_e2e::{E2eSession, host, init_logging, port, require};
use drift_rdp::SessionCommand;

const CONNECT: Duration = Duration::from_secs(40);
const SCROLL_FILE: &str = "/tmp/drift_scroll.txt";

async fn headless(user: &str, pass: &str) -> E2eSession {
    host::ensure_unlocked_session(&drift_e2e::headless_session_user());
    let mut s =
        E2eSession::start_with(ConnectMode::Headless, port("DRIFT_E2E_HL_PORT", 3392), user, pass, |p| {
            p.clipboard = ClipboardPrefs::TextAndImages
        });
    s.send(SessionCommand::Focus(true));
    s.wait_state("Connected", CONNECT, |st| matches!(st, SessionState::Connected { .. })).await;
    s
}

/// `host/scrolltool.py`, uploaded into the test user's home.
fn scrolltool() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join("host/scrolltool.py");
    root.canonicalize().unwrap_or(root)
}

/// Starts the scroll recorder and waits for its window.
fn start_scrolltool(user: &str) {
    host::kill_in_session(user, "scrolltool.py");
    host::remove_remote(SCROLL_FILE);
    let remote = host::upload_to_home(user, &scrolltool());
    host::spawn_in_session(user, &format!("python3 {remote} 90"));
    host::wait_for_remote(SCROLL_FILE, Duration::from_secs(30), |t| t.starts_with("total"))
        .unwrap_or_else(|| panic!("the scroll recorder did not start"));
}

/// What `scrolltool.py` has recorded so far.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Recorded {
    dx: f64,
    dy: f64,
    scrolls: u32,
    keys: u32,
    motions: u32,
}

fn recorded() -> Recorded {
    let text = host::read_remote(SCROLL_FILE).unwrap_or_default();
    let mut fields = text.split_whitespace().skip(1);
    let mut next = || fields.next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let (dx, dy) = (next(), next());
    let mut count = || {
        let v = next();
        u32::try_from(v.max(0.0).round() as i64).unwrap_or(0)
    };
    Recorded { dx, dy, scrolls: count(), keys: count(), motions: count() }
}

/// Waits until the scroll counter stops growing (all wheel events have been delivered).
async fn settled(s: &mut E2eSession) -> Recorded {
    let mut last = recorded();
    for _ in 0..20 {
        let _ = s.settle(Duration::from_millis(500)).await;
        let now = recorded();
        if now.scrolls == last.scrolls && now.scrolls > 0 {
            return now;
        }
        last = now;
    }
    last
}

/// Plan §1.5: g-r-d turns wheel units into `value/120 × 10 px`, so 30 events of ±12 move the
/// same distance as 3 events of ±120, and 5 × 120 moves 5/3 of that.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_scroll() {
    init_logging();
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = drift_e2e::headless_session_user();
    let mut s = headless(&rdp_user, &pass).await;
    start_scrolltool(&user);
    s.input(InputEvent::MouseMove { x: 640, y: 400 });

    for _ in 0..3 {
        s.input(InputEvent::Wheel { horizontal: false, units: 120 });
    }
    let first = settled(&mut s).await;
    let three_notches = first.dy;
    assert!(three_notches != 0.0, "the wheel reached the session: {first:?}");
    assert!(three_notches < 0.0, "+120 scrolls up (GTK reports negative dy)");

    let before = three_notches;
    for _ in 0..30 {
        s.input(InputEvent::Wheel { horizontal: false, units: 12 });
    }
    let after_fractional = settled(&mut s).await.dy;
    let fractional = after_fractional - before;
    assert!(
        (fractional - three_notches).abs() <= three_notches.abs() * 0.15,
        "30 × 12 units == 3 × 120 units ({fractional} vs {three_notches})"
    );

    let before = after_fractional;
    for _ in 0..5 {
        s.input(InputEvent::Wheel { horizontal: false, units: 120 });
    }
    let after_five = settled(&mut s).await.dy;
    let five_notches = after_five - before;
    let expected = three_notches / 3.0 * 5.0;
    assert!(
        (five_notches - expected).abs() <= expected.abs() * 0.15,
        "5 × 120 units scrolls 5/3 of 3 × 120 ({five_notches} vs {expected})"
    );
    host::kill_in_session(&user, "scrolltool.py");
    s.close().await;
}

/// Locks in the horizontal wheel sign (plan §1.5: g-r-d inverts `PTR_FLAGS_HWHEEL` internally,
/// so the sign has to be established by experiment): **positive** `PTR_FLAGS_HWHEEL` units
/// arrive in the session as a **positive** GTK `dx`, i.e. the content scrolls right, the same
/// way a positive vertical unit count scrolls up (negative GTK `dy`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_hscroll() {
    init_logging();
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = drift_e2e::headless_session_user();
    let mut s = headless(&rdp_user, &pass).await;
    start_scrolltool(&user);
    s.input(InputEvent::MouseMove { x: 640, y: 400 });

    for _ in 0..3 {
        s.input(InputEvent::Wheel { horizontal: true, units: 120 });
    }
    let got = settled(&mut s).await;
    assert!(got.scrolls > 0, "the horizontal wheel reached the session: {got:?}");
    assert!(got.dx != 0.0, "a horizontal wheel event moved the content: {got:?}");
    assert!(got.dx > 0.0, "+120 horizontal units scroll right ({got:?}); this sign is the contract");
    host::kill_in_session(&user, "scrolltool.py");
    s.close().await;
}

/// Smoke test for the whole input path: scancodes, Unicode events and pointer motion all
/// reach a GTK window in the session (the counters of `scrolltool.py`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_keys_reach_the_session() {
    init_logging();
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = drift_e2e::headless_session_user();
    let mut s = headless(&rdp_user, &pass).await;
    start_scrolltool(&user);

    s.input(InputEvent::MouseMove { x: 400, y: 300 });
    s.input(InputEvent::MouseMove { x: 640, y: 400 });
    for scancode in [0x1E_u8, 0x30, 0x2E] {
        s.input(InputEvent::Key { scancode, extended: false, down: true });
        s.input(InputEvent::Key { scancode, extended: false, down: false });
    }
    for ch in "xyz".encode_utf16() {
        s.input(InputEvent::Unicode { ch, down: true });
        s.input(InputEvent::Unicode { ch, down: false });
    }
    let mut got = recorded();
    for _ in 0..20 {
        let _ = s.settle(Duration::from_millis(500)).await;
        got = recorded();
        if got.keys >= 6 && got.motions > 0 {
            break;
        }
    }
    assert!(got.motions > 0, "pointer motion reached the session: {got:?}");
    assert!(got.keys >= 6, "scancode and Unicode key presses reached the session: {got:?}");
    host::kill_in_session(&user, "scrolltool.py");
    s.close().await;
}

/// M2-2: "Type using Mac layout" sends Unicode key events; typing into GNOME Text Editor and
/// copying it back through CLIPRDR returns the same string — including the shifted characters,
/// which are typed **without sending Shift**, the point of the Unicode path.
///
/// The string stays inside the remote keyboard layout on purpose: g-r-d turns each Unicode
/// event into an XKB **keysym** (`xkb_utf32_to_keysym`, `grd-session-rdp.c`) and mutter can
/// only inject keysyms the session's layout can produce. On the US-layout test session
/// `Grüße ✓` therefore never arrives; that case is a manual acceptance step
/// (`docs/acceptance.md`, M2-2) on a host whose layout provides those keysyms.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_unicode_typing() {
    init_logging();
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = drift_e2e::headless_session_user();
    let mut s = headless(&rdp_user, &pass).await;
    let typed = "drift !@#$%^&*()_+ 42";
    drift_e2e::type_and_copy(&mut s, &user, typed).await;

    let contents = s
        .wait_clipboard_where(Duration::from_secs(30), |c| {
            c.text().is_some_and(|t| t.trim_end_matches('\n') == "drift !@#$%^&*()_+ 42")
        })
        .await;
    let text = contents.text().unwrap_or_default().trim_end_matches('\n').to_owned();
    assert_eq!(text, typed, "Unicode events typed the exact string");
    host::kill_in_session(&user, "gnome-text-editor");
    s.close().await;
}
