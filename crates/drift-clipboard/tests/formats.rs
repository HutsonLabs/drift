//! M5-1 Red: pure format mapping (plan §6 M5-1, §1.6).

use drift_clipboard::formats::{
    CF_DIB, CF_TEXT, CF_TIFF, CF_UNICODETEXT, ClipError, ClipFormat, FormatKind, LOCAL_PNG_FORMAT_ID,
    MAX_CLIPBOARD_BYTES, PNG_FORMAT_NAME, check_size, decode_ansi_text, decode_inbound, decode_unicode_text,
    dib_to_png, encode_outbound, encode_unicode_text, filter_local, outbound_formats, png_passthrough,
    png_to_dib, select_inbound,
};
use drift_clipboard::{ClipboardContents, ClipboardItem};
use drift_core::ClipboardPrefs;
use proptest::prelude::*;

const PNG_FIXTURE: &[u8] = include_bytes!("fixtures/remote_clip_d011.png");
const REMOTE_TEXT: &[u8] = include_bytes!("fixtures/remote_cf_unicodetext.bin");
const DIB_24_BU: &[u8] = include_bytes!("fixtures/dib_24_bottomup.bin");
const DIB_24_TD: &[u8] = include_bytes!("fixtures/dib_24_topdown.bin");
const DIB_32_BU: &[u8] = include_bytes!("fixtures/dib_32_bottomup.bin");
const DIB_32_TD_BF: &[u8] = include_bytes!("fixtures/dib_32_topdown_bitfields.bin");

/// Fixture pixels, top row first (see tests/fixtures/README.md).
const EXPECTED: [[[u8; 3]; 3]; 2] =
    [[[255, 0, 0], [0, 255, 0], [0, 0, 255]], [[255, 255, 255], [0, 0, 0], [10, 20, 30]]];

fn contents(items: Vec<ClipboardItem>) -> ClipboardContents {
    ClipboardContents { items }
}

fn png_pixels(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let img =
        image::load_from_memory_with_format(png, image::ImageFormat::Png).expect("valid png").to_rgba8();
    (img.width(), img.height(), img.into_raw())
}

// ---------------------------------------------------------------- text

#[test]
fn unicode_text_is_utf16le_crlf_with_nul() {
    let bytes = encode_unicode_text("a\nb");
    assert_eq!(bytes, vec![b'a', 0, b'\r', 0, b'\n', 0, b'b', 0, 0, 0]);
    // Existing CRLF is not doubled.
    assert_eq!(encode_unicode_text("a\r\nb"), bytes);
    assert_eq!(encode_unicode_text(""), vec![0, 0]);
}

#[test]
fn unicode_text_decodes_captured_remote_sample() {
    assert_eq!(decode_unicode_text(REMOTE_TEXT), "copy-me-9137");
}

#[test]
fn unicode_text_decode_stops_at_nul_and_converts_crlf() {
    let mut b = Vec::new();
    for u in "x\r\ny".encode_utf16().chain([0u16]).chain("junk".encode_utf16()) {
        b.extend_from_slice(&u.to_le_bytes());
    }
    assert_eq!(decode_unicode_text(&b), "x\ny");
    // Odd trailing byte ignored; unpaired surrogate replaced; lone CR kept.
    assert_eq!(decode_unicode_text(&[b'a', 0, b'\r', 0, 0x00, 0xD8, b'z']), "a\r\u{FFFD}");
    assert_eq!(decode_unicode_text(&[]), "");
}

#[test]
fn ansi_text_decodes_utf8_or_latin1() {
    assert_eq!(decode_ansi_text(b"caf\xc3\xa9\r\nx\0junk"), "café\nx");
    assert_eq!(decode_ansi_text(b"caf\xe9"), "café");
}

