# M1-3 — AVC420 decode: full-range SPS rewrite, low latency, fixtures

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-video/src/{annexb,metablock,params,sps,decode,quality}.rs`

## Context

Plan §1.4 fixes the facts: g-r-d sends AVC420 as Annex-B (AUD first), High 4.0, no B-frames,
**no colour description in the VUI**, and its encoder shader writes **BT.709 full-range** samples.
M1-3 asks for `VTDecompressionSession` with `RealTime` + "EnableLowLatency" producing
IOSurface-backed `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` buffers, and a fixture test
"`leg3.h264` gives 398 frames, PSNR ≥ 40 dB vs an ffmpeg golden".

## Decisions

1. **SPS rewrite to signal full range.** Measured: with the untagged g-r-d SPS, VideoToolbox
   treats the stream as video range and *expands* every sample into the `420f` output buffer
   (Y 16→0, 235→255; PSNR 27.5 dB against the raw samples). Tagging the
   `CMVideoFormatDescription` extensions (`FullRangeVideo`, BT.709 matrix) has **no effect** —
   the decoder follows the SPS VUI. Requesting `420v` output passes samples through but would
   mislabel full-range data. Drift therefore rewrites the SPS before building the format
   description (`sps::with_bt709_full_range`): only the VUI `video_signal_type` is replaced
   (full range, primaries/transfer/matrix = 1), a minimal VUI is added if absent, every other
   bit is copied. Result: both captures decode **bit-exact** against ffmpeg (PSNR ∞). This also
   matches the renderer's hard-coded BT.709 full-range shader (plan §2 decision 3). Slices refer
   to the SPS by id, so nothing else changes. The rebuild decision still compares the original
   SPS/PPS bytes (`params::ParameterSetTracker`).
2. **"EnableLowLatency".** VideoToolbox has no decoder-side low-latency key (the SDK only has
   `kVTVideoEncoderSpecification_EnableLowLatencyRateControl`). Low latency on decode is realised
   as: `kVTDecompressionPropertyKey_RealTime = true`, hardware decoder requested
   (`EnableHardwareAcceleratedVideoDecoder`), and **synchronous** decode (no
   `EnableAsynchronousDecompression`, no temporal processing), so the picture exists when
   `H264Decoder::decode` returns and the frame ack can follow present immediately.
3. **Fixtures.** The spike's `/private/tmp/leg3.h264` holds only **42** access units (55 628
   bytes; the 398-frame capture quoted in §1.4 was not preserved). It is kept at
   `fixtures/h264/leg3.h264` (same path the M0-3 importer uses) and decodes 42/42. To keep a long
   full-screen-motion test, a new capture was taken on 2026-09-22 with the spike's `probe2`
   against the drifttest2 headless session (:3392 via SSH forward) while `anim.py` ran:
   `fixtures/h264/headless_anim.h264`, **407** access units, 1280×800, same SPS/PPS as leg3.
   `fixtures/h264/sps_change_640x400.h264` (12 frames, `ffmpeg -f lavfi testsrc2 … libx264
   -profile high -bf 0`) provides a different SPS for the rebuild test. Goldens are
   `ffmpeg -f rawvideo -pix_fmt nv12` frames (`fixtures/h264/goldens/*.nv12`, ffmpeg 9.0.2). No
   credentials are involved in any of these files.
4. **Metablock fixture bytes.** No raw `RFX_AVC420_BITMAP_STREAM` wire bytes were captured in the
   spike (the dump hook sat after IronRDP's metablock parser). The parser is tested on the
   MS-RDPEGFX AVC444 example's AVC420 sub-stream (`fixtures/pdus/avc420_bitmap_stream_msrdpegfx.bin`,
   also used by IronRDP's test suite), on a full-surface metablock wrapped around the first real
   g-r-d access unit, and differentially (proptest) against IronRDP's encoder. When M0-3/M1-2
   capture GFX PDU streams, add a replay test through `parse_avc420_bitmap_stream`.
5. **Output ownership.** `DecodedPicture` wraps the retained `CVPixelBuffer`
   (`Send + Sync` newtype, documented), implements `Nv12Source`, and exposes
   `pixel_buffer()` for `CVMetalTextureCache` import by `drift-render`.
6. **Dev profile.** `[profile.dev.package.drift-video] opt-level = 3`, like `drift-codec`: the
   PSNR-heavy tests went from 38 s to 1.4 s and the dev app's decode path is not debug-slow.

## Consequences

- If g-r-d ever starts signalling a colour description, the rewrite simply replaces it with the
  same BT.709 full-range values (idempotent).
- A stream that is genuinely video range would be shown with slightly lifted blacks; g-r-d does
  not produce such streams (§1.4), and the renderer assumes full range anyway.
