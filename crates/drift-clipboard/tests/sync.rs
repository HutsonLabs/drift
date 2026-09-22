//! Red: pure CLIPRDR sync state (initial list, echo prevention, newest-wins, focus scoping,
//! prefs, size cap). Consumed by the M5-2 `CliprdrBackend` glue in drift-rdp.

use std::sync::Arc;
use std::time::Duration;

use drift_clipboard::formats::{
    CF_DIB, CF_TEXT, CF_UNICODETEXT, ClipError, ClipFormat, LOCAL_PNG_FORMAT_ID, MAX_CLIPBOARD_BYTES,
    PNG_FORMAT_NAME, encode_unicode_text,
};
use drift_clipboard::sync::{ECHO_WINDOW, FETCH_TIMEOUT};
use drift_clipboard::{ClipboardContents, ClipboardItem, ClipboardSync, SyncAction, SyncInput};
use drift_core::ClipboardPrefs;
use drift_testkit::ManualClock;

const PNG: &[u8] = include_bytes!("fixtures/remote_clip_d011.png");

fn text(s: &str) -> ClipboardContents {
    ClipboardContents { items: vec![ClipboardItem::Text(s.into())] }
}

fn unicode() -> ClipFormat {
    ClipFormat::standard(CF_UNICODETEXT)
}

fn remote_png() -> ClipFormat {
    ClipFormat::named(0xD011, PNG_FORMAT_NAME)
}

fn utf16(s: &str) -> Vec<u8> {
    encode_unicode_text(s)
}

fn ready(prefs: ClipboardPrefs, focused: bool) -> (ClipboardSync, ManualClock) {
    let clock = ManualClock::new();
    let mut s = ClipboardSync::new(prefs, focused, Arc::new(clock.clone()));
    s.handle(SyncInput::InitialFormatListRequested);
    (s, clock)
}

fn local(cc: i64, c: ClipboardContents) -> SyncInput {
    SyncInput::LocalChanged { change_count: cc, contents: c }
}

// ---------------------------------------------------------------- initial list

#[test]
fn not_ready_until_initial_list_answered() {
    let clock = ManualClock::new();
    let mut s = ClipboardSync::new(ClipboardPrefs::TextAndImages, true, Arc::new(clock));
    assert!(!s.is_ready());
    // Local changes before the initial request are remembered, not advertised.
    assert_eq!(s.handle(local(1, text("early"))), vec![]);
    // A remote copy before readiness is ignored (IronRDP would not deliver it anyway).
    assert_eq!(s.handle(SyncInput::RemoteFormatList(vec![unicode()])), vec![]);
    let acts = s.handle(SyncInput::InitialFormatListRequested);
    assert!(s.is_ready());
    assert_eq!(acts, vec![SyncAction::SendFormatList(vec![unicode()])]);
}

#[test]
fn initial_list_is_empty_without_local_contents_or_when_off_or_unfocused() {
    for (prefs, focused, seed) in [
        (ClipboardPrefs::TextAndImages, true, false),
        (ClipboardPrefs::Off, true, true),
        (ClipboardPrefs::TextAndImages, false, true),
    ] {
        let mut s = ClipboardSync::new(prefs, focused, Arc::new(ManualClock::new()));
        if seed {
            s.handle(local(1, text("x")));
        }
        assert_eq!(
            s.handle(SyncInput::InitialFormatListRequested),
            vec![SyncAction::SendFormatList(vec![])],
            "{prefs:?} focused={focused}"
        );
        assert!(s.is_ready());
    }
}

// ---------------------------------------------------------------- remote → local

#[test]
fn remote_text_copy_is_fetched_and_written() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    let acts = s.handle(SyncInput::RemoteFormatList(vec![ClipFormat::standard(CF_TEXT), unicode()]));
    assert_eq!(acts, vec![SyncAction::RequestRemoteData(unicode())]);
    let acts = s.handle(SyncInput::RemoteData(Some(utf16("copy-me-1\nx"))));
    assert_eq!(acts, vec![SyncAction::WritePasteboard(text("copy-me-1\nx"))]);
}

