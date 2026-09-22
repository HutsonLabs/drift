//! The session actor behind [`crate::spawn_session`] (M1-1 connect, M3-1 redirect loop).
//!
//! One Tokio task per tab. It connects leg after leg ([`crate::connect::connect_leg`]),
//! follows Server Redirections ([`crate::redirect::RedirectLoop`]), runs IronRDP's
//! `ActiveStage` while a leg is up and forwards input. Every state change goes through
//! `SessionState::transition`.
//!
//! Commands are received by a small forwarder task so that `Close` (or dropping every
//! [`SessionHandle`]) cancels a leg that is still connecting.

use std::sync::Arc;
use std::time::Duration;

use drift_core::{
    CertFingerprint, Clock, ConnectStage, ConnectionProfile, DesktopSize, DisconnectReason, SessionState,
};
use drift_gfx::FrameSink;
use ironrdp_async::FramedWrite as _;
use ironrdp_connector::ClientConnector;
use ironrdp_graphics::image_processing::PixelFormat;
use ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu;
use ironrdp_session::image::DecodedImage;
use ironrdp_session::{ActiveStage, ActiveStageBuilder, ActiveStageOutput, GracefulDisconnectReason};
use tokio::sync::{mpsc, watch};

use crate::connect::{
    self, CertDecision, CertExpectation, ConnectObserver, ConnectedLeg, LegAuth, LegRequest, TokioDialer,
    UpgradedFramed,
};
use crate::fastpath::InputEncoder;
use crate::gfx_ack::GfxAckOnly;
use crate::redirect::RedirectLoop;
use crate::session::{
    CertificateRole, SessionCommand, SessionEvent, SessionEvents, SessionHandle, SessionOptions,
    SessionSecrets,
};

/// How long `Close` waits for the server to acknowledge the Shutdown Request.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(1);

/// Spawns the actor; see [`crate::spawn_session`].
pub(crate) fn spawn(
    profile: ConnectionProfile,
    secrets: SessionSecrets,
    frame_sink: Box<dyn FrameSink>,
    clock: Arc<dyn Clock>,
    options: SessionOptions,
) -> (SessionHandle, SessionEvents) {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<SessionCommand>();
    let (fwd_tx, fwd_rx) = mpsc::unbounded_channel::<SessionCommand>();
    let (closed_tx, closed_rx) = watch::channel(false);
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            if cmd == SessionCommand::Close {
                let _ = closed_tx.send(true);
            }
            if fwd_tx.send(cmd).is_err() {
                return;
            }
        }
        // Every handle dropped: same as Close.
        let _ = closed_tx.send(true);
    });

    let actor = Actor {
        profile,
        secrets,
        _frame_sink: frame_sink,
        clock,
        options,
        io: Io { cmds: fwd_rx, events: ev_tx, state: SessionState::Idle, leg: 1, session_pin: None },
        closed: closed_rx,
    };
    tokio::spawn(actor.run());
    (SessionHandle::from_sender(cmd_tx), ev_rx)
}

/// How a connected leg ended.
enum LegEnd {
    /// The server redirected the client.
    Redirect(Box<ServerRedirectionPdu>),
    /// The user closed the session.
    Closed,
    /// The leg failed or the server ended it.
    Ended(DisconnectReason),
}

/// The actor's channels and state; borrowed by the connect observer.
struct Io {
    cmds: mpsc::UnboundedReceiver<SessionCommand>,
    events: mpsc::UnboundedSender<SessionEvent>,
    state: SessionState,
    leg: u8,
    /// A certificate the user accepted in this session (without persisting it yet).
    session_pin: Option<CertFingerprint>,
}

impl Io {
    fn emit(&self, event: SessionEvent) {
        let _ = self.events.send(event);
    }

    fn set_state(&mut self, next: SessionState) {
        if self.state == next {
            return;
        }
        match self.state.transition(next) {
            Ok(next) => {
                self.state = next.clone();
                self.emit(SessionEvent::State(next));
            }
            Err(e) => tracing::warn!(error = %e, "rejected session state transition"),
        }
    }
}

impl ConnectObserver for Io {
    fn stage(&mut self, stage: ConnectStage) {
        self.set_state(SessionState::Connecting { leg: self.leg, stage });
    }

    async fn decide_certificate(
        &mut self,
        host: &str,
        port: u16,
        fingerprint: CertFingerprint,
        role: CertificateRole,
    ) -> CertDecision {
        if self.session_pin == Some(fingerprint) {
            return CertDecision::Accept { pin: false };
        }
        self.emit(SessionEvent::CertificatePrompt { host: host.to_owned(), port, fingerprint, role });
        loop {
            match self.cmds.recv().await {
                Some(SessionCommand::AcceptCertificate { fingerprint: accepted, pin })
                    if accepted == fingerprint =>
                {
                    self.session_pin = Some(fingerprint);
                    if pin {
                        self.emit(SessionEvent::CertificatePinned(fingerprint));
                    }
                    return CertDecision::Accept { pin };
                }
                Some(SessionCommand::AcceptCertificate { .. }) => {
                    tracing::warn!("ignoring acceptance of a certificate that is not being prompted");
                }
                Some(SessionCommand::RejectCertificate) => return CertDecision::Reject,
                Some(SessionCommand::Close) | None => return CertDecision::Closed,
                Some(_) => {}
            }
        }
    }
}

