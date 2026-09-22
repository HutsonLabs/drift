//! The capability sets Drift advertises (plan §1.4, §2 decision 8).

use ironrdp_egfx::pdu::{CapabilitiesAdvertisePdu, CapabilitySet, GfxPdu};

/// The capability sets sent in `RDPGFX_CAPS_ADVERTISE_PDU`.
pub fn advertised_caps() -> Vec<CapabilitySet> {
    Vec::new()
}

/// The `CapabilitiesAdvertise` PDU carrying [`advertised_caps`].
pub fn caps_advertise_pdu() -> GfxPdu {
    GfxPdu::CapabilitiesAdvertise(CapabilitiesAdvertisePdu::from_typed(&advertised_caps()))
}