#[test]
fn remote_text_and_image_are_fetched_sequentially() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    let acts = s.handle(SyncInput::RemoteFormatList(vec![remote_png(), unicode()]));
    assert_eq!(acts, vec![SyncAction::RequestRemoteData(unicode())]);
    let acts = s.handle(SyncInput::RemoteData(Some(utf16("t"))));
    assert_eq!(acts, vec![SyncAction::RequestRemoteData(remote_png())]);
    let acts = s.handle(SyncInput::RemoteData(Some(PNG.to_vec())));
    assert_eq!(
        acts,
        vec![SyncAction::WritePasteboard(ClipboardContents {
            items: vec![ClipboardItem::Text("t".into()), ClipboardItem::Png(PNG.to_vec())]
        })]
    );
}

#[test]
fn remote_error_and_bad_image_are_rejected() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(SyncInput::RemoteFormatList(vec![remote_png(), unicode()]));
    assert_eq!(
        s.handle(SyncInput::RemoteData(None)),
        vec![SyncAction::Rejected(ClipError::RemoteError), SyncAction::RequestRemoteData(remote_png())]
    );
    let acts = s.handle(SyncInput::RemoteData(Some(b"not a png".to_vec())));
    assert!(matches!(acts.as_slice(), [SyncAction::Rejected(ClipError::InvalidImage(_))]), "{acts:?}");
}

#[test]
fn oversized_remote_payload_gives_rejection_event() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(SyncInput::RemoteFormatList(vec![remote_png()]));
    let acts = s.handle(SyncInput::RemoteData(Some(vec![0; MAX_CLIPBOARD_BYTES + 1])));
    assert_eq!(
        acts,
        vec![SyncAction::Rejected(ClipError::TooLarge {
            size: MAX_CLIPBOARD_BYTES + 1,
            max: MAX_CLIPBOARD_BYTES
        })]
    );
}

#[test]
fn text_only_prefs_skip_images_and_off_ignores_everything() {
    let (mut s, _) = ready(ClipboardPrefs::Text, true);
    assert_eq!(s.handle(SyncInput::RemoteFormatList(vec![remote_png()])), vec![]);
    let (mut s, _) = ready(ClipboardPrefs::Off, true);
    assert_eq!(s.handle(SyncInput::RemoteFormatList(vec![unicode()])), vec![]);
    assert_eq!(s.handle(local(3, text("x"))), vec![]);
    assert_eq!(
        s.handle(SyncInput::RemoteDataRequest { format_id: CF_UNICODETEXT }),
        vec![SyncAction::SendData {
            format_id: CF_UNICODETEXT,
            data: Err(ClipError::Unavailable(CF_UNICODETEXT))
        }]
    );
}

#[test]
fn unsolicited_remote_data_is_ignored() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    assert_eq!(s.handle(SyncInput::RemoteData(Some(utf16("x")))), vec![]);
}

// ---------------------------------------------------------------- local → remote

#[test]
fn local_change_is_advertised_and_served() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    let c = ClipboardContents {
        items: vec![ClipboardItem::Text("a\nb".into()), ClipboardItem::Png(PNG.to_vec())],
    };
    let acts = s.handle(local(7, c));
    assert_eq!(
        acts,
        vec![SyncAction::SendFormatList(vec![
            unicode(),
            ClipFormat::named(LOCAL_PNG_FORMAT_ID, PNG_FORMAT_NAME),
            ClipFormat::standard(CF_DIB)
        ])]
    );
    assert_eq!(
        s.handle(SyncInput::RemoteDataRequest { format_id: CF_UNICODETEXT }),
        vec![SyncAction::SendData { format_id: CF_UNICODETEXT, data: Ok(utf16("a\r\nb")) }]
    );
    assert_eq!(
        s.handle(SyncInput::RemoteDataRequest { format_id: LOCAL_PNG_FORMAT_ID }),
        vec![SyncAction::SendData { format_id: LOCAL_PNG_FORMAT_ID, data: Ok(PNG.to_vec()) }]
    );
}