struct Actor {
    profile: ConnectionProfile,
    secrets: SessionSecrets,
    /// Driven by the GFX channel (task M1-2); held for the session's lifetime.
    _frame_sink: Box<dyn FrameSink>,
    clock: Arc<dyn Clock>,
    options: SessionOptions,
    io: Io,
    closed: watch::Receiver<bool>,
}

impl Actor {
    async fn run(mut self) {
        let reason = self.connect_loop().await;
        let end = match reason {
            DisconnectReason::UserClosed => SessionState::Disconnected { reason },
            r if r.is_retryable() => SessionState::Disconnected { reason: r },
            r => SessionState::Failed { reason: r },
        };
        self.io.set_state(end);
        if *self.closed.borrow() {
            return;
        }
        // Stay addressable until the app closes the tab (reconnect commands arrive with M7).
        while let Some(cmd) = self.io.cmds.recv().await {
            if cmd == SessionCommand::Close {
                break;
            }
        }
    }

    fn tls_server_name(&self) -> String {
        self.options.tls_server_name.clone().unwrap_or_else(|| self.profile.host.clone())
    }

    fn first_leg(&self) -> LegRequest {
        LegRequest {
            leg: 1,
            mode: self.profile.mode,
            host: self.profile.host.clone(),
            port: self.profile.port,
            tls_server_name: self.tls_server_name(),
            auth: LegAuth::Nla {
                username: self.profile.rdp_username.clone(),
                password: self.secrets.rdp_password.clone(),
            },
            expectation: self
                .profile
                .cert_pin
                .map_or(CertExpectation::TrustOnFirstUse, CertExpectation::Pinned),
            role: CertificateRole::Server,
            routing_token: None,
            client_name: self.options.client_name.clone(),
            desktop: connect::DEFAULT_DESKTOP,
            scale: 100,
        }
    }

    /// Connects and runs legs until the session ends; returns why.
    async fn connect_loop(&mut self) -> DisconnectReason {
        let mut redirects =
            RedirectLoop::new(self.profile.mode, self.profile.host.clone(), self.profile.port);
        let mut request = self.first_leg();
        loop {
            self.io.leg = request.leg;
            let leg = match self.connect_one(request).await {
                Ok(leg) => leg,
                Err(reason) => return reason,
            };
            let size = leg.result.desktop_size;
            let desktop = DesktopSize { width: u32::from(size.width), height: u32::from(size.height) };
            if let Some(state) = redirects.on_activated(desktop, 100) {
                self.io.set_state(state);
            }
            match self.run_leg(leg).await {
                LegEnd::Closed => return DisconnectReason::UserClosed,
                LegEnd::Ended(reason) => return reason,
                LegEnd::Redirect(mut pdu) => {
                    let next = match redirects.on_redirect(&mut pdu) {
                        Ok(next) => next,
                        Err(reason) => return reason,
                    };
                    tracing::info!(leg = next.leg, "following Server Redirection");
                    let tls_server_name = if next.host == self.profile.host {
                        self.tls_server_name()
                    } else {
                        next.host.clone()
                    };
                    let expectation = match next.target_certificate {
                        Some(der) => CertExpectation::Exact(der),
                        None => self
                            .profile
                            .cert_pin
                            .or(self.io.session_pin)
                            .map_or(CertExpectation::TrustOnFirstUse, CertExpectation::Pinned),
                    };
                    request = LegRequest {
                        leg: next.leg,
                        mode: self.profile.mode,
                        host: next.host,
                        port: next.port,
                        tls_server_name,
                        auth: LegAuth::Rdstls(next.credentials),
                        expectation,
                        role: CertificateRole::RedirectTarget,
                        routing_token: next.routing_token,
                        client_name: self.options.client_name.clone(),
                        desktop: connect::DEFAULT_DESKTOP,
                        scale: 100,
                    };
                }
            }
        }
    }

    async fn connect_one(&mut self, request: LegRequest) -> Result<ConnectedLeg, DisconnectReason> {
        if *self.closed.borrow() {
            return Err(DisconnectReason::UserClosed);
        }
        let timeout = self.options.connect_timeout;
        let clock = Arc::clone(&self.clock);
        let closed = &mut self.closed;
        let io = &mut self.io;
        tokio::select! {
            res = connect::connect_leg(request, &TokioDialer, clock.as_ref(), timeout, io, attach_channels) => res,
            () = wait_closed(closed) => Err(DisconnectReason::UserClosed),
        }
    }

