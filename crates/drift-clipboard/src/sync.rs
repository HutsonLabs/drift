//! Pure CLIPRDR sync state for one session (M5-1/M5-3 support for **M5-2**).
//!
//! [`ClipboardSync`] is driven by the session actor: the IronRDP `CliprdrBackend` glue in
//! `drift-rdp` turns backend callbacks into [`SyncInput`]s, and executes the returned
//! [`SyncAction`]s (`initiate_copy`, `initiate_paste`, `submit_format_data`, pasteboard write).
//!
//! Rules:
//! - **Initial list:** the server's format-list request is always answered (the local list
//!   when focused and allowed, otherwise empty); until then nothing is advertised.
//! - **Echo prevention:** pasteboard changes whose `changeCount` is one of our own writes are
//!   ignored, and a remote list that merely echoes our last advertisement shortly after we
//!   sent it is ignored.
//! - **Newest wins:** every change is stamped with the [`Clock`]; a remote fetch is dropped
//!   when a newer local change arrives before it completes, and vice versa.
//! - **Focus scoping:** only the focused session syncs. Local changes seen while unfocused are
//!   advertised when the session regains focus; remote copies while unfocused are ignored.
//!
//! IronRDP allows one Format Data Request in flight and its response carries no format id, so
//! the state tracks the in-flight request itself; superseded requests are marked stale and
//! their responses dropped.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use drift_core::{ClipboardPrefs, Clock};

use crate::formats::{
    ClipError, ClipFormat, FormatKind, decode_inbound, encode_outbound, filter_local, outbound_formats,
    select_inbound,
};
use crate::{ClipboardContents, ClipboardItem};

/// A remote fetch that has not completed after this long is abandoned.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// A remote list equal to our last advertisement within this window is treated as an echo.
pub const ECHO_WINDOW: Duration = Duration::from_millis(500);
/// How many of our own pasteboard writes are remembered for echo suppression.
const OWN_WRITES_KEPT: usize = 8;

/// Events fed into [`ClipboardSync`].
#[derive(Debug, Clone, PartialEq)]
pub enum SyncInput {
    /// The server asked for our initial format list (`on_request_format_list`).
    InitialFormatListRequested,
    /// The server announced a new remote clipboard (`on_remote_copy`).
    RemoteFormatList(Vec<ClipFormat>),
    /// Data for the in-flight [`SyncAction::RequestRemoteData`] (`on_format_data_response`);
    /// `None` when the server answered with an error.
    RemoteData(Option<Vec<u8>>),
    /// The server wants local data in `format_id` (`on_format_data_request`).
    RemoteDataRequest {
        /// Requested format id (one of ours).
        format_id: u32,
    },
    /// The local pasteboard changed.
    LocalChanged {
        /// NSPasteboard `changeCount` after the change.
        change_count: i64,
        /// The new contents (all representations the watcher read).
        contents: ClipboardContents,
    },
    /// The adapter executed our [`SyncAction::WritePasteboard`]; `change_count` is the
    /// pasteboard's `changeCount` right after the write.
    LocalWritten {
        /// `changeCount` after our write.
        change_count: i64,
    },
    /// The session's tab gained (`true`) or lost focus.
    Focus(bool),
    /// The profile's clipboard preference changed.
    Prefs(ClipboardPrefs),
    /// Periodic tick for timeouts.
    Tick,
}

/// What the actor must do next.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncAction {
    /// Send a Format List PDU (`CliprdrClient::initiate_copy`).
    SendFormatList(Vec<ClipFormat>),
    /// Request remote data (`CliprdrClient::initiate_paste`).
    RequestRemoteData(ClipFormat),
    /// Answer a Format Data Request (`submit_format_data`); `Err` → error response.
    SendData {
        /// The requested format id.
        format_id: u32,
        /// The payload, or why there is none.
        data: Result<Vec<u8>, ClipError>,
    },
    /// Write these contents to the local pasteboard, then feed back [`SyncInput::LocalWritten`].
    WritePasteboard(ClipboardContents),
    /// A payload was refused (size cap, undecodable image, remote error).
    Rejected(ClipError),
}

/// The latest local pasteboard state seen by this session.
#[derive(Debug, Clone)]
struct LocalSnapshot {
    contents: ClipboardContents,
    at: Instant,
}

/// A remote clipboard being fetched, one format at a time.
#[derive(Debug)]
struct RemoteFetch {
    queue: VecDeque<(ClipFormat, FormatKind)>,
    items: Vec<ClipboardItem>,
    announced: Instant,
}

/// The single Format Data Request on the wire.
#[derive(Debug)]
struct Inflight {
    kind: FormatKind,
    since: Instant,
    /// Superseded: the response is consumed and dropped.
    stale: bool,
}

