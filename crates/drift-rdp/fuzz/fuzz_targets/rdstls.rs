//! Fuzz target: the RDSTLS exchange (plan §1.3, tasks M0-2 / M3-1 / M9-2).
//!
//! On legs 2 and 3 the server speaks first: it sends an RDSTLS capabilities PDU and later an
//! AuthResponse, both parsed before Drift trusts anything about the connection. The target
//! decodes both from the input, re-encodes what decodes (a round-trip catches length
//! disagreements between the two directions), and walks the same `OneTimeCredentials` →
//! `RdstlsCredentials` conversion the connector performs.
//!
//! Errors are expected; panics, overflows and OOMs are bugs. Seed the corpus with
//! `fixtures/pdus/rdstls_*.bin`.
#![no_main]

use drift_rdp::rdstls::OneTimeCredentials;
use ironrdp_connector::rdstls::{RdstlsAuthResponse, RdstlsCapabilities, RdstlsResultCode};
use ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(caps) = ironrdp_core::decode::<RdstlsCapabilities>(data) {
        let _ = ironrdp_core::encode_vec(&caps);
    }
    if let Ok(response) = ironrdp_core::decode::<RdstlsAuthResponse>(data) {
        let _ = ironrdp_core::encode_vec(&response);
        let code: RdstlsResultCode = response.result_code;
        std::hint::black_box((code.is_success(), code.description()));
    }
    // The client half: one-time credentials taken from a redirection PDU and handed to the
    // connector. Both copies zeroize on drop, so this also exercises the M9-3 drop paths.
    if let Ok(pdu) = ironrdp_core::decode::<ServerRedirectionPdu>(data)
        && let Some(credentials) = OneTimeCredentials::from_redirection(&pdu)
    {
        std::hint::black_box(credentials.username().len());
        let connector = credentials.into_connector();
        std::hint::black_box(connector.password.len());
    }
});
