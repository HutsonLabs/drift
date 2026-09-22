//! `CLIPRDR_TEMP_DIRECTORY` ([MS-RDPECLIP] 2.2.2.3).
//!
//! The PDU carries a fixed 520-byte `wszTempDir` (260 UTF-16 code units, null-terminated), so
//! `dataLen` is always 520. FreeRDP 3.x servers (GNOME Remote Desktop) log "header told length
//! is 520, but actually read 0" for every such PDU because their handler does not advance the
//! stream; the PDU is optional, so a client that has no temporary directory for file transfers
//! should not send it at all.

use ironrdp_cliprdr::CliprdrClient;
use ironrdp_cliprdr::backend::CliprdrBackend;
use ironrdp_cliprdr::pdu::{
    ClientTemporaryDirectory, ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, ClipboardPdu,
    FileContentsRequest, FileContentsResponse, FormatDataRequest, FormatDataResponse, LockDataId,
};
use ironrdp_core::{AsAny, decode, encode_vec};
use ironrdp_svc::{SvcMessage, SvcProcessor as _};

use super::test_helpers::{TestBackend, monitor_ready_pdu, real_now_ms, server_capabilities_pdu};

const DATA_LEN: usize = 520;

fn temp_dir_pdu_bytes(path: &str) -> Vec<u8> {
    let mut bytes = vec![0x06, 0x00, 0x00, 0x00]; // msgType = CB_TEMP_DIRECTORY, msgFlags = 0
    bytes.extend_from_slice(&u32::try_from(DATA_LEN).unwrap().to_le_bytes());
    let mut dir: Vec<u8> = path.encode_utf16().flat_map(u16::to_le_bytes).collect();
    dir.resize(DATA_LEN, 0);
    bytes.extend_from_slice(&dir);
    bytes
}

#[test]
fn temporary_directory_round_trips_with_520_byte_data_len() {
    let pdu = ClipboardPdu::TemporaryDirectory(ClientTemporaryDirectory::new("/tmp").unwrap());

    let encoded = encode_vec(&pdu).unwrap();
    assert_eq!(encoded, temp_dir_pdu_bytes("/tmp"));
    assert_eq!(encoded.len(), 8 + DATA_LEN);

    let decoded: ClipboardPdu<'_> = decode(&encoded).unwrap();
    assert_eq!(decoded, pdu);
    match decoded {
        ClipboardPdu::TemporaryDirectory(dir) => assert_eq!(dir.temporary_directory_path().unwrap(), "/tmp"),
        other => panic!("unexpected PDU: {}", other.message_name()),
    }
}

#[test]
fn longest_path_fits_with_terminator() {
    let path = "a".repeat(259);
    let pdu = ClipboardPdu::TemporaryDirectory(ClientTemporaryDirectory::new(&path).unwrap());

    let encoded = encode_vec(&pdu).unwrap();
    assert_eq!(encoded.len(), 8 + DATA_LEN);
    assert_eq!(&encoded[encoded.len() - 2..], [0, 0]);
}

#[test]
fn path_without_room_for_terminator_is_rejected() {
    assert!(ClientTemporaryDirectory::new(&"a".repeat(260)).is_err());
}

#[test]
fn decode_rejects_data_len_mismatch() {
    let mut bytes = temp_dir_pdu_bytes("/tmp");
    bytes[4..8].copy_from_slice(&0u32.to_le_bytes());

    assert!(decode::<ClipboardPdu<'_>>(bytes.as_slice()).is_err());
}

#[test]
fn decode_rejects_unterminated_path() {
    let mut bytes = temp_dir_pdu_bytes("");
    for byte in &mut bytes[8..] {
        *byte = b'a';
    }

    assert!(decode::<ClipboardPdu<'_>>(bytes.as_slice()).is_err());
}

fn initial_copy_pdus(backend: Box<dyn CliprdrBackend>) -> Vec<Vec<u8>> {
    let mut cliprdr = CliprdrClient::new(backend);
    cliprdr.process(&server_capabilities_pdu()).unwrap();
    cliprdr.process(&monitor_ready_pdu()).unwrap();

    let formats = vec![ClipboardFormat::new(ClipboardFormatId::new(13))];
    let messages: Vec<SvcMessage> = cliprdr.initiate_copy(&formats).unwrap().into();
    messages
        .into_iter()
        .map(|message| message.encode_unframed_pdu().unwrap())
        .collect()
}

fn message_names(pdus: &[Vec<u8>]) -> Vec<&'static str> {
    pdus.iter()
        .map(|bytes| decode::<ClipboardPdu<'_>>(bytes.as_slice()).unwrap().message_name())
        .collect()
}

#[test]
fn client_sends_temporary_directory_when_backend_has_one() {
    let pdus = initial_copy_pdus(Box::new(TestBackend));

    assert_eq!(
        message_names(&pdus),
        ["CLIPRDR_CAPABILITIES", "CLIPRDR_TEMP_DIRECTORY", "CLIPRDR_FORMAT_LIST"]
    );
    assert_eq!(pdus[1], temp_dir_pdu_bytes("/tmp"));
}

#[derive(Debug)]
struct NoTempDirBackend;

impl CliprdrBackend for NoTempDirBackend {
    fn temporary_directory(&self) -> &str {
        ""
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES
    }

    fn on_ready(&mut self) {}
    fn on_request_format_list(&mut self) {}
    fn on_process_negotiated_capabilities(&mut self, _capabilities: ClipboardGeneralCapabilityFlags) {}
    fn on_remote_copy(&mut self, _available_formats: &[ClipboardFormat]) {}
    fn on_format_data_request(&mut self, _request: FormatDataRequest) {}
    fn on_format_data_response(&mut self, _response: FormatDataResponse<'_>) {}
    fn on_file_contents_request(&mut self, _request: FileContentsRequest) {}
    fn on_file_contents_response(&mut self, _response: FileContentsResponse<'_>) {}
    fn on_lock(&mut self, _data_id: LockDataId) {}
    fn on_unlock(&mut self, _data_id: LockDataId) {}

    fn now_ms(&self) -> u64 {
        real_now_ms()
    }

    fn elapsed_ms(&self, since: u64) -> u64 {
        self.now_ms().saturating_sub(since)
    }
}

impl AsAny for NoTempDirBackend {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

#[test]
fn client_omits_optional_temporary_directory_when_backend_has_none() {
    let pdus = initial_copy_pdus(Box::new(NoTempDirBackend));

    assert_eq!(message_names(&pdus), ["CLIPRDR_CAPABILITIES", "CLIPRDR_FORMAT_LIST"]);
}
