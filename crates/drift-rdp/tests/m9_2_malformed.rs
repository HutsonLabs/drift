//! M9-2 Red: malformed server input never panics and reaches the app as `ProtocolError`.
//!
//! The fuzz targets in `crates/drift-rdp/fuzz`, `crates/drift-gfx/fuzz` and
//! `crates/drift-macos/fuzz` hunt for crashes overnight; these tests are their fast,
//! deterministic counterpart in the merge gate:
//!
//! - a **loopback** test drives the real actor with a `FakeServer` that sends garbage on the
//!   graphics channel, and asserts the session ends in `DisconnectReason::ProtocolError`
//!   rather than a panic or a hang;
//! - **sweeps** feed every parser Drift owns (the same entry points the fuzz targets use) with
//!   damaged versions of the captured g-r-d PDUs plus random noise, and only require that they
//!   return.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{Harness, PASS, USER, profile};
use drift_clipboard::formats::{self, FormatKind};
use drift_core::{ClipboardPrefs, ConnectMode, DisconnectReason, SessionState};
use drift_rdp::pointer;
use drift_rdp::redirect::RedirectLoop;
use drift_testkit::fixtures::{self, names};
use drift_testkit::{Channels, FakeServer, LegScript, ServerAction, TestCert};
use ironrdp_pdu::mcs::SendDataIndication;
use ironrdp_pdu::rdp::headers::{ShareControlHeader, ShareControlPdu};
use ironrdp_pdu::x224::X224;

const WAIT: Duration = Duration::from_secs(20);

/// A tiny xorshift PRNG: the sweeps must be reproducible in CI.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 { 0 } else { (self.next_u64() % bound as u64) as usize }
    }

    fn byte(&mut self) -> u8 {
        (self.next_u64() >> 24) as u8
    }

    /// `count` damaged variants of `seed`: byte flips, truncations and splices, plus a few
    /// degenerate inputs and pure noise. This is what a real damaged frame looks like, and it
    /// reaches far deeper into a parser than random bytes alone.
    fn damaged(&mut self, seed: &[u8], count: usize) -> Vec<Vec<u8>> {
        let mut out = vec![Vec::new(), vec![0; 1], seed.to_vec(), vec![0xFF; 64], vec![0; 4096]];
        for _ in 0..count {
            let mut input = seed.to_vec();
            match self.next_u64() % 4 {
                0 => input.truncate(self.below(input.len().max(1))),
                1 => {
                    let extra = self.below(64);
                    input.extend((0..extra).map(|_| self.byte()));
                }
                2 => input = (0..self.below(512)).map(|_| self.byte()).collect(),
                _ => {}
            }
            for _ in 0..1 + self.below(6) {
                if input.is_empty() {
                    break;
                }
                let at = self.below(input.len());
                input[at] = self.byte();
            }
            out.push(input);
        }
        out
    }
}

/// The Server Redirection PDU inside a captured X.224 frame, if it still decodes.
fn decode_redirection(frame: &[u8]) -> Option<ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu> {
    let X224(sdi) = ironrdp_core::decode::<X224<SendDataIndication<'_>>>(frame).ok()?;
    let header = ironrdp_core::decode::<ShareControlHeader>(sdi.user_data.as_ref()).ok()?;
    match header.share_control_pdu {
        ShareControlPdu::ServerRedirect(pdu) => Some(pdu),
        _ => None,
    }
}

// ------------------------------------------------------------------ loopback

/// Garbage on the graphics channel ends the session with `ProtocolError`, not a panic.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_graphics_bytes_end_the_session_with_a_protocol_error() {
    let cert = TestCert::generate("127.0.0.1");
    // A RDP_SEGMENTED_DATA header claiming one uncompressed segment, followed by a truncated
    // GFX PDU header: the shape a damaged g-r-d frame has.
    let garbage = vec![0xE0, 0x0A, 0x00, 0xFF, 0xFF, 0x01];
    let server = FakeServer::start(vec![
        LegScript::nla(cert.clone(), USER, PASS)
            .with_channels(Channels::all())
            .then(ServerAction::Wait(Duration::from_millis(50)))
            .then(ServerAction::GfxRaw(garbage)),
    ])
    .await
    .unwrap();

    let mut h = Harness::start(profile(ConnectMode::Headless, server.port(), Some(cert.fingerprint())), PASS);
    let state = h.wait_terminal(WAIT).await;
    let reason = match &state {
        SessionState::Disconnected { reason } | SessionState::Failed { reason } => reason.clone(),
        other => panic!("expected a terminal state, got {other:?}"),
    };
    assert!(
        matches!(reason, DisconnectReason::ProtocolError(_)),
        "malformed graphics input must surface as ProtocolError, got {reason:?}"
    );
}

// -------------------------------------------------------------------- sweeps

