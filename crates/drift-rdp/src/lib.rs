//! # drift-rdp
//!
//! The per-tab RDP session. The **public interface** consumed by `drift-app` is fixed in
//! [`session`] (M0-5, `docs/adr/M0-5-session-interface.md`) so the actor (stream A) and
//! the app (stream D) can be built in parallel.
//!
//! | Module | Task |
//! |---|---|
//! | [`session`] | M0-5 interface; the actor (`actor.rs`) from M1-1 onwards |
//! | [`connect`] | M1-1: TCP, TLS with TOFU pin, NLA, capabilities |
//! | [`redirect`] | M3-1: Server Redirection loop (coverage-gated) |
//! | [`rdstls`] | M3-1: RDSTLS client glue (coverage-gated) |

mod actor;
pub mod connect;
mod fastpath;
mod gfx_ack;
pub mod rdstls;
pub mod redirect;
pub mod session;
mod tls;

pub use session::{
    CertificateRole, CursorBitmap, CursorUpdate, SessionCapabilities, SessionClosed, SessionCommand,
    SessionEvent, SessionEvents, SessionHandle, SessionOptions, SessionSecrets, SessionStats, spawn_session,
};
