//! M1-2 Red: surface and cache state, dispatch of every GfxPdu to the FrameSink, and
//! ResetGraphics teardown (plan §1.4: each DISP resize gives ResetGraphics + a new surface id).
#![allow(clippy::unwrap_used)]

mod support;

use std::sync::atomic::Ordering;

use drift_core::{Bgra, Point, Rect, Size};
use drift_testkit::{FrameSinkCall, PresentMode};
use ironrdp_egfx::pdu::{
    Avc420Region, CacheImportReplyPdu, CacheToSurfacePdu, CapabilitiesConfirmPdu, CapabilitiesV81Flags,
    CapabilitySet, Codec1Type, Color, DeleteSurfacePdu, EvictCacheEntryPdu, GfxPdu, Point as GfxPoint,
    SolidFillPdu, SurfaceToCachePdu, SurfaceToSurfacePdu, encode_avc420_bitmap_stream,
};
use support::{create, end, harness, map, rect16, reset, start, w2s1, wire};

fn solid(id: u16, rects: Vec<ironrdp_pdu::geometry::ExclusiveRectangle>) -> GfxPdu {
    GfxPdu::SolidFill(SolidFillPdu {
        surface_id: id,
        fill_pixel: Color { b: 1, g: 2, r: 3, xa: 0 },
        rectangles: rects,
    })
}

#[test]
fn reset_graphics_then_new_surface_tears_down_the_old_surface() {
    let mut h = harness(PresentMode::Immediate, Size::new(1280, 800));
    h.client.process_payload(&wire(&[reset(1280, 800), create(1, 1280, 800), map(1, 0, 0)])).unwrap();
    // Resize: g-r-d sends ResetGraphics and a *new* surface id (plan §1.4).
    h.client.process_payload(&wire(&[reset(1600, 1000), create(2, 1600, 1000), map(2, 0, 0)])).unwrap();
    assert_eq!(
        h.log.take(),
        vec![
            FrameSinkCall::Reset { output: Size::new(1280, 800) },
            FrameSinkCall::CreateSurface { id: 1, size: Size::new(1280, 800) },
            FrameSinkCall::MapSurfaceToOutput { id: 1, origin: Point::new(0, 0) },
            FrameSinkCall::Reset { output: Size::new(1600, 1000) },
            FrameSinkCall::CreateSurface { id: 2, size: Size::new(1600, 1000) },
            FrameSinkCall::MapSurfaceToOutput { id: 2, origin: Point::new(0, 0) },
        ]
    );
    assert!(h.h264.resets.load(Ordering::SeqCst) >= 1, "ResetGraphics resets the H.264 decoder");

    // The old surface is gone: drawing to it is a protocol error and reaches no sink.
    let err = h.client.process_payload(&wire(&[solid(1, vec![rect16(0, 0, 4, 4)])])).unwrap_err();
    assert!(matches!(err, drift_gfx::GfxError::UnknownSurface(1)), "{err:?}");
    assert!(h.log.take().is_empty());

    // The new one works, and id 1 may be reused by a fresh CreateSurface.
    h.client.process_payload(&wire(&[solid(2, vec![rect16(0, 0, 4, 4)]), create(1, 8, 8)])).unwrap();
    assert_eq!(h.log.take().len(), 2);
}

#[test]
fn reset_graphics_drops_cache_entries() {
    let mut h = harness(PresentMode::Immediate, Size::new(64, 64));
    let s2c = GfxPdu::SurfaceToCache(SurfaceToCachePdu {
        surface_id: 1,
        cache_key: 9,
        cache_slot: 3,
        source_rectangle: rect16(0, 0, 8, 8),
    });
    h.client.process_payload(&wire(&[reset(64, 64), create(1, 64, 64), s2c])).unwrap();
    h.client.process_payload(&wire(&[reset(64, 64), create(1, 64, 64)])).unwrap();
    let c2s = GfxPdu::CacheToSurface(CacheToSurfacePdu {
        cache_slot: 3,
        surface_id: 1,
        destination_points: vec![GfxPoint { x: 0, y: 0 }],
    });
    let err = h.client.process_payload(&wire(&[c2s])).unwrap_err();
    assert_eq!(err, drift_gfx::GfxError::InvalidCacheSlot(3));
}

