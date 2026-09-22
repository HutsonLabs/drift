//! Errors from the GFX client. Every variant is a protocol error: the session actor ends the
//! connection with `DisconnectReason::ProtocolError` (never a panic, plan §10).

use drift_codec::CodecError;
use drift_core::DisconnectReason;
use drift_core::video::DecodeError;

/// Malformed, unsupported or inconsistent server input on the graphics channel.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GfxError {
    /// The `RDP_SEGMENTED_DATA` / ZGFX payload could not be decompressed.
    #[error("ZGFX decompression failed: {0}")]
    Zgfx(String),
    /// A GFX PDU could not be decoded (includes unknown codec ids and PDU types).
    #[error("malformed GFX PDU: {0}")]
    Decode(String),
    /// A codec Drift does not advertise and cannot decode.
    #[error("unsupported GFX codec {0}")]
    UnsupportedCodec(String),
    /// A PDU that only a client sends arrived from the server.
    #[error("unexpected client-to-server GFX PDU {0} from the server")]
    UnexpectedPdu(&'static str),
    /// A command referenced a surface that does not exist.
    #[error("unknown GFX surface {0}")]
    UnknownSurface(u16),
    /// `CreateSurface` for an id that already exists, or with an invalid size.
    #[error("invalid CreateSurface for surface {id}: {detail}")]
    InvalidSurface {
        /// Surface id.
        id: u16,
        /// Reason.
        detail: String,
    },
    /// A rectangle or point falls outside its surface.
    #[error("GFX rectangle out of bounds on surface {surface}: {detail}")]
    OutOfBounds {
        /// Surface id.
        surface: u16,
        /// Which rectangle.
        detail: String,
    },
    /// A cache slot outside `1..=max` or not holding an entry.
    #[error("invalid GFX cache slot {0}")]
    InvalidCacheSlot(u16),
    /// A CPU codec rejected its payload.
    #[error(transparent)]
    Codec(#[from] CodecError),
    /// The H.264 decoder rejected an AVC420 access unit.
    #[error(transparent)]
    H264(#[from] DecodeError),
}

impl GfxError {
    /// The session-level reason this error ends the connection with.
    pub fn disconnect_reason(&self) -> DisconnectReason {
        DisconnectReason::ProtocolError(String::new())
    }
}

impl From<GfxError> for DisconnectReason {
    fn from(e: GfxError) -> Self {
        e.disconnect_reason()
    }
}
