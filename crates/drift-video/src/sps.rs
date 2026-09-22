//! H.264 sequence parameter set parsing and colour signalling rewrite (pure).
//!
//! g-r-d's AVC420 SPS carries no `video_signal_type` in its VUI (plan §1.4), so decoders assume
//! **video range** — yet g-r-d's encoder shader writes **BT.709 full-range** samples. VideoToolbox
//! then expands every sample when producing the `420f` output buffer (measured: PSNR 27.5 dB vs
//! the raw samples). [`with_bt709_full_range`] rewrites the SPS so its VUI says what the samples
//! are (full range, BT.709 primaries/transfer/matrix); VideoToolbox then passes samples through.
//!
//! Only the VUI's `video_signal_type` part changes; every other syntax element is copied
//! bit-for-bit. Slices refer to the SPS by id, so the rest of the stream is unaffected.

/// Errors from SPS parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpsError {
    /// The NAL unit is not an SPS.
    #[error("not an SPS NAL unit")]
    NotSps,
    /// The bitstream ends early or holds an out-of-range Exp-Golomb code.
    #[error("malformed SPS")]
    Malformed,
}

/// Fields of an SPS that Drift inspects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpsInfo {
    /// `profile_idc` (100 = High).
    pub profile_idc: u8,
    /// `level_idc` (40 = level 4.0).
    pub level_idc: u8,
    /// Decoded picture width after cropping.
    pub width: u32,
    /// Decoded picture height after cropping.
    pub height: u32,
    /// `video_full_range_flag`, if the VUI signals it.
    pub full_range: Option<bool>,
    /// `matrix_coefficients`, if the VUI carries a colour description.
    pub matrix_coefficients: Option<u8>,
}

/// Parses the fields in [`SpsInfo`] from an SPS NAL unit (header byte included).
pub fn parse(sps: &[u8]) -> Result<SpsInfo, SpsError> {
    let rbsp = rbsp_of(sps)?;
    let header = parse_header(&rbsp)?;
    let mut info = SpsInfo {
        profile_idc: header.profile_idc,
        level_idc: header.level_idc,
        width: header.width,
        height: header.height,
        full_range: None,
        matrix_coefficients: None,
    };
    if header.vui_present {
        let mut r = BitReader::at(&rbsp, header.vui_flag_pos + 1);
        skip_aspect_and_overscan(&mut r)?;
        if let Some(signal) = read_signal_type(&mut r)? {
            info.full_range = Some(signal.full_range);
            info.matrix_coefficients = signal.matrix;
        }
    }
    Ok(info)
}

/// Returns the SPS with its VUI signalling BT.709 full range (`video_format` unspecified,
/// `video_full_range_flag = 1`, colour primaries / transfer / matrix = 1).
pub fn with_bt709_full_range(sps: &[u8]) -> Result<Vec<u8>, SpsError> {
    let rbsp = rbsp_of(sps)?;
    let header = parse_header(&rbsp)?;
    let mut w = BitWriter::default();
    let mut r = BitReader::at(&rbsp, 0);
    w.copy(&mut r, header.vui_flag_pos)?;
    w.bit(true); // vui_parameters_present_flag
    if header.vui_present {
        let mut r = BitReader::at(&rbsp, header.vui_flag_pos + 1);
        // aspect_ratio_info
        let aspect = r.bit()?;
        w.bit(aspect);
        if aspect {
            let idc = r.bits(8)?;
            w.bits(idc, 8);
            if idc == EXTENDED_SAR {
                w.copy(&mut r, 32)?;
            }
        }
        // overscan_info
        let overscan = r.bit()?;
        w.bit(overscan);
        if overscan {
            w.copy(&mut r, 1)?;
        }
        let _old = read_signal_type(&mut r)?;
        write_bt709_full_range(&mut w);
        // Everything after video_signal_type up to the rbsp_stop_one_bit is copied verbatim.
        let stop = stop_bit_position(&rbsp).ok_or(SpsError::Malformed)?;
        let rest = stop.checked_sub(r.pos).ok_or(SpsError::Malformed)?;
        w.copy(&mut r, rest)?;
    } else {
        w.bit(false); // aspect_ratio_info_present_flag
        w.bit(false); // overscan_info_present_flag
        write_bt709_full_range(&mut w);
        w.bit(false); // chroma_loc_info_present_flag
        w.bit(false); // timing_info_present_flag
        w.bit(false); // nal_hrd_parameters_present_flag
        w.bit(false); // vcl_hrd_parameters_present_flag
        w.bit(false); // pic_struct_present_flag
        w.bit(false); // bitstream_restriction_flag
    }
    w.trailing_bits();
    let mut out = vec![sps[0]];
    out.extend(escape(&w.bytes));
    Ok(out)
}

