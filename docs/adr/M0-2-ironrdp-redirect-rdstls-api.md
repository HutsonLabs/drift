# M0-2 — Server Redirection, RDSTLS and CLIPRDR changes in the IronRDP fork

- Status: accepted
- Date: 2026-09-22
- Context: plan §1.3, §1.6, §1.10, task M0-2. Patches: `third_party/ironrdp-patches/0001…0005`.

## Decisions

1. **Server Redirection PDU lives in `ironrdp-pdu`** as `rdp::server_redirection::ServerRedirectionPdu`
   and `ShareControlPdu::ServerRedirect`. The Share Control header of `pduType` 0xA has no
   `shareId`; the header decoder no longer reads one for that type (it would swallow
   `pad2Octets` and the packet `flags`). All optional fields are kept verbatim
   (`Option<String>` for UTF-16 strings, `Option<Vec<u8>>` for blobs); the password blob is
   never interpreted and is redacted from `Debug`. The target certificate stays raw in the PDU;
   `decode_target_certificate()` / `TargetCertificateContainer::der_certificate()` give the DER
   leaf that M3-1 compares with the TLS certificate of the next leg. The UTF-16 base64 decoding
   is a small local codec so the core-tier crate gains no dependency.
2. **The session surfaces the PDU** as `ActiveStageOutput::ServerRedirect(Box<ServerRedirectionPdu>)`.
   The session does not close the transport itself; the Drift `SessionActor` (M3-1) owns the
   redirect loop.
3. **RDSTLS runs inside `ClientConnector`**, not as a separate sequence: after the TLS upgrade,
   a server-selected `PROTOCOL_RDSTLS` moves the connector to `RdstlsCapabilities` (fixed 8-byte
   hint) and `RdstlsAuthResponse` (fixed 10-byte hint). The existing `ironrdp-async`/`ironrdp-tokio`/
   `ironrdp-blocking` `connect_finalize` drivers therefore run the exchange without changes.
   Credentials are passed with `ClientConnector::with_rdstls_credentials(RdstlsCredentials)`,
   built from the redirection PDU with `RdstlsCredentials::from_server_redirection`, and are
   dropped as soon as the request is written. A non-zero result is
   `ConnectorErrorKind::RdstlsAuthFailed(RdstlsResultCode)` so Drift can map `0x52E` to
   `RdstlsFailed(0x52E)` without string matching.
4. **RDSTLS is requested** when NLA is off and either load-balance info (the spike patch
   condition, kept as-is) or RDSTLS credentials are set.
5. **The spike's `ironrdp-client` change is not carried.** `ironrdp-drift.patch` also flipped
   `support_dyn_vc_gfx_protocol` to `true` in the example client's config builder; Drift builds
   its own `connector::Config` and sets the flag there, and changing the example client's
   default is not upstreamable.
6. **`CB_TEMP_DIRECTORY`.** Investigation showed IronRDP already encodes the PDU correctly
   (`dataLen` = 520, 520-byte `wszTempDir`). The g-r-d warning "header told length is 520, but
   actually read 0" comes from FreeRDP 3.x `cliprdr_server_receive_temporary_directory`, which
   never advances the stream. The fork therefore (a) validates `dataLen == 520` and the
   terminator on decode, and (b) skips the optional PDU when the backend's
   `temporary_directory()` is empty. Drift's `CliprdrBackend` (M5-2) returns `""` (no file
   transfer in v1), which removes the warning.

## Consequences

- Adding `ShareControlPdu::ServerRedirect`, `IoChannelPdu::ServerRedirect`,
  `ProcessorOutput::ServerRedirect`, `ActiveStageOutput::ServerRedirect` and new
  `ClientConnectorState` variants is a semver-breaking change for exhaustive matches upstream;
  the bundled `ironrdp-client` and `ironrdp-web` were updated to end the session on redirect.
- Tests are in `ironrdp-testsuite-core` (`pdu::server_redirection`, `session::server_redirection`,
  `connector::rdstls`, `clipboard::temporary_directory`) and use synthetic, sanitized vectors
  with the §1.3 shapes. Real captured fixtures (M0-3) can be added as extra tests later.
