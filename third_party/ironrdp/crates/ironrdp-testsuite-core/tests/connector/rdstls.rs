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

mod exchange {
    use ironrdp_connector::rdstls::{
        RdstlsAuthRequest, RdstlsAuthResponse, RdstlsCapabilities, RdstlsCredentials, RdstlsResultCode,
        RdstlsVersions,
    };
    use ironrdp_connector::{ClientConnector, ClientConnectorState, ConnectorErrorKind, Sequence as _, State as _};
    use ironrdp_core::{WriteBuf, decode, encode_vec};
    use ironrdp_pdu::nego::{ConnectionConfirm, ResponseFlags, SecurityProtocol};
    use ironrdp_pdu::rdp::server_redirection::{ServerRedirectionFlags, ServerRedirectionPdu};
    use ironrdp_pdu::x224::X224;

    /// Server RDSTLS capabilities as sent by g-r-d 50 (supported versions v1|v2).
    const SERVER_CAPABILITIES: [u8; 8] = [0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00];
    const AUTH_RESPONSE_SUCCESS: [u8; 10] = [0x01, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
    const AUTH_RESPONSE_LOGON_FAILURE: [u8; 10] = [0x01, 0x00, 0x04, 0x00, 0x01, 0x00, 0x2e, 0x05, 0x00, 0x00];

    /// AuthRequest for `credentials()` ([MS-RDPBCGR] 2.2.17.2, plan §1.3).
    #[rustfmt::skip]
    const EXPECTED_AUTH_REQUEST: [u8; 29] = [
        0x01, 0x00, // version = RDSTLS_VERSION_1
        0x02, 0x00, // pduType = RDSTLS_TYPE_AUTHREQ
        0x01, 0x00, // dataType = RDSTLS_DATA_PASSWORD_CREDS
        0x04, 0x00, 0x47, 0x00, 0x00, 0x00, // redirectionGuid
        0x06, 0x00, 0x61, 0x00, 0x62, 0x00, 0x00, 0x00, // userName "ab\0"
        0x02, 0x00, 0x00, 0x00, // domain ""
        0x03, 0x00, 0xA5, 0x5A, 0x00, // password blob
    ];

    fn credentials() -> RdstlsCredentials {
        RdstlsCredentials {
            redirection_guid: vec![0x47, 0x00, 0x00, 0x00],
            username: "ab".to_owned(),
            domain: String::new(),
            password: vec![0xA5, 0x5A, 0x00],
        }
    }

    #[test]
    fn decodes_server_capabilities() {
        let caps = decode::<RdstlsCapabilities>(SERVER_CAPABILITIES.as_slice()).unwrap();

        assert_eq!(caps.supported_versions, RdstlsVersions::V1 | RdstlsVersions::V2);
        assert_eq!(encode_vec(&caps).unwrap(), SERVER_CAPABILITIES);
    }

    #[test]
    fn rejects_capabilities_with_wrong_type() {
        let mut bytes = SERVER_CAPABILITIES;
        bytes[2] = 0x02;

        assert!(decode::<RdstlsCapabilities>(bytes.as_slice()).is_err());
    }

    #[test]
    fn encodes_auth_request_with_password_credentials() {
        let request = RdstlsAuthRequest::from(&credentials());

        assert_eq!(encode_vec(&request).unwrap(), EXPECTED_AUTH_REQUEST);
    }

    #[test]
    fn empty_domain_is_a_single_null_character() {
        let request = RdstlsAuthRequest::from(&credentials());
        let bytes = encode_vec(&request).unwrap();

        assert_eq!(&bytes[20..24], [0x02, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn decodes_auth_response_success() {
        let response = decode::<RdstlsAuthResponse>(AUTH_RESPONSE_SUCCESS.as_slice()).unwrap();

        assert_eq!(response.result_code, RdstlsResultCode::SUCCESS);
        assert!(response.result_code.is_success());
    }

    #[test]
    fn decodes_auth_response_logon_failure() {
        let response = decode::<RdstlsAuthResponse>(AUTH_RESPONSE_LOGON_FAILURE.as_slice()).unwrap();

        assert_eq!(response.result_code, RdstlsResultCode::LOGON_FAILURE);
        assert_eq!(response.result_code.0, 0x52E);
        assert!(!response.result_code.is_success());
        assert_eq!(encode_vec(&response).unwrap(), AUTH_RESPONSE_LOGON_FAILURE);
    }

    #[test]
    fn rejects_truncated_auth_response() {
        for len in 0..AUTH_RESPONSE_SUCCESS.len() {
            assert!(decode::<RdstlsAuthResponse>(&AUTH_RESPONSE_SUCCESS[..len]).is_err());
        }
    }

    #[test]
    fn credentials_from_server_redirection() {
        let redirection = ServerRedirectionPdu {
            redirection_flags: ServerRedirectionFlags::LOAD_BALANCE_INFO
                | ServerRedirectionFlags::USERNAME
                | ServerRedirectionFlags::PASSWORD
                | ServerRedirectionFlags::REDIRECTION_GUID,
            load_balance_info: Some(b"Cookie: msts=1\r\n".to_vec()),
            username: Some("ab".to_owned()),
            password: Some(vec![0xA5, 0x5A, 0x00]),
            redirection_guid: Some(vec![0x47, 0x00, 0x00, 0x00]),
            ..ServerRedirectionPdu::default()
        };

        assert_eq!(
            RdstlsCredentials::from_server_redirection(&redirection),
            Some(credentials())
        );

        let without_password = ServerRedirectionPdu {
            password: None,
            ..redirection
        };
        assert_eq!(RdstlsCredentials::from_server_redirection(&without_password), None);
    }

    #[test]
    fn credentials_debug_redacts_password() {
        let debug = format!("{:?}", credentials());

        assert!(!debug.contains("165"), "{debug}");
        assert!(!debug.to_lowercase().contains("a5"), "{debug}");
    }

    fn connector_until_security_upgrade(credentials: Option<RdstlsCredentials>) -> ClientConnector {
        let mut connector = ClientConnector::new(super::super::test_config(), "127.0.0.1:3389".parse().unwrap())
            .with_load_balance_info("Cookie: msts=1".to_owned());
        if let Some(credentials) = credentials {
            connector = connector.with_rdstls_credentials(credentials);
        }

        let mut output = WriteBuf::new();
        connector.step(&[], None, &mut output).unwrap();

        let confirm = encode_vec(&X224(ConnectionConfirm::Response {
            flags: ResponseFlags::empty(),
            protocol: SecurityProtocol::RDSTLS,
        }))
        .unwrap();
        output.clear();
        connector.step(&confirm, None, &mut output).unwrap();
        assert!(connector.should_perform_security_upgrade());
        connector.mark_security_upgrade_as_done();
        connector
    }

    #[test]
    fn connector_runs_rdstls_after_tls_upgrade() {
        let mut connector = connector_until_security_upgrade(Some(credentials()));
        assert!(matches!(connector.state, ClientConnectorState::RdstlsCapabilities { .. }));

        // The server capabilities are a fixed 8-byte PDU.
        let hint = connector.next_pdu_hint().expect("waits for server capabilities");
        assert_eq!(hint.find_size(&[]).unwrap(), Some((true, 8)));

        let mut output = WriteBuf::new();
        let written = connector.step(&SERVER_CAPABILITIES, None, &mut output).unwrap();
        assert_eq!(written.size(), Some(EXPECTED_AUTH_REQUEST.len()));
        assert_eq!(output.filled(), EXPECTED_AUTH_REQUEST);
        assert!(matches!(connector.state, ClientConnectorState::RdstlsAuthResponse { .. }));

        let hint = connector.next_pdu_hint().expect("waits for auth response");
        assert_eq!(hint.find_size(&[]).unwrap(), Some((true, 10)));

        output.clear();
        connector.step(&AUTH_RESPONSE_SUCCESS, None, &mut output).unwrap();
        match connector.state {
            ClientConnectorState::BasicSettingsExchangeSendInitial { selected_protocol } => {
                assert_eq!(selected_protocol, SecurityProtocol::RDSTLS)
            }
            _ => panic!("unexpected state: {}", connector.state.name()),
        }
    }

    #[test]
    fn connector_reports_rdstls_logon_failure() {
        let mut connector = connector_until_security_upgrade(Some(credentials()));
        let mut output = WriteBuf::new();
        connector.step(&SERVER_CAPABILITIES, None, &mut output).unwrap();

        output.clear();
        let error = connector
            .step(&AUTH_RESPONSE_LOGON_FAILURE, None, &mut output)
            .unwrap_err();

        match error.kind() {
            ConnectorErrorKind::RdstlsAuthFailed(code) => assert_eq!(*code, RdstlsResultCode::LOGON_FAILURE),
            other => panic!("unexpected error kind: {other}"),
        }
    }

    #[test]
    fn connector_requests_rdstls_when_given_credentials() {
        let mut connector = ClientConnector::new(super::super::test_config(), "127.0.0.1:3389".parse().unwrap())
            .with_rdstls_credentials(credentials());

        assert_eq!(
            super::requested_protocol(&mut connector),
            SecurityProtocol::SSL | SecurityProtocol::RDSTLS
        );
    }

    #[test]
    fn rdstls_selected_without_credentials_is_an_error() {
        let mut connector = connector_until_security_upgrade(None);
        let mut output = WriteBuf::new();

        assert!(connector.step(&SERVER_CAPABILITIES, None, &mut output).is_err());
    }
}