#[test]
fn oversized_local_image_is_rejected_and_not_advertised() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    let c = ClipboardContents {
        items: vec![ClipboardItem::Text("t".into()), ClipboardItem::Png(vec![0; MAX_CLIPBOARD_BYTES + 1])],
    };
    assert_eq!(
        s.handle(local(2, c)),
        vec![
            SyncAction::Rejected(ClipError::TooLarge {
                size: MAX_CLIPBOARD_BYTES + 1,
                max: MAX_CLIPBOARD_BYTES
            }),
            SyncAction::SendFormatList(vec![unicode()]),
        ]
    );
}

#[test]
fn repeated_change_count_is_not_readvertised() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    assert_eq!(s.handle(local(4, text("a"))).len(), 1);
    assert_eq!(s.handle(local(4, text("a"))), vec![]);
}

// ---------------------------------------------------------------- echo loop

#[test]
fn own_pasteboard_write_is_not_echoed_back_to_server() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(SyncInput::RemoteFormatList(vec![unicode()]));
    let acts = s.handle(SyncInput::RemoteData(Some(utf16("from-remote"))));
    assert_eq!(acts, vec![SyncAction::WritePasteboard(text("from-remote"))]);
    assert_eq!(s.handle(SyncInput::LocalWritten { change_count: 42 }), vec![]);
    // The poller sees changeCount 42: it is our own write, so nothing goes back to the server.
    assert_eq!(s.handle(local(42, text("from-remote"))), vec![]);
    // A genuine local change afterwards is advertised.
    assert_eq!(s.handle(local(43, text("mine"))), vec![SyncAction::SendFormatList(vec![unicode()])]);
}

#[test]
fn echoed_format_list_is_ignored_within_window() {
    let (mut s, clock) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(local(5, text("mine")));
    // The server echoes our own list straight back (its own ids, same formats).
    clock.advance(ECHO_WINDOW / 2);
    assert_eq!(s.handle(SyncInput::RemoteFormatList(vec![unicode()])), vec![]);
    // Outside the window the same list is a genuine remote copy.
    clock.advance(ECHO_WINDOW);
    assert_eq!(
        s.handle(SyncInput::RemoteFormatList(vec![unicode()])),
        vec![SyncAction::RequestRemoteData(unicode())]
    );
}

#[test]
fn different_remote_list_inside_echo_window_is_not_an_echo() {
    let (mut s, clock) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(local(5, text("mine")));
    clock.advance(Duration::from_millis(10));
    assert_eq!(
        s.handle(SyncInput::RemoteFormatList(vec![remote_png()])),
        vec![SyncAction::RequestRemoteData(remote_png())]
    );
}

// ---------------------------------------------------------------- newest wins

#[test]
fn newer_local_change_beats_slower_remote_fetch() {
    let (mut s, clock) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(SyncInput::RemoteFormatList(vec![unicode()]));
    clock.advance(Duration::from_millis(50));
    // The user copies locally while the remote fetch is in flight: local wins.
    assert_eq!(s.handle(local(9, text("local-newer"))), vec![SyncAction::SendFormatList(vec![unicode()])]);
    clock.advance(Duration::from_millis(50));
    // The stale remote data must not clobber the newer local clipboard.
    assert_eq!(s.handle(SyncInput::RemoteData(Some(utf16("remote-older")))), vec![]);
    // And the server gets the local data when it asks.
    assert_eq!(
        s.handle(SyncInput::RemoteDataRequest { format_id: CF_UNICODETEXT }),
        vec![SyncAction::SendData { format_id: CF_UNICODETEXT, data: Ok(utf16("local-newer")) }]
    );
}