proptest! {
    #[test]
    fn text_round_trip(s in "(\\PC|\r\n|\n|\r|[\u{10000}-\u{10FFFF}])*") {
        let wire = encode_unicode_text(&s);
        prop_assert_eq!(wire.len() % 2, 0);
        prop_assert_eq!(&wire[wire.len() - 2..], &[0u8, 0][..]);
        // Local form is LF-only; CRLF on the wire.
        let expected = s.replace("\r\n", "\n");
        let decoded = decode_unicode_text(&wire);
        // Interior NULs terminate the wire string, as in Windows.
        let expected = expected.split('\0').next().unwrap_or("").to_owned();
        prop_assert_eq!(decoded, expected);
    }

    #[test]
    fn surrogate_pairs_survive(cps in proptest::collection::vec(0x10000u32..=0x10FFFF, 0..32)) {
        let s: String = cps.iter().filter_map(|&c| char::from_u32(c)).collect();
        prop_assert_eq!(decode_unicode_text(&encode_unicode_text(&s)), s);
    }

    #[test]
    fn decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = decode_unicode_text(&bytes);
        let _ = decode_ansi_text(&bytes);
        let _ = dib_to_png(&bytes);
        let _ = decode_inbound(FormatKind::Dib, &bytes);
        let _ = decode_inbound(FormatKind::Png, &bytes);
    }

    #[test]
    fn remote_text_is_lf_only(units in proptest::collection::vec(any::<u16>(), 0..64)) {
        let mut b = Vec::new();
        for u in units { b.extend_from_slice(&u.to_le_bytes()); }
        prop_assert!(!decode_unicode_text(&b).contains("\r\n"));
    }
}

// ---------------------------------------------------------------- images

#[test]
fn png_fixture_passes_through_unchanged() {
    assert_eq!(png_passthrough(PNG_FIXTURE).as_deref(), Ok(PNG_FIXTURE));
    assert_eq!(decode_inbound(FormatKind::Png, PNG_FIXTURE), Ok(ClipboardItem::Png(PNG_FIXTURE.to_vec())));
    let local = contents(vec![ClipboardItem::Png(PNG_FIXTURE.to_vec())]);
    assert_eq!(
        encode_outbound(LOCAL_PNG_FORMAT_ID, &local, ClipboardPrefs::TextAndImages).as_deref(),
        Ok(PNG_FIXTURE)
    );
    assert!(matches!(png_passthrough(b"GIF89a....."), Err(ClipError::InvalidImage(_))));
}

#[test]
fn dib_fixtures_decode_to_png() {
    for (name, dib) in [
        ("24 bottom-up", DIB_24_BU),
        ("24 top-down", DIB_24_TD),
        ("32 bottom-up", DIB_32_BU),
        ("32 top-down bitfields", DIB_32_TD_BF),
    ] {
        let png = dib_to_png(dib).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (w, h, px) = png_pixels(&png);
        assert_eq!((w, h), (3, 2), "{name}");
        for (y, row) in EXPECTED.iter().enumerate() {
            for (x, rgb) in row.iter().enumerate() {
                let i = (y * 3 + x) * 4;
                assert_eq!(&px[i..i + 3], rgb, "{name} pixel ({x},{y})");
                assert_eq!(px[i + 3], 255, "{name} pixel ({x},{y}) must be opaque");
            }
        }
        match decode_inbound(FormatKind::Dib, dib) {
            Ok(ClipboardItem::Png(p)) => assert_eq!(p, png, "{name}"),
            other => panic!("{name}: {other:?}"),
        }
    }
}

#[test]
fn png_to_dib_round_trips_through_dib_decoder() {
    let dib = png_to_dib(PNG_FIXTURE).expect("encode dib");
    // BITMAPINFOHEADER, 32 bpp, bottom-up (positive height).
    assert_eq!(u32::from_le_bytes(dib[0..4].try_into().unwrap()), 40);
    assert_eq!(i32::from_le_bytes(dib[4..8].try_into().unwrap()), 320);
    assert_eq!(i32::from_le_bytes(dib[8..12].try_into().unwrap()), 200);
    assert_eq!(u16::from_le_bytes(dib[14..16].try_into().unwrap()), 32);
    assert_eq!(dib.len(), 40 + 320 * 200 * 4);
    let back = dib_to_png(&dib).expect("decode dib");
    let (w, h, a) = png_pixels(&back);
    let (_, _, b) = png_pixels(PNG_FIXTURE);
    assert_eq!((w, h), (320, 200));
    // Colour survives (alpha is not carried by BI_RGB).
    for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
        assert_eq!(&pa[..3], &pb[..3]);
    }
    assert!(matches!(png_to_dib(b"nope"), Err(ClipError::InvalidImage(_))));
}

#[test]
fn tiff_passes_through() {
    let tiff = b"II*\0rest-of-tiff".to_vec();
    assert_eq!(decode_inbound(FormatKind::Tiff, &tiff), Ok(ClipboardItem::Tiff(tiff.clone())));
    let local = contents(vec![ClipboardItem::Tiff(tiff.clone())]);
    assert_eq!(encode_outbound(CF_TIFF, &local, ClipboardPrefs::TextAndImages), Ok(tiff));
}

