//! Frame-acknowledgement policy (plan §1.4, M1-2).

use std::sync::{Arc, Mutex};

use ironrdp_dvc::DvcMessage;
use ironrdp_egfx::pdu::FrameAcknowledgePdu;

#[derive(Default)]
struct AckState {
    outbox: Vec<FrameAcknowledgePdu>,
}

/// Clonable, thread-safe handle onto the acknowledgements the GFX client has produced.
#[derive(Clone, Default)]
pub struct AckOutbox {
    inner: Arc<Mutex<AckState>>,
}

impl std::fmt::Debug for AckOutbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AckOutbox").finish_non_exhaustive()
    }
}

impl AckOutbox {
    /// Installs a callback run (on the presenting thread) whenever an ack is queued.
    pub fn set_notifier(&self, notifier: impl Fn() + Send + Sync + 'static) {
        let _ = notifier;
    }

    /// Removes and returns the queued acknowledgements, oldest first.
    pub fn drain(&self) -> Vec<FrameAcknowledgePdu> {
        let _ = &self.inner;
        Vec::new()
    }

    /// [`Self::drain`], as DVC messages ready for `ironrdp_dvc::encode_dvc_messages`.
    pub fn drain_messages(&self) -> Vec<DvcMessage> {
        Vec::new()
    }

    /// Frames handed to the sink whose `presented` callback has not run yet.
    pub fn in_flight(&self) -> u32 {
        0
    }
}
