//! CLIPRDR wiring (task **M5-2**): IronRDP's `CliprdrClient` ↔ [`drift_clipboard::ClipboardSync`].
//!
//! The decisions live in `drift-clipboard` (format mapping M5-1, sync state); this module is
//! the humble adapter:
//!
//! - [`ClipboardBackend`] implements `CliprdrBackend` and only queues callbacks — IronRDP calls
//!   them inside `ActiveStage::process`, where the actor cannot send anything yet.
//! - [`ClipboardChannel::drain`] turns the queue into [`SyncInput`]s and returns the
//!   [`SyncAction`]s the actor must execute, in order.
//! - [`apply`] executes one action against the active stage: Format List, Format Data Request
//!   and Format Data Response become frames; a pasteboard write becomes a
//!   [`SessionEvent::ClipboardRemote`](crate::SessionEvent::ClipboardRemote).
//!
//! The **initial format-list request is always answered** (plan §1.6): without it IronRDP's
//! `Cliprdr` never leaves its initialization state and remote copies are ignored for the whole
//! session. `temporary_directory` is empty on purpose: the optional `CB_TEMP_DIRECTORY` PDU
//! makes g-r-d log a length warning and Drift never transfers files.

use std::sync::{Arc, Mutex, PoisonError};

use drift_clipboard::formats::ClipFormat;
use drift_clipboard::{ClipboardContents, ClipboardSync, SyncAction, SyncInput};
use drift_core::{ClipboardPrefs, Clock, DisconnectReason};
use ironrdp_cliprdr::backend::CliprdrBackend;
use ironrdp_cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardFormatName, ClipboardGeneralCapabilityFlags,
    FileContentsRequest, FileContentsResponse, FormatDataRequest, FormatDataResponse, LockDataId,
    OwnedFormatDataResponse,
};
use ironrdp_cliprdr::{CliprdrClient, CliprdrSvcMessages};
use ironrdp_core::impl_as_any;
use ironrdp_session::ActiveStage;
use ironrdp_cliprdr::Client;

/// A `CliprdrBackend` callback, handled outside `ActiveStage::process`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BackendEvent {
    /// The initialization sequence asks for our format list.
    InitialFormatList,
    /// The remote clipboard changed.
    RemoteCopy(Vec<ClipFormat>),
    /// The server wants our data in this format.
    DataRequest(u32),
    /// The server answered our request (`None` = error response).
    DataResponse(Option<Vec<u8>>),
}

/// Queue shared by the backend (called by IronRDP) and the actor.
type Queue = Arc<Mutex<Vec<BackendEvent>>>;

fn lock(queue: &Queue) -> std::sync::MutexGuard<'_, Vec<BackendEvent>> {
    queue.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The clipboard backend of one leg: queues callbacks, decides nothing.
#[derive(Debug)]
pub(crate) struct ClipboardBackend {
    queue: Queue,
}

impl_as_any!(ClipboardBackend);

impl ClipboardBackend {
    fn push(&self, event: BackendEvent) {
        lock(&self.queue).push(event);
    }
}

impl CliprdrBackend for ClipboardBackend {
    fn temporary_directory(&self) -> &str {
        // Empty: skip the optional CB_TEMP_DIRECTORY PDU (plan §1.6).
        ""
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES
    }

    fn on_ready(&mut self) {
        tracing::debug!("clipboard channel ready");
    }

    fn on_request_format_list(&mut self) {
        self.push(BackendEvent::InitialFormatList);
    }

    fn on_process_negotiated_capabilities(&mut self, capabilities: ClipboardGeneralCapabilityFlags) {
        tracing::debug!(?capabilities, "clipboard capabilities negotiated");
    }

    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        self.push(BackendEvent::RemoteCopy(available_formats.iter().map(from_ironrdp).collect()));
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        self.push(BackendEvent::DataRequest(request.format.value()));
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        let data = (!response.is_error()).then(|| response.data().to_vec());
        self.push(BackendEvent::DataResponse(data));
    }

    fn on_file_contents_request(&mut self, _request: FileContentsRequest) {}

    fn on_file_contents_response(&mut self, _response: FileContentsResponse<'_>) {}

    fn on_lock(&mut self, _data_id: LockDataId) {}

    fn on_unlock(&mut self, _data_id: LockDataId) {}
}

