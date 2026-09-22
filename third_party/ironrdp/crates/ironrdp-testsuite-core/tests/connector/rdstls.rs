use ironrdp_connector::{ClientConnector, ClientConnectorState, Sequence as _, State as _};
use ironrdp_core::WriteBuf;
use ironrdp_pdu::nego::SecurityProtocol;

fn requested_protocol(connector: &mut ClientConnector) -> SecurityProtocol {
    let mut output = WriteBuf::new();
    connector.step(&[], None, &mut output).unwrap();

    match connector.state {
        ClientConnectorState::ConnectionInitiationWaitConfirm { requested_protocol } => requested_protocol,
        _ => panic!("unexpected connector state: {}", connector.state.name()),
    }
}

#[test]
fn redirected_leg_without_nla_requests_rdstls() {
    let mut connector = ClientConnector::new(super::test_config(), "127.0.0.1:3389".parse().unwrap())
        .with_load_balance_info("Cookie: msts=1234567890".to_owned());

    assert_eq!(
        requested_protocol(&mut connector),
        SecurityProtocol::SSL | SecurityProtocol::RDSTLS
    );
}

#[test]
fn plain_tls_connection_does_not_request_rdstls() {
    let mut connector = ClientConnector::new(super::test_config(), "127.0.0.1:3389".parse().unwrap());

    assert_eq!(requested_protocol(&mut connector), SecurityProtocol::SSL);
}

#[test]
fn load_balance_info_with_nla_does_not_request_rdstls() {
    let mut config = super::test_config();
    config.enable_credssp = true;
    let mut connector = ClientConnector::new(config, "127.0.0.1:3389".parse().unwrap())
        .with_load_balance_info("Cookie: msts=1234567890".to_owned());

    assert!(!requested_protocol(&mut connector).contains(SecurityProtocol::RDSTLS));
}
