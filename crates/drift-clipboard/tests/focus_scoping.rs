//! M5-3 Red: pasteboard polling + focus scoping with fakes. One watcher polls the (fake)
//! pasteboard and fans changes out to every session; only the focused session syncs.

use std::cell::RefCell;
use std::sync::Arc;

use drift_clipboard::formats::{CF_UNICODETEXT, ClipFormat};
use drift_clipboard::poll::{LocalChange, POLL_INTERVAL, PasteboardPort, PasteboardWatcher};
use drift_clipboard::{ClipboardContents, ClipboardItem, ClipboardSync, SyncAction, SyncInput};
use drift_core::ClipboardPrefs;
use drift_testkit::ManualClock;

#[derive(Default)]
struct FakePasteboard {
    count: RefCell<i64>,
    contents: RefCell<ClipboardContents>,
    reads: RefCell<Vec<ClipboardPrefs>>,
}

impl FakePasteboard {
    fn user_copies(&self, c: ClipboardContents) {
        *self.count.borrow_mut() += 1;
        *self.contents.borrow_mut() = c;
    }
}

impl PasteboardPort for FakePasteboard {
    fn change_count(&self) -> i64 {
        *self.count.borrow()
    }

    fn read(&self, level: ClipboardPrefs) -> ClipboardContents {
        self.reads.borrow_mut().push(level);
        let mut c = self.contents.borrow().clone();
        if level != ClipboardPrefs::TextAndImages {
            c.items.retain(|i| matches!(i, ClipboardItem::Text(_)));
        }
        c
    }

    fn write(&self, contents: &ClipboardContents) -> i64 {
        *self.count.borrow_mut() += 1;
        *self.contents.borrow_mut() = contents.clone();
        *self.count.borrow()
    }
}

fn text(s: &str) -> ClipboardContents {
    ClipboardContents { items: vec![ClipboardItem::Text(s.into())] }
}

fn unicode_list() -> SyncAction {
    SyncAction::SendFormatList(vec![ClipFormat::standard(CF_UNICODETEXT)])
}

/// Minimal stand-in for the app's main-thread poll loop + per-session actors.
struct Harness {
    pb: FakePasteboard,
    watcher: PasteboardWatcher,
    sessions: Vec<ClipboardSync>,
}

impl Harness {
    fn new(n: usize, focused: usize) -> Self {
        let pb = FakePasteboard::default();
        let clock = ManualClock::new();
        let watcher = PasteboardWatcher::new(pb.change_count());
        let sessions = (0..n)
            .map(|i| {
                let mut s =
                    ClipboardSync::new(ClipboardPrefs::TextAndImages, i == focused, Arc::new(clock.clone()));
                s.handle(SyncInput::InitialFormatListRequested);
                s
            })
            .collect();
        Self { pb, watcher, sessions }
    }

    /// One 250 ms poll tick; returns each session's actions.
    fn tick(&mut self) -> Vec<Vec<SyncAction>> {
        match self.watcher.poll(&self.pb, ClipboardPrefs::TextAndImages) {
            Some(LocalChange { change_count, contents }) => self
                .sessions
                .iter_mut()
                .map(|s| s.handle(SyncInput::LocalChanged { change_count, contents: contents.clone() }))
                .collect(),
            None => vec![vec![]; self.sessions.len()],
        }
    }

    fn focus(&mut self, idx: usize) -> Vec<Vec<SyncAction>> {
        self.sessions.iter_mut().enumerate().map(|(i, s)| s.handle(SyncInput::Focus(i == idx))).collect()
    }
}

#[test]
fn poll_interval_is_250ms() {
    assert_eq!(POLL_INTERVAL.as_millis(), 250);
}

#[test]
fn watcher_reports_only_changes() {
    let pb = FakePasteboard::default();
    let mut w = PasteboardWatcher::new(pb.change_count());
    assert_eq!(w.poll(&pb, ClipboardPrefs::TextAndImages), None);
    assert!(pb.reads.borrow().is_empty(), "no read without a changeCount change");
    pb.user_copies(text("a"));
    assert_eq!(w.poll(&pb, ClipboardPrefs::Text), Some(LocalChange { change_count: 1, contents: text("a") }));
    assert_eq!(*pb.reads.borrow(), vec![ClipboardPrefs::Text]);
    assert_eq!(w.poll(&pb, ClipboardPrefs::TextAndImages), None);
    // Off: the change is consumed but nothing is read.
    pb.user_copies(text("b"));
    assert_eq!(w.poll(&pb, ClipboardPrefs::Off), None);
    assert_eq!(pb.reads.borrow().len(), 1);
    assert_eq!(
        PasteboardWatcher::snapshot(&pb, ClipboardPrefs::TextAndImages),
        LocalChange { change_count: 2, contents: text("b") }
    );
}

#[test]
fn only_focused_session_advertises_local_copy() {
    let mut h = Harness::new(3, 1);
    h.pb.user_copies(text("hello"));
    let acts = h.tick();
    assert_eq!(acts, vec![vec![], vec![unicode_list()], vec![]]);
    // Switching to tab 2 advertises the pending local clipboard there, and nowhere else.
    let acts = h.focus(2);
    assert_eq!(acts, vec![vec![], vec![], vec![unicode_list()]]);
    // Tab 0 got the change while unfocused too: it advertises when focused.
    let acts = h.focus(0);
    assert_eq!(acts, vec![vec![unicode_list()], vec![], vec![]]);
}

#[test]
fn remote_copy_in_one_tab_reaches_other_tab_via_pasteboard_but_not_back() {
    let mut h = Harness::new(2, 0);
    // Tab 0's server copies text; tab 0 fetches and writes the pasteboard.
    let s0 = &mut h.sessions[0];
    assert_eq!(
        s0.handle(SyncInput::RemoteFormatList(vec![ClipFormat::standard(CF_UNICODETEXT)])),
        vec![SyncAction::RequestRemoteData(ClipFormat::standard(CF_UNICODETEXT))]
    );
    let acts = s0.handle(SyncInput::RemoteData(Some(drift_clipboard::formats::encode_unicode_text("r"))));
    let [SyncAction::WritePasteboard(c)] = acts.as_slice() else { panic!("{acts:?}") };
    let cc = h.pb.write(c);
    assert_eq!(h.sessions[0].handle(SyncInput::LocalWritten { change_count: cc }), vec![]);
    // Next poll: tab 0 recognises its own write (no echo); tab 1 is unfocused (deferred).
    assert_eq!(h.tick(), vec![vec![], vec![]]);
    // Focusing tab 1 hands the text to its server.
    assert_eq!(h.focus(1), vec![vec![], vec![unicode_list()]]);
}

#[test]
fn unfocused_session_ignores_remote_copies() {
    let mut h = Harness::new(2, 0);
    assert_eq!(
        h.sessions[1].handle(SyncInput::RemoteFormatList(vec![ClipFormat::standard(CF_UNICODETEXT)])),
        vec![]
    );
}