/// A CLIPRDR format as IronRDP spells it.
fn to_ironrdp(format: &ClipFormat) -> ClipboardFormat {
    let out = ClipboardFormat::new(ClipboardFormatId(format.id));
    match &format.name {
        Some(name) => out.with_name(ClipboardFormatName::new(name.clone())),
        None => out,
    }
}

/// A CLIPRDR format as Drift spells it.
fn from_ironrdp(format: &ClipboardFormat) -> ClipFormat {
    ClipFormat { id: format.id.value(), name: format.name.as_ref().map(|n| n.value().to_owned()) }
}

/// The clipboard state of one leg: the sync state machine plus the backend's queue.
pub(crate) struct ClipboardChannel {
    sync: ClipboardSync,
    queue: Queue,
}

impl std::fmt::Debug for ClipboardChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardChannel").field("sync", &self.sync).finish_non_exhaustive()
    }
}

impl ClipboardChannel {
    /// A fresh channel state for a leg (CLIPRDR restarts with every connection).
    pub(crate) fn new(prefs: ClipboardPrefs, focused: bool, clock: Arc<dyn Clock>) -> Self {
        Self { sync: ClipboardSync::new(prefs, focused, clock), queue: Queue::default() }
    }

    /// The `CliprdrClient` to register as a static channel on this leg.
    pub(crate) fn client(&self) -> CliprdrClient {
        CliprdrClient::new(Box::new(ClipboardBackend { queue: Arc::clone(&self.queue) }))
    }

    /// Feeds one input into the sync state.
    pub(crate) fn handle(&mut self, input: SyncInput) -> Vec<SyncAction> {
        self.sync.handle(input)
    }

    /// Turns the backend's queued callbacks into actions.
    pub(crate) fn drain(&mut self) -> Vec<SyncAction> {
        let events = std::mem::take(&mut *lock(&self.queue));
        events
            .into_iter()
            .flat_map(|event| {
                let input = match event {
                    BackendEvent::InitialFormatList => SyncInput::InitialFormatListRequested,
                    BackendEvent::RemoteCopy(formats) => SyncInput::RemoteFormatList(formats),
                    BackendEvent::DataRequest(format_id) => SyncInput::RemoteDataRequest { format_id },
                    BackendEvent::DataResponse(data) => SyncInput::RemoteData(data),
                };
                self.sync.handle(input)
            })
            .collect()
    }
}

/// What executing a [`SyncAction`] produced.
pub(crate) enum ClipOutcome {
    /// Bytes to write on the wire.
    Frame(Vec<u8>),
    /// Contents for the local pasteboard (the app writes them and reports the change back).
    Write(ClipboardContents),
}

/// Executes one clipboard action against the active stage.
pub(crate) fn apply(
    stage: &mut ActiveStage,
    action: SyncAction,
) -> Result<Option<ClipOutcome>, DisconnectReason> {
    let messages = match action {
        SyncAction::SendFormatList(formats) => {
            let list: Vec<ClipboardFormat> = formats.iter().map(to_ironrdp).collect();
            let Some(cliprdr) = stage.get_svc_processor_mut::<CliprdrClient>() else { return Ok(None) };
            cliprdr.initiate_copy(&list).map_err(|e| protocol("advertise clipboard formats", &e))?
        }
        SyncAction::RequestRemoteData(format) => {
            let Some(cliprdr) = stage.get_svc_processor_mut::<CliprdrClient>() else { return Ok(None) };
            cliprdr
                .initiate_paste(ClipboardFormatId(format.id))
                .map_err(|e| protocol("request clipboard data", &e))?
        }
        SyncAction::SendData { format_id, data } => {
            let response = match data {
                Ok(bytes) => OwnedFormatDataResponse::new_data(bytes),
                Err(e) => {
                    tracing::warn!(error = %e, format_id, "answering a clipboard request with an error");
                    OwnedFormatDataResponse::new_error()
                }
            };
            let Some(cliprdr) = stage.get_svc_processor_mut::<CliprdrClient>() else { return Ok(None) };
            cliprdr.submit_format_data(response).map_err(|e| protocol("answer clipboard request", &e))?
        }
        SyncAction::WritePasteboard(contents) => return Ok(Some(ClipOutcome::Write(contents))),
        SyncAction::Rejected(e) => {
            tracing::warn!(error = %e, "clipboard payload refused");
            return Ok(None);
        }
    };
    let frame = encode(stage, messages)?;
    Ok(Some(ClipOutcome::Frame(frame)))
}