/// CLIPRDR sync state for one session.
pub struct ClipboardSync {
    clock: Arc<dyn Clock>,
    prefs: ClipboardPrefs,
    focused: bool,
    ready: bool,
    /// Latest local pasteboard contents (unfiltered).
    local: Option<LocalSnapshot>,
    last_local_change_count: Option<i64>,
    /// A local change this server has not been told about yet.
    pending_local: bool,
    /// What we last advertised (filtered); served on Format Data Requests.
    advertised: ClipboardContents,
    /// Sorted kinds of our last advertisement and when it was sent (echo detection).
    last_sent: Option<(Vec<Option<FormatKind>>, Instant)>,
    /// `changeCount`s produced by our own pasteboard writes.
    own_writes: VecDeque<i64>,
    /// Contents of a write not yet acknowledged by [`SyncInput::LocalWritten`].
    unacked_write: Option<ClipboardContents>,
    fetch: Option<RemoteFetch>,
    inflight: Option<Inflight>,
}

impl std::fmt::Debug for ClipboardSync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardSync")
            .field("prefs", &self.prefs)
            .field("focused", &self.focused)
            .field("ready", &self.ready)
            .field("pending_local", &self.pending_local)
            .field("inflight", &self.inflight)
            .finish_non_exhaustive()
    }
}

fn sorted_kinds(formats: &[ClipFormat]) -> Vec<Option<FormatKind>> {
    let mut kinds: Vec<_> = formats.iter().map(ClipFormat::kind).collect();
    kinds.sort_unstable();
    kinds
}