#[test]
fn every_drawing_pdu_reaches_the_sink() {
    let mut h = harness(PresentMode::Immediate, Size::new(64, 64));
    let pixels: Vec<u8> = (0..4 * 2 * 4).map(|i| i as u8).collect();
    let avc =
        encode_avc420_bitmap_stream(&[Avc420Region::new(0, 0, 16, 8, 22, 100)], &[0, 0, 0, 1, 0x09, 0x30]);
    let pdus = vec![
        GfxPdu::CapabilitiesConfirm(CapabilitiesConfirmPdu::from_typed(&CapabilitySet::V8_1 {
            flags: CapabilitiesV81Flags::AVC420_ENABLED,
        })),
        reset(64, 64),
        create(1, 64, 64),
        create(2, 32, 32),
        map(1, 0, 0),
        start(5),
        solid(1, vec![rect16(0, 0, 2, 2), rect16(4, 4, 6, 8)]),
        w2s1(1, Codec1Type::Uncompressed, rect16(8, 8, 12, 10), pixels.clone()),
        w2s1(1, Codec1Type::Avc420, rect16(0, 0, 64, 64), avc),
        GfxPdu::SurfaceToSurface(SurfaceToSurfacePdu {
            source_surface_id: 1,
            destination_surface_id: 2,
            source_rectangle: rect16(0, 0, 8, 8),
            destination_points: vec![GfxPoint { x: 1, y: 2 }, GfxPoint { x: 24, y: 24 }],
        }),
        GfxPdu::SurfaceToCache(SurfaceToCachePdu {
            surface_id: 1,
            cache_key: 0xABCD,
            cache_slot: 7,
            source_rectangle: rect16(0, 0, 16, 16),
        }),
        GfxPdu::CacheToSurface(CacheToSurfacePdu {
            cache_slot: 7,
            surface_id: 2,
            destination_points: vec![GfxPoint { x: 16, y: 16 }],
        }),
        GfxPdu::EvictCacheEntry(EvictCacheEntryPdu { cache_slot: 7 }),
        GfxPdu::CacheImportReply(CacheImportReplyPdu { cache_slots: vec![] }),
        end(5),
        GfxPdu::DeleteSurface(DeleteSurfacePdu { surface_id: 2 }),
    ];
    h.client.process_payload(&wire(&pdus)).unwrap();
    assert_eq!(
        h.client.confirmed_caps(),
        Some(&CapabilitySet::V8_1 { flags: CapabilitiesV81Flags::AVC420_ENABLED })
    );
    let calls = h.log.take();
    assert_eq!(
        calls,
        vec![
            FrameSinkCall::Reset { output: Size::new(64, 64) },
            FrameSinkCall::CreateSurface { id: 1, size: Size::new(64, 64) },
            FrameSinkCall::CreateSurface { id: 2, size: Size::new(32, 32) },
            FrameSinkCall::MapSurfaceToOutput { id: 1, origin: Point::new(0, 0) },
            FrameSinkCall::SolidFill {
                id: 1,
                color: Bgra::new(1, 2, 3, 0xFF),
                rects: vec![Rect::new(0, 0, 2, 2), Rect::new(4, 4, 2, 4)],
            },
            FrameSinkCall::BlitBgra {
                id: 1,
                rect: Rect::new(8, 8, 4, 2),
                stride: 16,
                len: 32,
                hash: drift_testkit::frame_sink::fnv1a64(
                    &pixels.chunks(4).flat_map(|p| [p[0], p[1], p[2], 0xFF]).collect::<Vec<_>>()
                ),
            },
            FrameSinkCall::BlitNv12 { id: 1, size: Size::new(64, 64), regions: vec![Rect::new(0, 0, 16, 8)] },
            FrameSinkCall::SurfaceToSurface {
                src: 1,
                dst: 2,
                rect: Rect::new(0, 0, 8, 8),
                dests: vec![Point::new(1, 2), Point::new(24, 24)],
            },
            FrameSinkCall::SurfaceToCache { id: 1, rect: Rect::new(0, 0, 16, 16), slot: 7 },
            FrameSinkCall::CacheToSurface { slot: 7, id: 2, dests: vec![Point::new(16, 16)] },
            FrameSinkCall::EvictCache { slot: 7 },
            FrameSinkCall::EndFrame { frame_id: 5 },
            FrameSinkCall::DeleteSurface { id: 2 },
        ]
    );
}

