//! Frame-acknowledgement policy (plan §1.4, M1-2).
//!
//! g-r-d throttles the stream once `max(2, min(rtt·60+2, 60))` frames are unacknowledged, so
//! each `FrameAcknowledge` must leave as soon as its frame is on screen:
//!
//! - `GfxClient` hands every `EndFrame` to the [`FrameSink`](crate::FrameSink) with a
//!   `presented` callback built here. The callback runs on the render thread and queues the
//!   ack in the shared [`AckOutbox`], then pokes the notifier so the session actor wakes up and
//!   sends it. Nothing is acked before it is presented.
//! - `queueDepth` is honest: the number of frames already handed to the renderer but not yet
//!   presented when the ack is built (0 is encoded as `QUEUE_DEPTH_UNAVAILABLE`, which has the
//!   same wire value). `totalFramesDecoded` counts every `EndFrame` processed so far.
//! - While the session is hidden, the next presented frame is acknowledged with
//!   `SUSPEND_FRAME_ACKNOWLEDGEMENT` and later frames are not acknowledged; the actor also sends
//!   Suppress Output, so normally no frame arrives at all. The suspend has to ride on a frame
//!   the server still tracks: g-r-d 50.2 drops acks whose `frameId` it no longer tracks
//!   (`handle_frame_ack_event`), so a suspend for an already-acknowledged frame would be
//!   ignored. When shown again, the next presented frame gets a normal ack, which resumes
//!   acknowledgement on the server.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ironrdp_dvc::DvcMessage;
use ironrdp_egfx::pdu::{FrameAcknowledgePdu, GfxPdu, QueueDepth};

use crate::sink::PresentedCallback;

type Notifier = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct AckState {
    outbox: Vec<FrameAcknowledgePdu>,
    /// Frames handed to the sink whose `presented` callback has not run yet.
    in_flight: u32,
    /// `EndFrame`s processed since the channel opened (wrapping, like the wire field).
    total_decoded: u32,
    hidden: bool,
    /// While hidden: the suspend ack has been queued.
    suspend_sent: bool,
    notifier: Option<Notifier>,
}

/// Clonable, thread-safe handle onto the acknowledgements the GFX client has produced.
///
/// The session actor drains it (after being woken by the notifier) and sends the PDUs on the
/// graphics channel, e.g. with `ironrdp_dvc::encode_dvc_messages(channel_id, outbox.drain_messages(), …)`.
#[derive(Clone, Default)]
pub struct AckOutbox {
    inner: Arc<Mutex<AckState>>,
}

impl std::fmt::Debug for AckOutbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.lock();
        f.debug_struct("AckOutbox")
            .field("queued", &s.outbox.len())
            .field("in_flight", &s.in_flight)
            .field("total_decoded", &s.total_decoded)
            .field("hidden", &s.hidden)
            .finish_non_exhaustive()
    }
}

impl AckOutbox {
    fn lock(&self) -> MutexGuard<'_, AckState> {
        // The state stays consistent even if a holder panicked (every update is a few
        // integer assignments), so recover from poisoning instead of propagating it.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Installs a callback run (on the presenting thread, outside the internal lock)
    /// whenever an ack is queued. Replaces any previous notifier.
    pub fn set_notifier(&self, notifier: impl Fn() + Send + Sync + 'static) {
        self.lock().notifier = Some(Arc::new(notifier));
    }

    /// Removes and returns the queued acknowledgements, oldest first.
    pub fn drain(&self) -> Vec<FrameAcknowledgePdu> {
        std::mem::take(&mut self.lock().outbox)
    }

    /// [`Self::drain`], as DVC messages ready for `ironrdp_dvc::encode_dvc_messages`.
    pub fn drain_messages(&self) -> Vec<DvcMessage> {
        self.drain().into_iter().map(|ack| Box::new(GfxPdu::FrameAcknowledge(ack)) as DvcMessage).collect()
    }

    /// Frames handed to the sink whose `presented` callback has not run yet.
    pub fn in_flight(&self) -> u32 {
        self.lock().in_flight
    }

    /// Records a processed `EndFrame` and returns the callback the sink must run once the
    /// frame is presented.
    pub(crate) fn frame_ended(&self, frame_id: u32) -> PresentedCallback {
        {
            let mut s = self.lock();
            s.in_flight = s.in_flight.saturating_add(1);
            s.total_decoded = s.total_decoded.wrapping_add(1);
        }
        let this = self.clone();
        Box::new(move || this.presented(frame_id))
    }

    /// `EndFrame`s processed so far.
    pub(crate) fn total_decoded(&self) -> u32 {
        self.lock().total_decoded
    }

    /// Switches between the visible and hidden policies.
    pub(crate) fn set_hidden(&self, hidden: bool) {
        let mut s = self.lock();
        if s.hidden != hidden {
            s.suspend_sent = false;
        }
        s.hidden = hidden;
    }

    fn presented(&self, frame_id: u32) {
        let notifier = {
            let mut s = self.lock();
            s.in_flight = s.in_flight.saturating_sub(1);
            let queue_depth = if !s.hidden {
                match s.in_flight {
                    0 => QueueDepth::Unavailable,
                    n => QueueDepth::AvailableBytes(n),
                }
            } else if !s.suspend_sent {
                s.suspend_sent = true;
                QueueDepth::Suspend
            } else {
                return;
            };
            let total_frames_decoded = s.total_decoded;
            s.outbox.push(FrameAcknowledgePdu { queue_depth, frame_id, total_frames_decoded });
            s.notifier.clone()
        };
        if let Some(notify) = notifier {
            notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_shows_counters_and_poison_is_recovered() {
        let outbox = AckOutbox::default();
        let cb = outbox.frame_ended(1);
        let o2 = outbox.clone();
        let _ = std::thread::spawn(move || {
            let _guard = o2.inner.lock();
            panic!("poison the lock");
        })
        .join();
        cb();
        assert_eq!(outbox.drain().len(), 1);
        let dbg = format!("{outbox:?}");
        assert!(dbg.contains("total_decoded: 1"), "{dbg}");
    }
}