#[test]
fn truncated_or_garbage_dib_is_rejected() {
    assert!(matches!(dib_to_png(&DIB_24_BU[..20]), Err(ClipError::InvalidImage(_))));
    assert!(matches!(dib_to_png(&DIB_24_BU[..50]), Err(ClipError::InvalidImage(_))));
    assert!(matches!(dib_to_png(&[]), Err(ClipError::InvalidImage(_))));
}

// ---------------------------------------------------------------- size cap

#[test]
fn size_cap_rejects_oversized_payloads() {
    assert_eq!(MAX_CLIPBOARD_BYTES, 32 * 1024 * 1024);
    assert_eq!(check_size(MAX_CLIPBOARD_BYTES), Ok(()));
    assert_eq!(
        check_size(MAX_CLIPBOARD_BYTES + 1),
        Err(ClipError::TooLarge { size: MAX_CLIPBOARD_BYTES + 1, max: MAX_CLIPBOARD_BYTES })
    );
    let big = vec![0u8; MAX_CLIPBOARD_BYTES + 1];
    for kind in
        [FormatKind::Png, FormatKind::Tiff, FormatKind::Dib, FormatKind::UnicodeText, FormatKind::AnsiText]
    {
        assert!(matches!(decode_inbound(kind, &big), Err(ClipError::TooLarge { .. })), "{kind:?}");
    }
    let local = contents(vec![ClipboardItem::Png(big.clone())]);
    assert!(matches!(
        encode_outbound(LOCAL_PNG_FORMAT_ID, &local, ClipboardPrefs::TextAndImages),
        Err(ClipError::TooLarge { .. })
    ));
}

#[test]
fn dib_with_huge_dimensions_is_rejected_before_decoding() {
    // 20000 x 20000 x 4 = 1.6 GB decoded: rejected from the header alone.
    let mut dib = DIB_24_BU.to_vec();
    dib[4..8].copy_from_slice(&20000i32.to_le_bytes());
    dib[8..12].copy_from_slice(&20000i32.to_le_bytes());
    assert!(matches!(dib_to_png(&dib), Err(ClipError::TooLarge { .. })));
}

#[test]
fn filter_local_emits_rejection_for_oversized_items() {
    let big = vec![0u8; MAX_CLIPBOARD_BYTES + 1];
    let c = contents(vec![ClipboardItem::Text("hi".into()), ClipboardItem::Png(big)]);
    let (kept, rejected) = filter_local(&c, ClipboardPrefs::TextAndImages);
    assert_eq!(kept, contents(vec![ClipboardItem::Text("hi".into())]));
    assert_eq!(
        rejected,
        vec![ClipError::TooLarge { size: MAX_CLIPBOARD_BYTES + 1, max: MAX_CLIPBOARD_BYTES }]
    );
}

// ---------------------------------------------------------------- advertisement / selection

#[test]
fn format_kinds() {
    assert_eq!(ClipFormat::standard(CF_UNICODETEXT).kind(), Some(FormatKind::UnicodeText));
    assert_eq!(ClipFormat::standard(CF_TEXT).kind(), Some(FormatKind::AnsiText));
    assert_eq!(ClipFormat::standard(CF_TIFF).kind(), Some(FormatKind::Tiff));
    assert_eq!(ClipFormat::standard(CF_DIB).kind(), Some(FormatKind::Dib));
    // Registered ids are per side: PNG is matched by name.
    assert_eq!(ClipFormat::named(0xD011, PNG_FORMAT_NAME).kind(), Some(FormatKind::Png));
    assert_eq!(ClipFormat::named(0xC0F0, "image/jpeg").kind(), None);
    assert_eq!(ClipFormat::standard(0xD011).kind(), None);
    assert!(FormatKind::Png.is_image() && FormatKind::Tiff.is_image() && FormatKind::Dib.is_image());
    assert!(!FormatKind::UnicodeText.is_image() && !FormatKind::AnsiText.is_image());
}