impl ClipboardSync {
    /// A new sync state; not ready until the initial format list has been answered.
    pub fn new(prefs: ClipboardPrefs, focused: bool, clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            prefs,
            focused,
            ready: false,
            local: None,
            last_local_change_count: None,
            pending_local: false,
            advertised: ClipboardContents::empty(),
            last_sent: None,
            own_writes: VecDeque::new(),
            unacked_write: None,
            fetch: None,
            inflight: None,
        }
    }

    /// `true` once the server's initial format-list request has been answered.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Current preference.
    pub fn prefs(&self) -> ClipboardPrefs {
        self.prefs
    }

    /// Whether this session currently has focus.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Processes one input and returns the actions to execute, in order.
    pub fn handle(&mut self, input: SyncInput) -> Vec<SyncAction> {
        let now = self.clock.now();
        match input {
            SyncInput::InitialFormatListRequested => self.on_initial_request(now),
            SyncInput::RemoteFormatList(formats) => self.on_remote_list(&formats, now),
            SyncInput::RemoteData(data) => self.on_remote_data(data, now),
            SyncInput::RemoteDataRequest { format_id } => self.on_data_request(format_id),
            SyncInput::LocalChanged { change_count, contents } => {
                self.on_local_change(change_count, contents, now)
            }
            SyncInput::LocalWritten { change_count } => {
                self.unacked_write = None;
                self.own_writes.push_back(change_count);
                if self.own_writes.len() > OWN_WRITES_KEPT {
                    self.own_writes.pop_front();
                }
                vec![]
            }
            SyncInput::Focus(focused) => {
                self.focused = focused;
                if focused && self.pending_local { self.advertise_local(now) } else { vec![] }
            }
            SyncInput::Prefs(prefs) => self.on_prefs(prefs, now),
            SyncInput::Tick => {
                if self.inflight.as_ref().is_some_and(|i| now.duration_since(i.since) >= FETCH_TIMEOUT) {
                    self.inflight = None;
                    self.fetch = None;
                }
                vec![]
            }
        }
    }

    /// `true` when this session may talk to the server about clipboard changes right now.
    fn can_sync(&self) -> bool {
        self.ready && self.focused && self.prefs != ClipboardPrefs::Off
    }

    fn on_initial_request(&mut self, now: Instant) -> Vec<SyncAction> {
        self.ready = true;
        if self.can_sync() && self.local.is_some() {
            return self.advertise_local(now);
        }
        self.pending_local = self.local.is_some() && self.prefs != ClipboardPrefs::Off;
        self.advertised = ClipboardContents::empty();
        vec![self.send_list(Vec::new(), now)]
    }

    fn send_list(&mut self, formats: Vec<ClipFormat>, now: Instant) -> SyncAction {
        self.last_sent = Some((sorted_kinds(&formats), now));
        SyncAction::SendFormatList(formats)
    }

    /// Advertises the latest local contents (local now owns the clipboard: any remote fetch
    /// in progress is superseded).
    fn advertise_local(&mut self, now: Instant) -> Vec<SyncAction> {
        let contents = self.local.as_ref().map(|l| l.contents.clone()).unwrap_or_default();
        let (kept, rejected) = filter_local(&contents, self.prefs);
        let formats = outbound_formats(&kept, self.prefs);
        self.advertised = kept;
        self.pending_local = false;
        self.supersede_fetch();
        let mut actions: Vec<_> = rejected.into_iter().map(SyncAction::Rejected).collect();
        actions.push(self.send_list(formats, now));
        actions
    }

    fn supersede_fetch(&mut self) {
        self.fetch = None;
        if let Some(inflight) = &mut self.inflight {
            inflight.stale = true;
        }
    }

    fn on_local_change(
        &mut self,
        change_count: i64,
        contents: ClipboardContents,
        now: Instant,
    ) -> Vec<SyncAction> {
        if self.last_local_change_count == Some(change_count) {
            return vec![];
        }
        self.last_local_change_count = Some(change_count);
        let own = self.own_writes.contains(&change_count) || self.unacked_write.as_ref() == Some(&contents);
        self.local = Some(LocalSnapshot { contents, at: now });
        if own {
            return vec![];
        }
        if self.prefs == ClipboardPrefs::Off {
            return vec![];
        }
        if !self.can_sync() {
            self.pending_local = true;
            return vec![];
        }
        self.advertise_local(now)
    }

    fn on_prefs(&mut self, prefs: ClipboardPrefs, now: Instant) -> Vec<SyncAction> {
        if prefs == self.prefs {
            return vec![];
        }
        self.prefs = prefs;
        if !(self.ready && self.focused) {
            self.pending_local = self.local.is_some();
            return vec![];
        }
        if prefs == ClipboardPrefs::Off {
            self.supersede_fetch();
            self.advertised = ClipboardContents::empty();
            return vec![self.send_list(Vec::new(), now)];
        }
        if self.local.is_some() { self.advertise_local(now) } else { vec![] }
    }

    fn on_remote_list(&mut self, formats: &[ClipFormat], now: Instant) -> Vec<SyncAction> {
        if !self.can_sync() {
            return vec![];
        }
        let echo = self.last_sent.as_ref().is_some_and(|(kinds, at)| {
            now.duration_since(*at) <= ECHO_WINDOW && *kinds == sorted_kinds(formats)
        });
        if echo {
            return vec![];
        }
        // The remote side now owns the clipboard.
        self.supersede_fetch();
        let queue: VecDeque<_> = select_inbound(formats, self.prefs).into();
        if queue.is_empty() {
            return vec![];
        }
        self.fetch = Some(RemoteFetch { queue, items: Vec::new(), announced: now });
        if self.inflight.is_some() {
            // Wait for the superseded response before issuing the next request.
            return vec![];
        }
        self.next_fetch_step(now)
    }

    /// Issues the next queued request, or completes the fetch.
    fn next_fetch_step(&mut self, now: Instant) -> Vec<SyncAction> {
        let Some(fetch) = &mut self.fetch else { return vec![] };
        if let Some((format, kind)) = fetch.queue.pop_front() {
            self.inflight = Some(Inflight { kind, since: now, stale: false });
            return vec![SyncAction::RequestRemoteData(format)];
        }
        let Some(fetch) = self.fetch.take() else { return vec![] };
        // Newest wins: never overwrite a local change newer than the remote announcement.
        let local_newer = self.local.as_ref().is_some_and(|l| l.at > fetch.announced);
        if fetch.items.is_empty() || local_newer {
            return vec![];
        }
        let contents = ClipboardContents { items: fetch.items };
        self.unacked_write = Some(contents.clone());
        vec![SyncAction::WritePasteboard(contents)]
    }

    fn on_remote_data(&mut self, data: Option<Vec<u8>>, now: Instant) -> Vec<SyncAction> {
        let Some(inflight) = self.inflight.take() else { return vec![] };
        let mut actions = Vec::new();
        if !inflight.stale {
            let item = data.ok_or(ClipError::RemoteError).and_then(|d| decode_inbound(inflight.kind, &d));
            match item {
                Ok(item) => {
                    if let Some(fetch) = &mut self.fetch {
                        fetch.items.push(item);
                    }
                }
                Err(e) => actions.push(SyncAction::Rejected(e)),
            }
        }
        actions.extend(self.next_fetch_step(now));
        actions
    }

    fn on_data_request(&mut self, format_id: u32) -> Vec<SyncAction> {
        vec![SyncAction::SendData {
            format_id,
            data: encode_outbound(format_id, &self.advertised, self.prefs),
        }]
    }
}