const EXTENDED_SAR: u32 = 255;

/// Profiles whose SPS carries chroma format, bit depths and scaling matrices (7.3.2.1.1).
const HIGH_PROFILES: [u8; 12] = [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134];

struct Header {
    profile_idc: u8,
    level_idc: u8,
    width: u32,
    height: u32,
    vui_flag_pos: usize,
    vui_present: bool,
}

struct SignalType {
    full_range: bool,
    matrix: Option<u8>,
}

fn rbsp_of(sps: &[u8]) -> Result<Vec<u8>, SpsError> {
    match sps.first() {
        Some(h) if h & 0x1F == crate::annexb::nal_type::SPS => Ok(unescape(&sps[1..])),
        _ => Err(SpsError::NotSps),
    }
}

fn parse_header(rbsp: &[u8]) -> Result<Header, SpsError> {
    let mut r = BitReader::at(rbsp, 0);
    let profile_idc = to_u8(r.bits(8)?)?;
    r.bits(8)?; // constraint_set flags + reserved
    let level_idc = to_u8(r.bits(8)?)?;
    r.ue_max(31)?; // seq_parameter_set_id
    let mut chroma_format_idc = 1;
    if HIGH_PROFILES.contains(&profile_idc) {
        chroma_format_idc = r.ue_max(3)?;
        if chroma_format_idc == 3 {
            r.bit()?; // separate_colour_plane_flag
        }
        r.ue_max(6)?; // bit_depth_luma_minus8
        r.ue_max(6)?; // bit_depth_chroma_minus8
        r.bit()?; // qpprime_y_zero_transform_bypass_flag
        if r.bit()? {
            let lists = if chroma_format_idc == 3 { 12 } else { 8 };
            for i in 0..lists {
                if r.bit()? {
                    skip_scaling_list(&mut r, if i < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    r.ue_max(12)?; // log2_max_frame_num_minus4
    match r.ue_max(2)? {
        0 => {
            r.ue_max(12)?; // log2_max_pic_order_cnt_lsb_minus4
        }
        1 => {
            r.bit()?; // delta_pic_order_always_zero_flag
            r.se()?; // offset_for_non_ref_pic
            r.se()?; // offset_for_top_to_bottom_field
            for _ in 0..r.ue_max(255)? {
                r.se()?;
            }
        }
        _ => {}
    }
    r.ue()?; // max_num_ref_frames
    r.bit()?; // gaps_in_frame_num_value_allowed_flag
    let width_mbs = r.ue()?.checked_add(1).ok_or(SpsError::Malformed)?;
    let height_map_units = r.ue()?.checked_add(1).ok_or(SpsError::Malformed)?;
    let frame_mbs_only = r.bit()?;
    if !frame_mbs_only {
        r.bit()?; // mb_adaptive_frame_field_flag
    }
    r.bit()?; // direct_8x8_inference_flag
    let crop = if r.bit()? { [r.ue()?, r.ue()?, r.ue()?, r.ue()?] } else { [0; 4] };
    let vui_flag_pos = r.pos;
    let vui_present = r.bit()?;

    let field_factor = if frame_mbs_only { 1 } else { 2 };
    let (crop_x, crop_y) = match chroma_format_idc {
        0 => (1, field_factor),
        1 => (2, 2 * field_factor),
        2 => (2, field_factor),
        _ => (1, field_factor),
    };
    let full_w = width_mbs.checked_mul(16).ok_or(SpsError::Malformed)?;
    let full_h = height_map_units.checked_mul(16 * field_factor).ok_or(SpsError::Malformed)?;
    let cut = |a: u32, b: u32, unit: u32| a.checked_add(b).and_then(|s| s.checked_mul(unit));
    let width =
        cut(crop[0], crop[1], crop_x).and_then(|c| full_w.checked_sub(c)).ok_or(SpsError::Malformed)?;
    let height =
        cut(crop[2], crop[3], crop_y).and_then(|c| full_h.checked_sub(c)).ok_or(SpsError::Malformed)?;
    Ok(Header { profile_idc, level_idc, width, height, vui_flag_pos, vui_present })
}

fn skip_scaling_list(r: &mut BitReader<'_>, size: usize) -> Result<(), SpsError> {
    let (mut last, mut next) = (8i64, 8i64);
    for _ in 0..size {
        if next != 0 {
            let delta = i64::from(r.se()?);
            next = (last + delta).rem_euclid(256);
        }
        if next != 0 {
            last = next;
        }
    }
    Ok(())
}

fn skip_aspect_and_overscan(r: &mut BitReader<'_>) -> Result<(), SpsError> {
    if r.bit()? && r.bits(8)? == EXTENDED_SAR {
        r.bits(32)?;
    }
    if r.bit()? {
        r.bit()?;
    }
    Ok(())
}

fn read_signal_type(r: &mut BitReader<'_>) -> Result<Option<SignalType>, SpsError> {
    if !r.bit()? {
        return Ok(None);
    }
    r.bits(3)?; // video_format
    let full_range = r.bit()?;
    let mut matrix = None;
    if r.bit()? {
        r.bits(16)?; // colour_primaries, transfer_characteristics
        matrix = Some(to_u8(r.bits(8)?)?);
    }
    Ok(Some(SignalType { full_range, matrix }))
}

fn write_bt709_full_range(w: &mut BitWriter) {
    w.bit(true); // video_signal_type_present_flag
    w.bits(5, 3); // video_format: unspecified
    w.bit(true); // video_full_range_flag
    w.bit(true); // colour_description_present_flag
    w.bits(1, 8); // colour_primaries: BT.709
    w.bits(1, 8); // transfer_characteristics: BT.709
    w.bits(1, 8); // matrix_coefficients: BT.709
}

/// Bit index of the `rbsp_stop_one_bit` (the last set bit).
fn stop_bit_position(rbsp: &[u8]) -> Option<usize> {
    let (index, byte) = rbsp.iter().enumerate().rev().find(|(_, b)| **b != 0)?;
    Some(index * 8 + 7 - byte.trailing_zeros() as usize)
}

fn to_u8(v: u32) -> Result<u8, SpsError> {
    u8::try_from(v).map_err(|_| SpsError::Malformed)
}

/// Removes emulation-prevention bytes (`00 00 03` → `00 00`).
fn unescape(ebsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ebsp.len());
    let mut zeros = 0;
    for &b in ebsp {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

/// Inserts emulation-prevention bytes.
fn escape(rbsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rbsp.len() + 4);
    let mut zeros = 0;
    for &b in rbsp {
        if zeros >= 2 && b <= 3 {
            out.push(3);
            zeros = 0;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    fn bit(&mut self) -> Result<bool, SpsError> {
        let byte = self.data.get(self.pos / 8).ok_or(SpsError::Malformed)?;
        let bit = byte & (0x80 >> (self.pos % 8)) != 0;
        self.pos += 1;
        Ok(bit)
    }

    fn bits(&mut self, n: u32) -> Result<u32, SpsError> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | u32::from(self.bit()?);
        }
        Ok(v)
    }

    fn ue(&mut self) -> Result<u32, SpsError> {
        let mut zeros = 0;
        while !self.bit()? {
            zeros += 1;
            if zeros > 31 {
                return Err(SpsError::Malformed);
            }
        }
        let suffix = u64::from(self.bits(zeros)?);
        u32::try_from((1u64 << zeros) - 1 + suffix).map_err(|_| SpsError::Malformed)
    }

    fn ue_max(&mut self, max: u32) -> Result<u32, SpsError> {
        let v = self.ue()?;
        if v > max { Err(SpsError::Malformed) } else { Ok(v) }
    }

    fn se(&mut self) -> Result<i32, SpsError> {
        let k = i64::from(self.ue()?);
        let v = if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) };
        i32::try_from(v).map_err(|_| SpsError::Malformed)
    }
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    len: usize,
}

impl BitWriter {
    fn bit(&mut self, b: bool) {
        if self.len.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if b && let Some(last) = self.bytes.last_mut() {
            *last |= 0x80 >> (self.len % 8);
        }
        self.len += 1;
    }

    fn bits(&mut self, v: u32, n: u32) {
        for i in (0..n).rev() {
            self.bit((v >> i) & 1 == 1);
        }
    }

    /// Copies `n` bits from the reader.
    fn copy(&mut self, r: &mut BitReader<'_>, n: usize) -> Result<(), SpsError> {
        for _ in 0..n {
            let b = r.bit()?;
            self.bit(b);
        }
        Ok(())
    }

    /// `rbsp_trailing_bits()`: a one bit, then zero bits to the byte boundary.
    fn trailing_bits(&mut self) {
        self.bit(true);
        while !self.len.is_multiple_of(8) {
            self.bit(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// g-r-d 50.2's SPS (leg3 and the headless capture): High 4.0, 1280×800, VUI without
    /// video_signal_type.
    const GRD_SPS: &[u8] = &[
        0x67, 0x64, 0x0c, 0x28, 0xac, 0x2b, 0x40, 0x28, 0x03, 0x2d, 0xff, 0x80, 0x00, 0x80, 0x00, 0x88, 0x00,
        0x00, 0x1f, 0x40, 0x00, 0x0e, 0xa6, 0x00, 0x78, 0x44, 0x23, 0x50,
    ];

    #[test]
    fn parses_grd_sps() {
        let info = parse(GRD_SPS).unwrap();
        assert_eq!(info.profile_idc, 100);
        assert_eq!(info.level_idc, 40);
        assert_eq!((info.width, info.height), (1280, 800));
        assert_eq!(info.full_range, None);
    }

    #[test]
    fn rewrites_grd_sps_to_full_range_bt709() {
        let out = with_bt709_full_range(GRD_SPS).unwrap();
        let info = parse(&out).unwrap();
        assert_eq!(info.full_range, Some(true));
        assert_eq!(info.matrix_coefficients, Some(1));
        assert_eq!((info.profile_idc, info.level_idc, info.width, info.height), (100, 40, 1280, 800));
        // Idempotent.
        assert_eq!(with_bt709_full_range(&out).unwrap(), out);
    }

    #[test]
    fn real_fixture_sps_all_rewrite() {
        for name in ["leg3", "headless_anim", "sps_change_640x400"] {
            let data =
                std::fs::read(drift_testkit::fixtures::fixtures_dir().join(format!("h264/{name}.h264")))
                    .unwrap();
            let sps = crate::annexb::split_nals(&data)
                .into_iter()
                .find(|n| crate::annexb::nal_unit_type(n) == Some(7))
                .unwrap();
            let before = parse(sps).unwrap();
            let after = parse(&with_bt709_full_range(sps).unwrap()).unwrap();
            assert_eq!((after.width, after.height), (before.width, before.height), "{name}");
            assert_eq!(after.full_range, Some(true), "{name}");
        }
    }

    #[test]
    fn rejects_non_sps_and_truncation() {
        assert_eq!(parse(&[0x68, 0xEE]), Err(SpsError::NotSps));
        assert_eq!(with_bt709_full_range(&[]), Err(SpsError::NotSps));
        assert_eq!(parse(&GRD_SPS[..5]), Err(SpsError::Malformed));
        assert_eq!(with_bt709_full_range(&GRD_SPS[..5]), Err(SpsError::Malformed));
    }

    /// Baseline SPS without VUI (x264 --profile baseline, 320x240): a VUI is added.
    #[test]
    fn adds_vui_when_absent() {
        // profile 66, level 13, sps_id 0, log2_max_frame_num 4 (ue 0), poc type 2 (ue 2),
        // max_num_ref_frames 1, gaps 0, width_mbs 20 (ue 19), height 15 (ue 14), frame_mbs_only 1,
        // direct_8x8 1, cropping 0, vui 0.
        let mut w = BitWriterForTest::default();
        w.bits(66, 8);
        w.bits(0xC0, 8);
        w.bits(13, 8);
        for v in [0, 0, 2, 1] {
            w.ue(v);
        }
        w.bits(0, 1);
        w.ue(19);
        w.ue(14);
        w.bits(0b110, 3);
        w.bits(0, 1); // vui_parameters_present_flag
        w.trailing();
        let mut sps = vec![0x67];
        sps.extend(escape_for_test(&w.bytes));
        let info = parse(&sps).unwrap();
        assert_eq!((info.width, info.height, info.full_range), (320, 240, None));
        let out = with_bt709_full_range(&sps).unwrap();
        let info = parse(&out).unwrap();
        assert_eq!((info.width, info.height, info.full_range), (320, 240, Some(true)));
    }

    #[derive(Default)]
    struct BitWriterForTest {
        bytes: Vec<u8>,
        n: usize,
    }

    impl BitWriterForTest {
        fn bit(&mut self, b: u32) {
            if self.n.is_multiple_of(8) {
                self.bytes.push(0);
            }
            if b != 0 {
                *self.bytes.last_mut().unwrap() |= 0x80 >> (self.n % 8);
            }
            self.n += 1;
        }
        fn bits(&mut self, v: u32, n: u32) {
            for i in (0..n).rev() {
                self.bit((v >> i) & 1);
            }
        }
        fn ue(&mut self, v: u32) {
            let x = v + 1;
            let len = 32 - x.leading_zeros();
            self.bits(0, len - 1);
            self.bits(x, len);
        }
        fn trailing(&mut self) {
            self.bit(1);
            while !self.n.is_multiple_of(8) {
                self.bit(0);
            }
        }
    }

    fn escape_for_test(rbsp: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut zeros = 0;
        for &b in rbsp {
            if zeros >= 2 && b <= 3 {
                out.push(3);
                zeros = 0;
            }
            zeros = if b == 0 { zeros + 1 } else { 0 };
            out.push(b);
        }
        out
    }

    proptest! {
        #[test]
        fn never_panics(data in proptest::collection::vec(any::<u8>(), 0..64)) {
            let mut sps = vec![0x67];
            sps.extend(data);
            let _ = parse(&sps);
            if let Ok(out) = with_bt709_full_range(&sps) {
                // Whatever parses must still parse after the rewrite, with the same size.
                let (a, b) = (parse(&sps), parse(&out));
                if let (Ok(a), Ok(b)) = (a, b) {
                    prop_assert_eq!((a.width, a.height, a.profile_idc), (b.width, b.height, b.profile_idc));
                    prop_assert_eq!(b.full_range, Some(true));
                }
            }
        }
    }
}