#[test]
fn outbound_advertisement() {
    let text = ClipboardItem::Text("t".into());
    let png = ClipboardItem::Png(PNG_FIXTURE.to_vec());
    let tiff = ClipboardItem::Tiff(b"II*\0".to_vec());
    let all = contents(vec![text.clone(), png.clone(), tiff.clone()]);
    assert_eq!(
        outbound_formats(&all, ClipboardPrefs::TextAndImages),
        vec![
            ClipFormat::standard(CF_UNICODETEXT),
            ClipFormat::named(LOCAL_PNG_FORMAT_ID, PNG_FORMAT_NAME),
            ClipFormat::standard(CF_DIB),
        ]
    );
    assert_eq!(outbound_formats(&all, ClipboardPrefs::Text), vec![ClipFormat::standard(CF_UNICODETEXT)]);
    assert_eq!(outbound_formats(&all, ClipboardPrefs::Off), vec![]);
    assert_eq!(
        outbound_formats(&contents(vec![tiff]), ClipboardPrefs::TextAndImages),
        vec![ClipFormat::standard(CF_TIFF)]
    );
    assert_eq!(outbound_formats(&ClipboardContents::empty(), ClipboardPrefs::TextAndImages), vec![]);
}

#[test]
fn outbound_data_per_format() {
    let local = contents(vec![ClipboardItem::Text("a\nb".into()), ClipboardItem::Png(PNG_FIXTURE.to_vec())]);
    let p = ClipboardPrefs::TextAndImages;
    assert_eq!(encode_outbound(CF_UNICODETEXT, &local, p), Ok(encode_unicode_text("a\nb")));
    let dib = encode_outbound(CF_DIB, &local, p).expect("dib");
    assert_eq!(dib, png_to_dib(PNG_FIXTURE).expect("dib"));
    assert_eq!(encode_outbound(CF_TEXT, &local, p), Err(ClipError::Unavailable(CF_TEXT)));
    assert_eq!(
        encode_outbound(LOCAL_PNG_FORMAT_ID, &local, ClipboardPrefs::Text),
        Err(ClipError::Unavailable(LOCAL_PNG_FORMAT_ID))
    );
    assert_eq!(
        encode_outbound(CF_UNICODETEXT, &local, ClipboardPrefs::Off),
        Err(ClipError::Unavailable(CF_UNICODETEXT))
    );
    assert_eq!(encode_outbound(CF_TIFF, &local, p), Err(ClipError::Unavailable(CF_TIFF)));
}

#[test]
fn inbound_selection_prefers_unicode_and_png() {
    let gedit = [ClipFormat::standard(CF_TEXT), ClipFormat::standard(CF_UNICODETEXT)];
    assert_eq!(
        select_inbound(&gedit, ClipboardPrefs::TextAndImages),
        vec![(ClipFormat::standard(CF_UNICODETEXT), FormatKind::UnicodeText)]
    );
    // CF_TEXT only when Unicode is absent.
    assert_eq!(
        select_inbound(&[ClipFormat::standard(CF_TEXT)], ClipboardPrefs::Text),
        vec![(ClipFormat::standard(CF_TEXT), FormatKind::AnsiText)]
    );
    let shot = [
        ClipFormat::standard(CF_DIB),
        ClipFormat::standard(CF_TIFF),
        ClipFormat::named(0xD011, PNG_FORMAT_NAME),
        ClipFormat::named(0xD012, "image/jpeg"),
    ];
    assert_eq!(
        select_inbound(&shot, ClipboardPrefs::TextAndImages),
        vec![(ClipFormat::named(0xD011, PNG_FORMAT_NAME), FormatKind::Png)]
    );
    assert_eq!(select_inbound(&shot, ClipboardPrefs::Text), vec![]);
    assert_eq!(
        select_inbound(&shot[..2], ClipboardPrefs::TextAndImages),
        vec![(ClipFormat::standard(CF_TIFF), FormatKind::Tiff)]
    );
    assert_eq!(
        select_inbound(&shot[..1], ClipboardPrefs::TextAndImages),
        vec![(ClipFormat::standard(CF_DIB), FormatKind::Dib)]
    );
    assert_eq!(select_inbound(&gedit, ClipboardPrefs::Off), vec![]);
    let both = [ClipFormat::named(0xD011, PNG_FORMAT_NAME), ClipFormat::standard(CF_UNICODETEXT)];
    assert_eq!(
        select_inbound(&both, ClipboardPrefs::TextAndImages),
        vec![
            (ClipFormat::standard(CF_UNICODETEXT), FormatKind::UnicodeText),
            (ClipFormat::named(0xD011, PNG_FORMAT_NAME), FormatKind::Png)
        ]
    );
}

#[test]
fn inbound_text_decoding() {
    assert_eq!(
        decode_inbound(FormatKind::UnicodeText, REMOTE_TEXT),
        Ok(ClipboardItem::Text("copy-me-9137".into()))
    );
    assert_eq!(decode_inbound(FormatKind::AnsiText, b"hi\r\n\0"), Ok(ClipboardItem::Text("hi\n".into())));
}
