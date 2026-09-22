//! Interim graphics-pipeline listener: advertises the verified caps and acknowledges frames
//! without decoding them.
//!
//! g-r-d **terminates the session** when the client does not open
//! `Microsoft::Windows::RDS::Graphics` ("[RDP.RDPGFX] Failed to open channel … Terminating
//! session", `ERRINFO_BAD_CAPABILITIES`, observed on the homelab host in M1-1). Until the
//! actor wires `drift_gfx::GfxClient` (task M1-2, which renders through the tab's `FrameSink`),
//! this listener keeps sessions alive: it sends `CapabilitiesAdvertise [V8_1{AVC420_ENABLED}, V8{}]`
//! (plan §1.4) and answers every `EndFrame` with a `FrameAcknowledge` so g-r-d never throttles.

use ironrdp_core::{ReadCursor, impl_as_any};
use ironrdp_dvc::{DvcClientProcessor, DvcMessage, DvcProcessor};
use ironrdp_egfx::pdu::{
    CapabilitiesAdvertisePdu, CapabilitiesV8Flags, CapabilitiesV81Flags, CapabilitySet, FrameAcknowledgePdu,
    GfxPdu, QueueDepth,
};
use ironrdp_graphics::zgfx;
use ironrdp_pdu::{PduResult, pdu_other_err};

/// The graphics pipeline dynamic channel name.
pub(crate) const CHANNEL_NAME: &str = "Microsoft::Windows::RDS::Graphics";

const GFX_HEADER_SIZE: usize = 8;

/// Caps-and-acks-only GFX client (see the module docs).
#[derive(Default)]
pub(crate) struct GfxAckOnly {
    zgfx: zgfx::Decompressor,
    frames_decoded: u32,
}

impl std::fmt::Debug for GfxAckOnly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GfxAckOnly").field("frames_decoded", &self.frames_decoded).finish_non_exhaustive()
    }
}

impl_as_any!(GfxAckOnly);

/// `CapabilitiesAdvertise [V8_1{AVC420_ENABLED}, V8{}]`.
pub(crate) fn caps_advertise() -> GfxPdu {
    GfxPdu::CapabilitiesAdvertise(CapabilitiesAdvertisePdu::from_typed(&[
        CapabilitySet::V8_1 { flags: CapabilitiesV81Flags::AVC420_ENABLED },
        CapabilitySet::V8 { flags: CapabilitiesV8Flags::empty() },
    ]))
}

impl GfxAckOnly {
    /// Decompresses one DVC payload and returns a `FrameAcknowledge` per `EndFrame`.
    fn acks_for(&mut self, payload: &[u8]) -> Result<Vec<GfxPdu>, String> {
        let mut data = Vec::new();
        self.zgfx.decompress(payload, &mut data).map_err(|e| format!("ZGFX: {e:?}"))?;
        let mut acks = Vec::new();
        let mut rest = data.as_slice();
        while !rest.is_empty() {
            let len = rest
                .get(4..GFX_HEADER_SIZE)
                .and_then(|b| <[u8; 4]>::try_from(b).ok())
                .map(u32::from_le_bytes)
                .and_then(|l| usize::try_from(l).ok())
                .filter(|l| (GFX_HEADER_SIZE..=rest.len()).contains(l))
                .ok_or_else(|| "malformed RDPGFX_HEADER".to_owned())?;
            let (pdu, tail) = rest.split_at(len);
            rest = tail;
            // Only EndFrame matters here; other PDUs (including codecs this listener does not
            // know) are skipped by their declared length.
            if let Ok(GfxPdu::EndFrame(end)) =
                ironrdp_core::decode_cursor::<GfxPdu>(&mut ReadCursor::new(pdu))
            {
                self.frames_decoded = self.frames_decoded.wrapping_add(1);
                acks.push(GfxPdu::FrameAcknowledge(FrameAcknowledgePdu {
                    queue_depth: QueueDepth::Unavailable,
                    frame_id: end.frame_id,
                    total_frames_decoded: self.frames_decoded,
                }));
            }
        }
        Ok(acks)
    }
}

impl DvcProcessor for GfxAckOnly {
    fn channel_name(&self) -> &str {
        CHANNEL_NAME
    }

    fn start(&mut self, _channel_id: u32) -> PduResult<Vec<DvcMessage>> {
        Ok(vec![Box::new(caps_advertise()) as DvcMessage])
    }

    fn process(&mut self, _channel_id: u32, payload: &[u8]) -> PduResult<Vec<DvcMessage>> {
        let acks = self.acks_for(payload).map_err(|e| {
            tracing::warn!(error = %e, "malformed graphics pipeline data");
            pdu_other_err!("graphics pipeline", "malformed GFX data")
        })?;
        Ok(acks.into_iter().map(|a| Box::new(a) as DvcMessage).collect())
    }
}

impl DvcClientProcessor for GfxAckOnly {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use drift_testkit::fixtures::{self, names};

    use super::*;

    fn frame_ids(pdus: &[GfxPdu]) -> Vec<u32> {
        pdus.iter()
            .filter_map(|p| match p {
                GfxPdu::FrameAcknowledge(a) => Some(a.frame_id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn caps_advertise_matches_the_captured_client_pdu() {
        let client = fixtures::records(names::GFX_LEG2_GREETER_AVC420_CLIENT);
        let ours = ironrdp_core::encode_vec(&caps_advertise()).unwrap();
        assert_eq!(ours.len(), 34);
        assert_eq!(client.first().map(Vec::as_slice), Some(ours.as_slice()));
    }

    #[test]
    fn acks_every_captured_frame_like_the_reference_client() {
        for (raw, client) in [
            (names::GFX_LEG2_GREETER_AVC420_RAW, names::GFX_LEG2_GREETER_AVC420_CLIENT),
            (names::GFX_HEADLESS_MOTION_AVC420_RAW, names::GFX_HEADLESS_MOTION_AVC420_CLIENT),
        ] {
            let mut gfx = GfxAckOnly::default();
            let mut ours = Vec::new();
            for payload in fixtures::records(raw) {
                ours.extend(gfx.acks_for(&payload).unwrap());
            }
            let reference: Vec<GfxPdu> = fixtures::records(client)
                .iter()
                .filter_map(|b| ironrdp_core::decode::<GfxPdu>(b).ok())
                .collect();
            assert!(!ours.is_empty(), "{raw}");
            assert_eq!(frame_ids(&ours), frame_ids(&reference), "{raw}");
        }
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        let mut gfx = GfxAckOnly::default();
        assert!(gfx.acks_for(&[0xE0, 0x04, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00]).is_err());
        assert!(gfx.process(1, &[]).is_err());
        assert_eq!(gfx.channel_name(), CHANNEL_NAME);
        assert_eq!(gfx.start(1).unwrap().len(), 1);
    }
}