    async fn run_leg(&mut self, leg: ConnectedLeg) -> LegEnd {
        let ConnectedLeg { mut framed, result, .. } = leg;
        let mut image =
            DecodedImage::new(PixelFormat::RgbA32, result.desktop_size.width, result.desktop_size.height);
        let mut stage = ActiveStageBuilder {
            static_channels: result.static_channels,
            user_channel_id: result.user_channel_id,
            io_channel_id: result.io_channel_id,
            message_channel_id: result.message_channel_id,
            share_id: result.share_id,
            compression_type: result.compression_type,
            enable_server_pointer: result.enable_server_pointer,
            pointer_software_rendering: result.pointer_software_rendering,
        }
        .build();
        let mut input = InputEncoder::default();

        loop {
            tokio::select! {
                pdu = framed.read_pdu() => {
                    let (action, payload) = match pdu {
                        Ok(pdu) => pdu,
                        Err(e) => return LegEnd::Ended(connect::classify_io_error(&e, ConnectStage::Activation)),
                    };
                    let outputs = match stage.process(&mut image, action, &payload) {
                        Ok(outputs) => outputs,
                        Err(e) => return LegEnd::Ended(DisconnectReason::ProtocolError(e.to_string())),
                    };
                    if let Some(end) = handle_outputs(&mut framed, outputs).await {
                        return end;
                    }
                }
                cmd = self.io.cmds.recv() => match cmd {
                    None | Some(SessionCommand::Close) => {
                        shutdown(&mut framed, &mut stage, &mut image, self.clock.as_ref()).await;
                        return LegEnd::Closed;
                    }
                    Some(SessionCommand::Input(event)) => {
                        let events = input.encode(event);
                        match stage.process_fastpath_input(&mut image, &events) {
                            Ok(outputs) => {
                                if let Some(end) = handle_outputs(&mut framed, outputs).await {
                                    return end;
                                }
                            }
                            Err(e) => tracing::warn!(error = %e, "dropping unencodable input"),
                        }
                    }
                    // Resize, visibility, clipboard, reconnect triggers: later tasks (M4-2, M5-2, M6-3, M7).
                    Some(_) => {}
                },
            }
        }
    }
}

/// Static channels every leg carries: DRDYNVC with the graphics pipeline, without which g-r-d
/// terminates the session. [`GfxAckOnly`] is the interim listener until the actor wires
/// `drift_gfx::GfxClient` with the tab's `FrameSink` (M1-2); Display Control and clipboard are
/// attached here by M4-2 and M5-2.
fn attach_channels(connector: ClientConnector) -> ClientConnector {
    connector
        .with_static_channel(ironrdp_dvc::DrdynvcClient::new().with_dynamic_channel(GfxAckOnly::default()))
}

/// Writes response frames and turns terminal outputs into a [`LegEnd`].
async fn handle_outputs(framed: &mut UpgradedFramed, outputs: Vec<ActiveStageOutput>) -> Option<LegEnd> {
    for output in outputs {
        match output {
            ActiveStageOutput::ResponseFrame(frame) => {
                if let Err(e) = framed.write_all(&frame).await {
                    return Some(LegEnd::Ended(connect::classify_io_error(&e, ConnectStage::Activation)));
                }
            }
            ActiveStageOutput::ServerRedirect(pdu) => return Some(LegEnd::Redirect(pdu)),
            ActiveStageOutput::Terminate(reason) => {
                return Some(LegEnd::Ended(match reason {
                    GracefulDisconnectReason::UserInitiated => DisconnectReason::LoggedOffRemotely,
                    GracefulDisconnectReason::ServerInitiated | GracefulDisconnectReason::Other(_) => {
                        DisconnectReason::ServerShutdown
                    }
                }));
            }
            _ => {}
        }
    }
    None
}

/// Graceful shutdown: Shutdown Request, then wait briefly for the server's answer.
async fn shutdown(
    framed: &mut UpgradedFramed,
    stage: &mut ActiveStage,
    image: &mut DecodedImage,
    clock: &dyn Clock,
) {
    let Ok(outputs) = stage.graceful_shutdown() else { return };
    for output in outputs {
        if let ActiveStageOutput::ResponseFrame(frame) = output
            && framed.write_all(&frame).await.is_err()
        {
            return;
        }
    }
    let wait = async {
        while let Ok((action, payload)) = framed.read_pdu().await {
            match stage.process(image, action, &payload) {
                Ok(outputs) => {
                    if handle_outputs(framed, outputs).await.is_some() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    };
    let deadline = clock.now() + SHUTDOWN_GRACE;
    // The real-time cap keeps a frozen test clock from holding the actor forever.
    let _ = tokio::time::timeout(SHUTDOWN_GRACE * 2, connect::until(clock, deadline, wait)).await;
}

async fn wait_closed(closed: &mut watch::Receiver<bool>) {
    let _ = closed.wait_for(|c| *c).await;
}
