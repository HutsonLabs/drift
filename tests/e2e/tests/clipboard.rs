//! M5-2 e2e: bidirectional clipboard against the real GNOME host (plan §6 M5-2, §1.6).
//! Run through `cargo xtask e2e`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use drift_clipboard::{ClipboardContents, ClipboardItem};
use drift_core::{ClipboardPrefs, ConnectMode, InputEvent, MouseButton, SessionState};
use drift_e2e::{E2eSession, host, init_logging, port, require};
use drift_rdp::SessionCommand;
use drift_testkit::fixtures::{self, names};

const CONNECT: Duration = Duration::from_secs(40);
const CLIP: Duration = Duration::from_secs(45);
/// Where `host/cliptool.py` reports what it read or wrote.
const CLIP_RESULT: &str = "/tmp/drift_clip_result.txt";

/// A connected, focused headless session on `drifttest2` with the clipboard enabled.
async fn focused_session(user: &str, pass: &str) -> E2eSession {
    host::ensure_unlocked_session(&drift_e2e::headless_session_user());
    let mut s =
        E2eSession::start_with(ConnectMode::Headless, port("DRIFT_E2E_HL_PORT", 3392), user, pass, |p| {
            p.clipboard = ClipboardPrefs::TextAndImages
        });
    s.send(SessionCommand::Focus(true));
    s.wait_state("Connected", CONNECT, |st| matches!(st, SessionState::Connected { .. })).await;
    s
}

/// `(width, height)` of a PNG, from its IHDR.
fn png_size(png: &[u8]) -> (u32, u32) {
    let be = |o: usize| u32::from_be_bytes(png[o..o + 4].try_into().expect("IHDR"));
    assert_eq!(&png[1..4], b"PNG", "not a PNG");
    (be(16), be(20))
}

/// Uploads `host/cliptool.py` and starts it in the session with `args`.
fn cliptool(user: &str, args: &str) {
    host::kill_in_session(user, "cliptool.py");
    host::remove_remote(CLIP_RESULT);
    let remote = host::upload_to_home(user, &helper("cliptool.py"));
    host::spawn_in_session(user, &format!("python3 {remote} {args}"));
}

/// A helper script from the repository's `host/` directory.
fn helper(name: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join("host").join(name);
    path.canonicalize().unwrap_or(path)
}

/// Clicks in the middle of the remote desktop (GNOME leaves a freshly launched window
/// unfocused, and an unfocused Wayland client's clipboard is not propagated, plan §1.6).
async fn click_center(s: &mut E2eSession) {
    for event in [
        InputEvent::MouseMove { x: 640, y: 400 },
        InputEvent::MouseButton { button: MouseButton::Left, down: true, x: 640, y: 400 },
        InputEvent::MouseButton { button: MouseButton::Left, down: false, x: 640, y: 400 },
    ] {
        s.input(event);
    }
    let _ = s.settle(Duration::from_secs(2)).await;
}

/// The remote copies a text selection (GNOME Text Editor, Ctrl+A/Ctrl+C); Drift fetches it
/// eagerly and hands it to the app (plan §6 M5-2).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_clip_remote_text() {
    init_logging();
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = drift_e2e::headless_session_user();
    let mut s = focused_session(&rdp_user, &pass).await;

    let text = format!("copy-me-{}", std::process::id());
    drift_e2e::type_and_copy(&mut s, &user, &text).await;

    let contents = s.wait_clipboard_where(CLIP, |c| c.text().is_some_and(|t| t.contains("copy-me-"))).await;
    assert_eq!(contents.text().map(str::trim_end), Some(text.as_str()), "the remote text arrived");
    host::kill_in_session(&user, "gnome-text-editor");
    s.close().await;
}

/// The remote copies an image (Print Screen → Enter in the GNOME screenshot UI, plan §1.6);
/// it arrives as a PNG of the remote desktop's size.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_clip_remote_image() {
    init_logging();
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = drift_e2e::headless_session_user();
    let mut s = focused_session(&rdp_user, &pass).await;

    // `cliptool.py` puts a 320×200 `image/png` on the session clipboard once its window has
    // focus (GNOME leaves a freshly launched window unfocused, and an unfocused Wayland
    // client's clipboard change is not propagated, plan §1.6).
    cliptool(&user, &format!("write-image /home/{user}/clip_in.png 60"));
    let _ = s.settle(Duration::from_secs(5)).await;
    click_center(&mut s).await;
    // A key press gives the helper a fresh Wayland input serial, which is what a client needs
    // to take the selection (the same reason a user's Ctrl+C works).
    s.input(InputEvent::Key { scancode: 0x2E, extended: false, down: true });
    s.input(InputEvent::Key { scancode: 0x2E, extended: false, down: false });
    let _ = s.settle(Duration::from_secs(2)).await;

    let contents =
        s.wait_clipboard_where(CLIP, |c| c.items.iter().any(|i| matches!(i, ClipboardItem::Png(_)))).await;
    let png = contents
        .items
        .iter()
        .find_map(|i| match i {
            ClipboardItem::Png(p) => Some(p.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no PNG in {:?}", contents.items));
    assert_eq!(png_size(&png), (320, 200), "clip_in.png is 320×200");
    host::kill_in_session(&user, "cliptool.py");
    s.close().await;
}

/// Drift advertises the local clipboard; a GTK app in the session reads text and image back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real GNOME host; run via `cargo xtask e2e`"]
async fn e2e_clip_local_text_image() {
    init_logging();
    let (rdp_user, pass) = (require("DRIFT_E2E_HL_USER"), require("DRIFT_E2E_HL_PASS"));
    let user = drift_e2e::headless_session_user();
    let mut s = focused_session(&rdp_user, &pass).await;

    let png = fixtures::read(names::CLIP_REMOTE_PNG);
    let (width, height) = png_size(&png);
    let text = format!("drift-local-{}", std::process::id());
    s.send(SessionCommand::ClipboardLocalChanged(ClipboardContents {
        items: vec![ClipboardItem::Text(text.clone()), ClipboardItem::Png(png)],
    }));
    // Give the format list time to reach the server before the reader asks for the data.
    let _ = s.settle(Duration::from_secs(3)).await;

    // GNOME occasionally needs a second reader (the first one can race the format list).
    let mut result = String::new();
    for attempt in 1..=3 {
        cliptool(&user, "read 25");
        result = host::wait_for_remote(CLIP_RESULT, CLIP, |t| t.contains("image:") || t.contains("err"))
            .unwrap_or_default();
        eprintln!("[e2e] clipboard reader attempt {attempt}: {result}");
        if result.contains("image:") {
            break;
        }
        let _ = s.settle(Duration::from_secs(2)).await;
    }
    assert!(result.contains(&text), "the text was pasted in the session: {result}");
    assert!(result.contains(&format!("image: {width}x{height}")), "the PNG was pasted: {result}");
    host::kill_in_session(&user, "cliptool.py");
    s.close().await;
}