#[test]
fn newer_remote_copy_beats_older_local_change() {
    let (mut s, clock) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(local(9, text("local-older")));
    clock.advance(ECHO_WINDOW * 2);
    assert_eq!(
        s.handle(SyncInput::RemoteFormatList(vec![unicode()])),
        vec![SyncAction::RequestRemoteData(unicode())]
    );
    clock.advance(Duration::from_millis(30));
    assert_eq!(
        s.handle(SyncInput::RemoteData(Some(utf16("remote-newer")))),
        vec![SyncAction::WritePasteboard(text("remote-newer"))]
    );
}

#[test]
fn newer_remote_copy_supersedes_in_flight_fetch() {
    let (mut s, clock) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(SyncInput::RemoteFormatList(vec![remote_png(), unicode()]));
    clock.advance(Duration::from_millis(20));
    // A second remote copy arrives while the first fetch is in flight: wait for the in-flight
    // response (dropped), then fetch the new one.
    assert_eq!(s.handle(SyncInput::RemoteFormatList(vec![unicode()])), vec![]);
    assert_eq!(
        s.handle(SyncInput::RemoteData(Some(utf16("old")))),
        vec![SyncAction::RequestRemoteData(unicode())]
    );
    assert_eq!(
        s.handle(SyncInput::RemoteData(Some(utf16("new")))),
        vec![SyncAction::WritePasteboard(text("new"))]
    );
}

#[test]
fn stuck_fetch_times_out() {
    let (mut s, clock) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(SyncInput::RemoteFormatList(vec![unicode()]));
    clock.advance(FETCH_TIMEOUT / 2);
    assert_eq!(s.handle(SyncInput::Tick), vec![]);
    clock.advance(FETCH_TIMEOUT);
    assert_eq!(s.handle(SyncInput::Tick), vec![]);
    // The late response is dropped; a new remote copy fetches normally.
    assert_eq!(s.handle(SyncInput::RemoteData(Some(utf16("late")))), vec![]);
    assert_eq!(
        s.handle(SyncInput::RemoteFormatList(vec![unicode()])),
        vec![SyncAction::RequestRemoteData(unicode())]
    );
}

// ---------------------------------------------------------------- focus scoping & prefs

#[test]
fn unfocused_session_defers_local_changes_until_focus() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, false);
    assert_eq!(s.handle(local(11, text("elsewhere"))), vec![]);
    assert_eq!(s.handle(SyncInput::RemoteFormatList(vec![unicode()])), vec![]);
    assert_eq!(s.handle(SyncInput::Focus(true)), vec![SyncAction::SendFormatList(vec![unicode()])]);
    assert!(s.is_focused());
    // Regaining focus without a new local change does not re-advertise.
    s.handle(SyncInput::Focus(false));
    assert_eq!(s.handle(SyncInput::Focus(true)), vec![]);
}

#[test]
fn unfocused_session_still_answers_data_requests() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    s.handle(local(1, text("x")));
    s.handle(SyncInput::Focus(false));
    assert_eq!(
        s.handle(SyncInput::RemoteDataRequest { format_id: CF_UNICODETEXT }),
        vec![SyncAction::SendData { format_id: CF_UNICODETEXT, data: Ok(utf16("x")) }]
    );
}

#[test]
fn prefs_change_readvertises_with_new_filter() {
    let (mut s, _) = ready(ClipboardPrefs::TextAndImages, true);
    let c =
        ClipboardContents { items: vec![ClipboardItem::Text("t".into()), ClipboardItem::Png(PNG.to_vec())] };
    s.handle(local(1, c));
    assert_eq!(
        s.handle(SyncInput::Prefs(ClipboardPrefs::Text)),
        vec![SyncAction::SendFormatList(vec![unicode()])]
    );
    assert_eq!(s.prefs(), ClipboardPrefs::Text);
    assert_eq!(s.handle(SyncInput::Prefs(ClipboardPrefs::Off)), vec![SyncAction::SendFormatList(vec![])]);
    assert_eq!(s.handle(SyncInput::Prefs(ClipboardPrefs::Off)), vec![]);
}
