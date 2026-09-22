//! [`GfxClient`]: the `Microsoft::Windows::RDS::Graphics` DVC processor.

use drift_codec::TilePool;
use drift_core::video::H264Decoder;
use ironrdp_dvc::{DvcClientProcessor, DvcMessage, DvcProcessor};
use ironrdp_egfx::pdu::{CapabilitySet, GfxPdu};
use ironrdp_pdu::PduResult;

use crate::ack::AckOutbox;
use crate::error::GfxError;
use crate::sink::FrameSink;

/// Name of the graphics pipeline dynamic virtual channel.
pub const CHANNEL_NAME: &str = "Microsoft::Windows::RDS::Graphics";

/// Drift's RDPGFX client.
pub struct GfxClient {
    sink: Box<dyn FrameSink>,
    h264: Box<dyn H264Decoder>,
    pool: TilePool,
    acks: AckOutbox,
}

impl std::fmt::Debug for GfxClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GfxClient").finish_non_exhaustive()
    }
}

impl GfxClient {
    /// Creates a client drawing into `sink`, decoding AVC420 with `h264` and CPU codecs on `pool`.
    pub fn new(sink: Box<dyn FrameSink>, h264: Box<dyn H264Decoder>, pool: TilePool) -> Self {
        Self { sink, h264, pool, acks: AckOutbox::default() }
    }

    /// Handle onto the frame acknowledgements this client produces.
    pub fn acks(&self) -> AckOutbox {
        self.acks.clone()
    }

    /// Processes one DVC payload as received (`RDP_SEGMENTED_DATA`, ZGFX).
    pub fn process_payload(&mut self, payload: &[u8]) -> Result<(), GfxError> {
        let _ = (payload, &mut self.sink, &mut self.h264, &self.pool);
        Ok(())
    }

    /// Processes already-decompressed bytes holding one or more GFX PDUs.
    pub fn process_pdus(&mut self, data: &[u8]) -> Result<(), GfxError> {
        let _ = data;
        Ok(())
    }

    /// Applies one decoded GFX PDU.
    pub fn handle_pdu(&mut self, pdu: GfxPdu) -> Result<(), GfxError> {
        let _ = pdu;
        Ok(())
    }

    /// Shows or hides the session.
    pub fn set_visible(&mut self, visible: bool) {
        let _ = visible;
    }

    /// The capability set the server confirmed, once `CapabilitiesConfirm` arrived.
    pub fn confirmed_caps(&self) -> Option<&CapabilitySet> {
        None
    }

    /// Frames completed (`EndFrame` processed) since the channel opened.
    pub fn total_frames_decoded(&self) -> u32 {
        0
    }

    /// The error that failed the last [`DvcProcessor::process`] call, if any.
    pub fn take_error(&mut self) -> Option<GfxError> {
        None
    }
}

ironrdp_core::impl_as_any!(GfxClient);

impl DvcProcessor for GfxClient {
    fn channel_name(&self) -> &str {
        CHANNEL_NAME
    }

    fn start(&mut self, _channel_id: u32) -> PduResult<Vec<DvcMessage>> {
        Ok(Vec::new())
    }

    fn process(&mut self, _channel_id: u32, payload: &[u8]) -> PduResult<Vec<DvcMessage>> {
        let _ = payload;
        Ok(Vec::new())
    }
}

impl DvcClientProcessor for GfxClient {}
