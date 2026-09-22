# M1-2 — GFX client: validation rules, ack/suspend policy, actor seam, fuzzing

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-gfx/src/{client,ack,state,caps,error}.rs`, `crates/drift-gfx/fuzz/`

## Context

Plan §2 decision 2 makes the `Microsoft::Windows::RDS::Graphics` client Drift's own
(`ironrdp_egfx::pdu` + `ironrdp_graphics::zgfx`, no `GraphicsPipelineClient`, no
`ironrdp_egfx::decode`). Plan §1.4 fixes the caps and says acks go out right after present, with
`SUSPEND_FRAME_ACKNOWLEDGEMENT` while hidden. Several details were left open.

## Decisions

1. **Shape.** `GfxClient::new(Box<dyn FrameSink>, Box<dyn H264Decoder>, TilePool)` implements
   `DvcProcessor` + `DvcClientProcessor`. `start` sends exactly the 34-byte
   `CapabilitiesAdvertise [V8_1{AVC420_ENABLED}, V8{}]` (byte-tested against the verified vector
   and the captured client PDU). Codec dispatch: AVC420 → `H264Decoder` (drift-video's
   `VtDecoder`); RFX Progressive, Planar, ClearCodec, Uncompressed → drift-codec. RemoteFX, Alpha,
   AVC444 and AVC444v2 are `GfxError::UnsupportedCodec` (never advertised); unknown codec ids fail
   PDU decoding (`GfxError::Decode`). The AVC420 metablock is parsed with
   `ironrdp_egfx::pdu::Avc420BitmapStream` (drift-gfx must not depend on drift-video, ADR M0-5).
2. **PDU framing.** Each PDU is decoded from exactly the `pduLength` bytes its `RDPGFX_HEADER`
   declares, so a PDU can never read into the next one and trailing padding is skipped.
3. **Validation (server input never reaches the sink unchecked).**
   - Unknown surface id in any drawing/mapping command, duplicate or empty `CreateSurface`,
     cache slot outside `1..=25600` (no `SMALL_CACHE` advertised, same bound as FreeRDP) or empty
     on `CacheToSurface`: error.
   - Source rectangles, copy/cache destinations and WireToSurface1 destinations (uncompressed,
     planar, ClearCodec) must lie inside their surface: error (FreeRDP does the same).
   - `SolidFill` rectangles and AVC420 region rectangles are **clipped** to the surface (the
     H.264 picture is 16-aligned, so regions may legitimately reach past the surface); empty
     results are dropped. Inverted rectangles are decode errors.
   - `DeleteSurface` of an unknown id and `EvictCacheEntry` of an empty slot are ignored.
     `CacheImportReply` is ignored (Drift never offers a cache import). RAIL window mappings are
     ignored; `MapSurfaceToScaledOutput` is treated as a plain mapping.
   - Client-only PDUs arriving from the server are `GfxError::UnexpectedPdu`.
   - `SolidFill` alpha: 0xFF on XRGB surfaces, the PDU's `XA` on ARGB surfaces.
4. **`ResetGraphics`** drops every surface and cache slot, all progressive contexts and
   references, the ClearCodec caches and the H.264 decoder state, then calls `FrameSink::reset`.
   Drawing to a pre-reset surface id afterwards is `UnknownSurface`; the id can be created anew.
5. **Acks.** `EndFrame` hands the sink a `presented` callback bound to the frame id; the
   callback queues the `FrameAcknowledge` in the shared `AckOutbox` and runs the actor's
   notifier. `totalFramesDecoded` = `EndFrame`s processed; `queueDepth` = frames handed to the
   sink but not yet presented at ack time (0 → the wire value 0, `QUEUE_DEPTH_UNAVAILABLE`).
   Replaying the captured g-r-d sessions with a synchronous presenter reproduces the reference
   client's acks byte for byte (greeter AVC420, greeter progressive, 425-frame headless motion).
6. **Suspend.** g-r-d 50.2 (`handle_frame_ack_event`) ignores acks whose `frameId` it no longer
   tracks, so a suspend sent for an already-acked frame would be dropped. Therefore
   `set_visible(false)` does not emit anything by itself: the **next presented frame** is acked
   with `queueDepth = SUSPEND_FRAME_ACKNOWLEDGEMENT` and later frames are not acked. If nothing is
   in flight and Suppress Output stops the stream, nothing needs suspending. `set_visible(true)`
   (or showing before the suspend went out) restores normal acks; the next normal ack resumes
   the server's ack tracking (g-r-d keeps tracking frame ids while suspended).
7. **Actor seam (for drift-rdp, M1-1/M6-3).** The actor registers `GfxClient` in its
   `DrdynvcClient`, installs `acks().set_notifier(...)` (e.g. a `tokio::sync::Notify`), and on
   wake sends `ironrdp_dvc::encode_dvc_messages(channel_id, acks.drain_messages(), ChannelFlags::empty())`
   (channel id from `ActiveStage::get_dvc::<GfxClient>()`). `DvcProcessor::process` already returns
   acks that were due during the call (synchronous presenters). IronRDP only propagates a generic
   PDU error from `process`, so the actor calls `get_dvc_mut::<GfxClient>()…take_error()` to obtain
   the `GfxError` and ends with `GfxError::disconnect_reason()` (`ProtocolError`). Visibility goes
   through `GfxClient::set_visible`, which also forwards to `FrameSink::set_visible`.
8. **Fuzzing.** `crates/drift-gfx/fuzz` is a standalone cargo-fuzz workspace (nightly-only
   sanitizer builds stay out of the main workspace and `cargo xtask ci`). Target `gfx_pdu_zgfx`
   feeds arbitrary bytes either as a raw ZGFX payload or as decompressed PDUs to a client with a
   live surface. Run locally with
   `cargo +nightly fuzz run --fuzz-dir crates/drift-gfx/fuzz gfx_pdu_zgfx -- -max_total_time=300`;
   `.github/workflows/fuzz-nightly.yml` runs it for 5 minutes every night. CI additionally runs
   proptest no-panic properties over the same entry points on every PR.

## Consequences

- Snapshots (`crates/drift-gfx/tests/snapshots/`) pin the exact sink call sequence for the
  captured greeter (AVC420 and progressive), 2560×1600 scale-200 and headless-motion streams;
  review any diff there as a behaviour change.
- Stricter-than-IronRDP validation could reject a server bug that IronRDP tolerates; the
  captured g-r-d 50.2 streams all pass, and the error surfaces as a clear `ProtocolError`.
