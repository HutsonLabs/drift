//! Codec errors. Malformed server input maps to [`CodecError`]; it never panics.

use std::fmt;

use drift_core::Rect;

/// Which CPU codec produced an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodecKind {
    /// RFX Progressive (`RDPGFX_CODECID_CAPROGRESSIVE`, 0x0009).
    Progressive,
    /// RDP 6.0 Planar (`RDPGFX_CODECID_PLANAR`, 0x000A).
    Planar,
    /// Uncompressed 32 bpp (`RDPGFX_CODECID_UNCOMPRESSED`, 0x0000).
    Uncompressed,
    /// ClearCodec (`RDPGFX_CODECID_CLEARCODEC`, 0x0008).
    ClearCodec,
}

impl fmt::Display for CodecKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Progressive => "RFX progressive",
            Self::Planar => "planar",
            Self::Uncompressed => "uncompressed",
            Self::ClearCodec => "ClearCodec",
        })
    }
}

/// A decode failure. The GFX layer maps every variant to
/// `DisconnectReason::ProtocolError`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    /// The payload could not be decoded.
    #[error("malformed {codec} payload: {detail}")]
    Malformed {
        /// Codec that rejected the payload.
        codec: CodecKind,
        /// Human-readable reason (from the underlying decoder).
        detail: String,
    },
    /// The payload size does not match the destination rectangle.
    #[error("{codec} payload is {actual} bytes, expected {expected}")]
    SizeMismatch {
        /// Codec that rejected the payload.
        codec: CodecKind,
        /// Bytes required by the destination rectangle.
        expected: usize,
        /// Bytes received.
        actual: usize,
    },
    /// The destination rectangle is empty or exceeds the protocol's 16-bit coordinate space.
    #[error("{codec} destination rectangle {rect:?} is invalid")]
    InvalidRect {
        /// Codec that rejected the rectangle.
        codec: CodecKind,
        /// The offending rectangle.
        rect: Rect,
    },
    /// The surface is larger than the codec supports (MS-RDPEGFX caps surfaces at 32766 px).
    #[error("{codec} surface {width}x{height} is too large")]
    SurfaceTooLarge {
        /// Codec that rejected the surface.
        codec: CodecKind,
        /// Surface width in pixels.
        width: u32,
        /// Surface height in pixels.
        height: u32,
    },
}

impl CodecError {
    /// Shorthand for [`CodecError::Malformed`].
    pub(crate) fn malformed(codec: CodecKind, detail: impl fmt::Display) -> Self {
        Self::Malformed { codec, detail: detail.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_the_codec() {
        let e = CodecError::malformed(CodecKind::Planar, "boom");
        assert_eq!(e.to_string(), "malformed planar payload: boom");
        let e = CodecError::SizeMismatch { codec: CodecKind::Uncompressed, expected: 16, actual: 3 };
        assert_eq!(e.to_string(), "uncompressed payload is 3 bytes, expected 16");
        let e = CodecError::InvalidRect { codec: CodecKind::ClearCodec, rect: Rect::new(0, 0, 0, 1) };
        assert!(e.to_string().starts_with("ClearCodec destination rectangle"));
        let e = CodecError::SurfaceTooLarge { codec: CodecKind::Progressive, width: 70000, height: 1 };
        assert_eq!(e.to_string(), "RFX progressive surface 70000x1 is too large");
    }
}
