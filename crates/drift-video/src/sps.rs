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
    let _ = sps;
    Err(SpsError::Malformed)
}

/// Returns the SPS with its VUI signalling BT.709 full range (`video_format` unspecified,
/// `video_full_range_flag = 1`, colour primaries / transfer / matrix = 1).
pub fn with_bt709_full_range(sps: &[u8]) -> Result<Vec<u8>, SpsError> {
    let _ = sps;
    Err(SpsError::Malformed)
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
                std::fs::read(drift_testkit::fixtures::fixtures_dir().join(format!("h264/{name}.h264"))).unwrap();
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
            if self.n % 8 == 0 {
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
            while self.n % 8 != 0 {
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