/// Redirection: damaged copies of the captured g-r-d redirection PDUs.
#[test]
fn redirection_sweep_never_panics() {
    let mut rng = Rng(0x5eed_0001);
    let mut followed = 0_usize;
    for seed in [names::SERVER_REDIRECTION_LEG1, names::SERVER_REDIRECTION_LEG2] {
        for input in rng.damaged(&fixtures::read(seed), 2_000) {
            let Some(mut pdu) = decode_redirection(&input) else { continue };
            followed += 1;
            let err = RedirectLoop::new(ConnectMode::RemoteLogin, "h", 3389).on_redirect(&mut pdu).err();
            assert!(
                !matches!(err, Some(DisconnectReason::Network | DisconnectReason::AuthFailed)),
                "a malformed redirect is a protocol error, not a transport one: {err:?}"
            );
        }
    }
    assert!(followed > 100, "the sweep must reach the redirect loop, not just the X.224 header");
}

/// RDSTLS: damaged capability and AuthResponse PDUs.
#[test]
fn rdstls_sweep_never_panics() {
    use ironrdp_connector::rdstls::{RdstlsAuthResponse, RdstlsCapabilities};

    let mut rng = Rng(0x5eed_0002);
    let seeds = [
        names::RDSTLS_CAPS,
        names::RDSTLS_AUTH_RESPONSE_SUCCESS,
        names::RDSTLS_AUTH_RESPONSE_LOGON_FAILURE,
        names::RDSTLS_AUTH_REQUEST_LEG2,
    ];
    for seed in seeds {
        for input in rng.damaged(&fixtures::read(seed), 1_000) {
            let _ = ironrdp_core::decode::<RdstlsCapabilities>(&input);
            let _ = ironrdp_core::decode::<RdstlsAuthResponse>(&input);
        }
    }
}

/// CLIPRDR: damaged format-data payloads through Drift's converters.
#[test]
fn cliprdr_sweep_never_panics() {
    let mut rng = Rng(0x5eed_0003);
    let kinds = [FormatKind::UnicodeText, FormatKind::AnsiText, FormatKind::Png, FormatKind::Dib];
    let seeds = [
        fixtures::read(names::CLIP_REMOTE_PNG),
        fixtures::read(names::CLIP_REMOTE_UNICODETEXT),
        read_dib("dib_24_bottomup.bin"),
        read_dib("dib_32_topdown_bitfields.bin"),
    ];
    for seed in &seeds {
        for input in rng.damaged(seed, 400) {
            for kind in kinds {
                let _ = formats::decode_inbound(kind, &input);
            }
            let _ = formats::dib_to_png(&input);
            let _ = formats::png_to_dib(&input);
            let _ = formats::png_passthrough(&input);
            let _ = formats::decode_unicode_text(&input);
            let _ = formats::decode_ansi_text(&input);
            let _ = formats::select_inbound(
                &[formats::ClipFormat::standard(u32::from(input.first().copied().unwrap_or(0)))],
                ClipboardPrefs::TextAndImages,
            );
        }
    }
}

/// One of the M5-1 DIB fixtures, which live next to the clipboard tests, not in `fixtures/`.
fn read_dib(name: &str) -> Vec<u8> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../drift-clipboard/tests/fixtures").join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    assert!(!bytes.is_empty(), "{} is empty", path.display());
    bytes
}

/// Pointer: IronRDP's decoders plus Drift's premultiply step (the full decoder is fuzzed in
/// `crates/drift-macos/fuzz`, where `PointerDecoder` lives).
#[test]
fn pointer_sweep_never_panics() {
    use ironrdp_graphics::pointer::{DecodedPointer, PointerBitmapTarget};
    use ironrdp_pdu::pointer::{ColorPointerAttribute, LargePointerAttribute, PointerAttribute};

    let mut rng = Rng(0x5eed_0004);
    let records = fixtures::records(names::FASTPATH_POINTER_SCALE100);
    let seeds: Vec<Vec<u8>> = records.iter().map(|r| r.to_vec()).collect();
    for seed in &seeds {
        for input in rng.damaged(seed, 200) {
            if let Ok(attr) = ironrdp_core::decode::<PointerAttribute<'_>>(&input) {
                let _ = DecodedPointer::decode_pointer_attribute(&attr, PointerBitmapTarget::Accelerated);
            }
            if let Ok(attr) = ironrdp_core::decode::<ColorPointerAttribute<'_>>(&input) {
                let _ =
                    DecodedPointer::decode_color_pointer_attribute(&attr, PointerBitmapTarget::Accelerated);
            }
            if let Ok(attr) = ironrdp_core::decode::<LargePointerAttribute<'_>>(&input) {
                let _ =
                    DecodedPointer::decode_large_pointer_attribute(&attr, PointerBitmapTarget::Accelerated);
            }
            let _ = pointer::rgba_to_premultiplied_bgra(&input);
        }
    }
}