#[test]
fn avc420_regions_are_clipped_to_the_surface() {
    let mut h = harness(PresentMode::Immediate, Size::new(64, 64));
    // A 1280x800 surface is coded as 1280x800 but the H.264 picture is 16-aligned; regions
    // reaching past the surface are clipped, and fully outside regions are dropped.
    let avc = encode_avc420_bitmap_stream(
        &[Avc420Region::new(0, 0, 64, 70, 22, 100), Avc420Region::new(0, 64, 64, 80, 22, 100)],
        &[0, 0, 0, 1, 0x09, 0x30],
    );
    h.client
        .process_payload(&wire(&[
            reset(64, 60),
            create(1, 64, 60),
            w2s1(1, Codec1Type::Avc420, rect16(0, 0, 64, 60), avc),
        ]))
        .unwrap();
    assert_eq!(
        h.log.take().last(),
        Some(&FrameSinkCall::BlitNv12 {
            id: 1,
            size: Size::new(64, 64),
            regions: vec![Rect::new(0, 0, 64, 60)]
        })
    );
}

#[test]
fn out_of_bounds_copies_are_protocol_errors() {
    let mut h = harness(PresentMode::Immediate, Size::new(64, 64));
    h.client.process_payload(&wire(&[reset(64, 64), create(1, 16, 16), create(2, 16, 16)])).unwrap();
    h.log.take();
    let cases = vec![
        GfxPdu::SurfaceToSurface(SurfaceToSurfacePdu {
            source_surface_id: 1,
            destination_surface_id: 2,
            source_rectangle: rect16(0, 0, 8, 8),
            destination_points: vec![GfxPoint { x: 10, y: 0 }],
        }),
        GfxPdu::SurfaceToSurface(SurfaceToSurfacePdu {
            source_surface_id: 1,
            destination_surface_id: 2,
            source_rectangle: rect16(0, 0, 17, 8),
            destination_points: vec![GfxPoint { x: 0, y: 0 }],
        }),
        GfxPdu::SurfaceToCache(SurfaceToCachePdu {
            surface_id: 1,
            cache_key: 1,
            cache_slot: 1,
            source_rectangle: rect16(8, 8, 20, 9),
        }),
        GfxPdu::SurfaceToCache(SurfaceToCachePdu {
            surface_id: 1,
            cache_key: 1,
            cache_slot: 0,
            source_rectangle: rect16(0, 0, 1, 1),
        }),
        w2s1(1, Codec1Type::Uncompressed, rect16(12, 12, 20, 13), vec![0; 8 * 4]),
        solid(1, vec![rect16(4, 4, 2, 8)]),
    ];
    for pdu in cases {
        let dbg = format!("{pdu:?}");
        assert!(h.client.process_payload(&wire(&[pdu])).is_err(), "{dbg}");
    }
    assert!(h.log.take().is_empty(), "rejected commands never reach the sink");
}

#[test]
fn solid_fill_is_clipped_to_the_surface() {
    let mut h = harness(PresentMode::Immediate, Size::new(64, 64));
    h.client
        .process_payload(&wire(&[
            reset(64, 64),
            create(1, 16, 16),
            solid(1, vec![rect16(8, 8, 32, 12), rect16(20, 20, 30, 30)]),
        ]))
        .unwrap();
    assert_eq!(
        h.log.take().last(),
        Some(&FrameSinkCall::SolidFill {
            id: 1,
            color: Bgra::new(1, 2, 3, 0xFF),
            rects: vec![Rect::new(8, 8, 8, 4)]
        })
    );
}
