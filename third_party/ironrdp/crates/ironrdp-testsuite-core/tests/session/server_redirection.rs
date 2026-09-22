//! A Server Redirection PDU received during the active session is surfaced to the caller
//! instead of failing with "unexpected share control PDU type".

use ironrdp_core::encode_vec;
use ironrdp_graphics::image_processing::PixelFormat;
use ironrdp_pdu::Action;
use ironrdp_pdu::mcs::SendDataIndication;
use ironrdp_pdu::rdp::headers::{ShareControlHeader, ShareControlPdu};
use ironrdp_pdu::rdp::server_redirection::{ServerRedirectionFlags, ServerRedirectionPdu};
use ironrdp_pdu::x224::X224;
use ironrdp_session::image::DecodedImage;
use ironrdp_session::x224::{Processor, ProcessorOutput};
use ironrdp_session::{ActiveStageBuilder, ActiveStageOutput};
use ironrdp_svc::StaticChannelSet;

const USER_CHANNEL_ID: u16 = 1002;
const IO_CHANNEL_ID: u16 = 1003;

fn redirection() -> ServerRedirectionPdu {
    ServerRedirectionPdu {
        session_id: 0,
        redirection_flags: ServerRedirectionFlags::LOAD_BALANCE_INFO
            | ServerRedirectionFlags::USERNAME
            | ServerRedirectionFlags::PASSWORD
            | ServerRedirectionFlags::PASSWORD_IS_PK_ENCRYPTED
            | ServerRedirectionFlags::REDIRECTION_GUID,
        load_balance_info: Some(b"Cookie: msts=3735928559\r\n".to_vec()),
        username: Some("drift-otp-user01".to_owned()),
        password: Some(vec![0xA5; 34]),
        redirection_guid: Some(vec![0x41, 0x00, 0x00, 0x00]),
        ..ServerRedirectionPdu::default()
    }
}

fn encode_redirection_frame(pdu: ServerRedirectionPdu) -> Vec<u8> {
    let control = ShareControlHeader {
        share_id: 0,
        pdu_source: USER_CHANNEL_ID,
        share_control_pdu: ShareControlPdu::ServerRedirect(pdu),
    };

    let indication = SendDataIndication {
        initiator_id: USER_CHANNEL_ID,
        channel_id: IO_CHANNEL_ID,
        user_data: encode_vec(&control).unwrap().into(),
    };

    encode_vec(&X224(indication)).unwrap()
}

#[test]
fn x224_processor_surfaces_server_redirection() {
    let mut processor = Processor::new(StaticChannelSet::new(), USER_CHANNEL_ID, IO_CHANNEL_ID, None, 0);

    let outputs = processor
        .process(&encode_redirection_frame(redirection()), &mut None)
        .unwrap();

    match outputs.as_slice() {
        [ProcessorOutput::ServerRedirect(pdu)] => assert_eq!(**pdu, redirection()),
        other => panic!("unexpected outputs: {other:?}"),
    }
}

#[test]
fn active_stage_surfaces_server_redirection() {
    let mut stage = ActiveStageBuilder {
        static_channels: StaticChannelSet::new(),
        user_channel_id: USER_CHANNEL_ID,
        io_channel_id: IO_CHANNEL_ID,
        message_channel_id: None,
        share_id: 0,
        compression_type: None,
        enable_server_pointer: false,
        pointer_software_rendering: false,
    }
    .build();
    let mut image = DecodedImage::new(PixelFormat::RgbA32, 64, 64);

    let outputs = stage
        .process(&mut image, Action::X224, &encode_redirection_frame(redirection()))
        .unwrap();

    let redirects: Vec<_> = outputs
        .iter()
        .filter_map(|output| match output {
            ActiveStageOutput::ServerRedirect(pdu) => Some(pdu),
            _ => None,
        })
        .collect();
    assert_eq!(redirects.len(), 1, "outputs: {outputs:?}");
    assert_eq!(*redirects[0].as_ref(), redirection());
}
