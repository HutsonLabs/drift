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

use std::sync::Arc;
use std::time::{Duration, Instant};

use drift_core::{ClipboardPrefs, Clock};

use crate::ClipboardContents;
use crate::formats::{ClipError, ClipFormat};

/// A remote fetch that has not completed after this long is abandoned.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// A remote list equal to our last advertisement within this window is treated as an echo.
pub const ECHO_WINDOW: Duration = Duration::from_millis(500);

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

/// CLIPRDR sync state for one session.
pub struct ClipboardSync {
    clock: Arc<dyn Clock>,
    prefs: ClipboardPrefs,
    focused: bool,
}

impl std::fmt::Debug for ClipboardSync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardSync")
            .field("prefs", &self.prefs)
            .field("focused", &self.focused)
            .finish_non_exhaustive()
    }
}

impl ClipboardSync {
    /// A new sync state; not ready until the initial format list has been answered.
    pub fn new(prefs: ClipboardPrefs, focused: bool, clock: Arc<dyn Clock>) -> Self {
        Self { clock, prefs, focused }
    }

    /// `true` once the server's initial format-list request has been answered.
    pub fn is_ready(&self) -> bool {
        todo!()
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
        let _: Instant = self.clock.now();
        let _ = input;
        todo!()
    }
}
