//! Reconnect backoff policy (`ReconnectPolicy`), owned by task **M7-1**.
//!
//! Stub: M7-1 adds exponential full-jitter backoff (base 500 ms, ×2, cap 30 s) here,
//! classifying failures with [`crate::DisconnectReason::is_retryable`].
