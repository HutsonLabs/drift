//! Fuzz target: the clipboard channel (tasks M5-1 / M5-2 / M9-2).
//!
//! Everything on CLIPRDR is remote data: format lists with server-chosen names and ids, and
//! Format Data Responses whose payload Drift parses as UTF-16 text, PNG or a DIB (where a
//! bogus `biWidth`/`biHeight` used to be a classic overflow). The target chains the two
//! layers Drift actually runs:
//!
//! 1. `ironrdp_cliprdr::pdu::ClipboardPdu` decoding of the raw channel bytes;
//! 2. the decoded PDU as a `SyncInput` into `drift_clipboard::ClipboardSync`, plus the format
//!    converters on the raw payload.
//!
//! Seed the corpus with `fixtures/clipboard/*.bin` and the DIB fixtures in
//! `crates/drift-clipboard/tests/fixtures/`.
#![no_main]

use std::sync::Arc;
use std::time::Instant;

use drift_clipboard::formats::{self, ClipFormat, FormatKind};
use drift_clipboard::{ClipboardSync, SyncInput};
use drift_core::{ClipboardPrefs, Clock};
use ironrdp_cliprdr::pdu::ClipboardPdu;
use libfuzzer_sys::fuzz_target;

/// A clock that never moves: the sync state machine only needs monotonic reads here.
struct FrozenClock(Instant);

impl Clock for FrozenClock {
    fn now(&self) -> Instant {
        self.0
    }
}

/// Every conversion Drift may run on a remote payload.
fn convert(payload: &[u8]) {
    for kind in [FormatKind::UnicodeText, FormatKind::AnsiText, FormatKind::Png, FormatKind::Dib] {
        let _ = formats::decode_inbound(kind, payload);
    }
    let _ = formats::dib_to_png(payload);
    let _ = formats::png_to_dib(payload);
    let _ = formats::png_passthrough(payload);
    let _ = formats::check_size(payload.len());
}

fuzz_target!(|data: &[u8]| {
    let clock = Arc::new(FrozenClock(Instant::now()));
    let mut sync = ClipboardSync::new(ClipboardPrefs::TextAndImages, true, clock);
    let _ = sync.handle(SyncInput::InitialFormatListRequested);

    if let Ok(pdu) = ironrdp_core::decode::<ClipboardPdu<'_>>(data) {
        let input = match pdu {
            ClipboardPdu::FormatList(list) => {
                // Both framings: g-r-d sends long format names, older peers short ones.
                let formats: Vec<ClipFormat> = [true, false]
                    .iter()
                    .filter_map(|long| list.get_formats(*long).ok())
                    .flatten()
                    .map(|f| match f.name() {
                        Some(name) => ClipFormat::named(f.id().value(), name.value()),
                        None => ClipFormat::standard(f.id().value()),
                    })
                    .collect();
                Some(SyncInput::RemoteFormatList(formats))
            }
            ClipboardPdu::FormatDataRequest(request) => {
                Some(SyncInput::RemoteDataRequest { format_id: request.format.value() })
            }
            ClipboardPdu::FormatDataResponse(response) => {
                let payload = (!response.is_error()).then(|| response.data().to_vec());
                if let Some(payload) = payload.as_deref() {
                    convert(payload);
                }
                Some(SyncInput::RemoteData(payload))
            }
            _ => None,
        };
        if let Some(input) = input {
            for action in sync.handle(input) {
                std::hint::black_box(format!("{action:?}").len());
            }
        }
    }
    // A Format Data Response payload that never made it through the PDU layer is still
    // attacker-controlled input for the converters.
    convert(data);
});
