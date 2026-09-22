//! Parameter-set tracking (pure): decides when the VideoToolbox session must be rebuilt.
//!
//! VideoToolbox binds a decompression session to one `CMVideoFormatDescription`, which is built
//! from exactly one SPS/PPS pair. g-r-d resends the parameter sets with every IDR; when they
//! change (e.g. after a resize the new surface starts a new stream), the format description and
//! session are rebuilt. Identical resends keep the session.

use crate::annexb::AccessUnit;

/// One SPS/PPS pair, as raw NAL units (with header byte, emulation prevention intact).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterSets {
    /// Sequence parameter set NAL unit.
    pub sps: Vec<u8>,
    /// Picture parameter set NAL unit.
    pub pps: Vec<u8>,
}

/// What the decoder must do before decoding an access unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamDecision {
    /// Keep the current session.
    Keep,
    /// Build a new format description + session from these parameter sets.
    Rebuild(ParameterSets),
    /// No complete SPS/PPS pair seen yet; the access unit cannot be decoded.
    NotReady,
}

/// Tracks the active SPS/PPS pair.
#[derive(Debug, Clone, Default)]
pub struct ParameterSetTracker {
    active: Option<ParameterSets>,
}

impl ParameterSetTracker {
    /// A tracker with no active parameter sets.
    pub fn new() -> Self {
        Self::default()
    }

    /// Observes an access unit and decides whether the session can be kept.
    ///
    /// A decision of [`ParamDecision::Rebuild`] makes the new pair active.
    pub fn observe(&mut self, au: &AccessUnit) -> ParamDecision {
        let _ = au;
        ParamDecision::NotReady
    }

    /// The active parameter sets, if any.
    pub fn active(&self) -> Option<&ParameterSets> {
        self.active.as_ref()
    }

    /// Forgets the active parameter sets (e.g. after `ResetGraphics` or a failed rebuild).
    pub fn reset(&mut self) {
        self.active = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn au(sps: &[&[u8]], pps: &[&[u8]], picture: bool) -> AccessUnit {
        AccessUnit {
            sps: sps.iter().map(|s| s.to_vec()).collect(),
            pps: pps.iter().map(|s| s.to_vec()).collect(),
            avcc: if picture { vec![0, 0, 0, 1, 0x41] } else { Vec::new() },
            is_idr: !sps.is_empty(),
        }
    }

    const SPS_A: &[u8] = &[0x67, 0x64, 0x00, 0x28];
    const SPS_B: &[u8] = &[0x67, 0x64, 0x00, 0x1E];
    const PPS: &[u8] = &[0x68, 0xEE];

    #[test]
    fn slice_before_parameter_sets_is_not_ready() {
        let mut t = ParameterSetTracker::new();
        assert_eq!(t.observe(&au(&[], &[], true)), ParamDecision::NotReady);
        assert_eq!(t.observe(&au(&[SPS_A], &[], true)), ParamDecision::NotReady);
        assert!(t.active().is_none());
    }

    #[test]
    fn first_pair_builds_then_identical_resend_keeps() {
        let mut t = ParameterSetTracker::new();
        let pair = ParameterSets { sps: SPS_A.to_vec(), pps: PPS.to_vec() };
        assert_eq!(t.observe(&au(&[SPS_A], &[PPS], true)), ParamDecision::Rebuild(pair.clone()));
        assert_eq!(t.active(), Some(&pair));
        assert_eq!(t.observe(&au(&[], &[], true)), ParamDecision::Keep);
        assert_eq!(t.observe(&au(&[SPS_A], &[PPS], true)), ParamDecision::Keep);
    }

    #[test]
    fn sps_change_triggers_rebuild() {
        let mut t = ParameterSetTracker::new();
        t.observe(&au(&[SPS_A], &[PPS], true));
        let d = t.observe(&au(&[SPS_B], &[PPS], true));
        assert_eq!(d, ParamDecision::Rebuild(ParameterSets { sps: SPS_B.to_vec(), pps: PPS.to_vec() }));
    }

    #[test]
    fn pps_only_update_combines_with_active_sps() {
        let mut t = ParameterSetTracker::new();
        t.observe(&au(&[SPS_A], &[PPS], true));
        let pps2: &[u8] = &[0x68, 0xCE];
        let d = t.observe(&au(&[], &[pps2], true));
        assert_eq!(d, ParamDecision::Rebuild(ParameterSets { sps: SPS_A.to_vec(), pps: pps2.to_vec() }));
    }

    #[test]
    fn reset_forgets() {
        let mut t = ParameterSetTracker::new();
        t.observe(&au(&[SPS_A], &[PPS], true));
        t.reset();
        assert!(t.active().is_none());
        assert_eq!(t.observe(&au(&[], &[], true)), ParamDecision::NotReady);
    }
}