fn encode(stage: &ActiveStage, messages: CliprdrSvcMessages<Client>) -> Result<Vec<u8>, DisconnectReason> {
    stage.process_svc_processor_messages(messages).map_err(|e| protocol("encode clipboard messages", &e))
}

fn protocol(context: &str, error: &dyn std::fmt::Display) -> DisconnectReason {
    DisconnectReason::ProtocolError(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use drift_core::SystemClock;
    use ironrdp_cliprdr::pdu::{Capabilities, ClipboardPdu, GeneralCapabilitySet};
    use ironrdp_core::encode_vec;
    use ironrdp_svc::SvcProcessor as _;

    use super::*;

    /// A backend that ignores the initial format-list request, like a client that forgets to
    /// answer it (plan §1.6: the channel then never becomes ready).
    #[derive(Debug, Default)]
    struct SilentBackend;

    impl_as_any!(SilentBackend);

    impl CliprdrBackend for SilentBackend {
        fn temporary_directory(&self) -> &str {
            ""
        }
        fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
            ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES
        }
        fn on_ready(&mut self) {}
        fn on_request_format_list(&mut self) {}
        fn on_process_negotiated_capabilities(&mut self, _c: ClipboardGeneralCapabilityFlags) {}
        fn on_remote_copy(&mut self, _f: &[ClipboardFormat]) {}
        fn on_format_data_request(&mut self, _r: FormatDataRequest) {}
        fn on_format_data_response(&mut self, _r: FormatDataResponse<'_>) {}
        fn on_file_contents_request(&mut self, _r: FileContentsRequest) {}
        fn on_file_contents_response(&mut self, _r: FileContentsResponse<'_>) {}
        fn on_lock(&mut self, _id: LockDataId) {}
        fn on_unlock(&mut self, _id: LockDataId) {}
    }

    /// The server's initialization sequence: Capabilities, then Monitor Ready.
    fn server_init(cliprdr: &mut CliprdrClient) {
        let caps = ClipboardPdu::Capabilities(Capabilities {
            capabilities: vec![
                GeneralCapabilitySet {
                    version: ironrdp_cliprdr::pdu::ClipboardProtocolVersion::V2,
                    general_flags: ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES,
                }
                .into(),
            ],
        });
        cliprdr.process(&encode_vec(&caps).unwrap()).unwrap();
        cliprdr.process(&encode_vec(&ClipboardPdu::MonitorReady).unwrap()).unwrap();
    }

    #[test]
    fn a_backend_that_ignores_the_initial_request_never_becomes_ready() {
        let mut cliprdr = CliprdrClient::new(Box::new(SilentBackend));
        server_init(&mut cliprdr);
        assert!(
            cliprdr.initiate_paste(ClipboardFormatId(13)).is_err(),
            "without an answer the channel stays in initialization"
        );
    }

    #[test]
    fn answering_the_initial_request_makes_the_channel_ready() {
        let mut channel =
            ClipboardChannel::new(ClipboardPrefs::TextAndImages, true, Arc::new(SystemClock));
        let mut cliprdr = channel.client();
        server_init(&mut cliprdr);
        // The backend queued the request; the sync state answers it with a (possibly empty) list.
        let actions = channel.drain();
        assert_eq!(actions, vec![SyncAction::SendFormatList(Vec::new())]);
        let SyncAction::SendFormatList(formats) = &actions[0] else { unreachable!() };
        let list: Vec<ClipboardFormat> = formats.iter().map(to_ironrdp).collect();
        cliprdr.initiate_copy(&list).unwrap();
        // The server's Format List Response completes the initialization.
        let ok = ClipboardPdu::FormatListResponse(ironrdp_cliprdr::pdu::FormatListResponse::Ok);
        cliprdr.process(&encode_vec(&ok).unwrap()).unwrap();
        assert!(cliprdr.initiate_paste(ClipboardFormatId(13)).is_ok(), "the channel is ready");
    }

    #[test]
    fn formats_round_trip_through_ironrdp() {
        for format in [ClipFormat::standard(13), ClipFormat::named(0xC0F0, "image/png")] {
            assert_eq!(from_ironrdp(&to_ironrdp(&format)), format);
        }
    }
}
