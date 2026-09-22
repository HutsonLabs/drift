//! Errors from the Metal / CoreVideo layer.

/// Failure while setting up GPU resources.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// No Metal device is available.
    #[error("no Metal device available")]
    NoDevice,
    /// The Metal shader library failed to compile or a function is missing.
    #[error("Metal shader error: {0}")]
    Shader(String),
    /// A Metal allocation (texture, buffer, queue, pipeline) failed.
    #[error("Metal allocation failed: {0}")]
    Allocation(&'static str),
    /// A CoreVideo call returned an error `CVReturn`.
    #[error("CoreVideo error {code} in {call}")]
    CoreVideo {
        /// The failing call.
        call: &'static str,
        /// The `CVReturn` value.
        code: i32,
    },
    /// The input has the wrong shape (size, pixel format…).
    #[error("invalid input: {0}")]
    Invalid(&'static str),
}
