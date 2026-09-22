//! H.264 byte-stream handling (pure, no FFI).
//!
//! g-r-d sends AVC420 as an **Annex-B** byte stream (start codes, an AUD `09 30` first; plan
//! §1.4). VideoToolbox wants **AVCC** samples (4-byte big-endian NAL lengths) plus the SPS/PPS in
//! a `CMVideoFormatDescription`. This module splits the byte stream into NAL units, drops access
//! unit delimiters, pulls out the parameter sets, and length-prefixes the rest.
//!
//! The encoder (M8-2) goes the other way ([`avcc_to_annex_b`]).

/// NAL unit types used by Drift (ITU-T H.264 table 7-1).
pub mod nal_type {
    /// Coded slice of a non-IDR picture.
    pub const SLICE: u8 = 1;
    /// Coded slice of an IDR picture.
    pub const IDR: u8 = 5;
    /// Supplemental enhancement information.
    pub const SEI: u8 = 6;
    /// Sequence parameter set.
    pub const SPS: u8 = 7;
    /// Picture parameter set.
    pub const PPS: u8 = 8;
    /// Access unit delimiter.
    pub const AUD: u8 = 9;
}

/// Errors from byte-stream conversion.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnnexBError {
    /// The input contains no NAL unit at all.
    #[error("no NAL units in access unit")]
    Empty,
    /// An AVCC length prefix points past the end of the buffer.
    #[error("AVCC NAL length {len} at offset {offset} exceeds buffer")]
    Truncated {
        /// Offset of the length prefix.
        offset: usize,
        /// Declared NAL length.
        len: usize,
    },
    /// A NAL unit is too large for a 32-bit length prefix.
    #[error("NAL unit too large")]
    TooLarge,
}

/// Splits an Annex-B byte stream into NAL unit payloads (without start codes).
///
/// Accepts 3-byte (`00 00 01`) and 4-byte (`00 00 00 01`) start codes, ignores leading zero
/// bytes before the first start code and strips `trailing_zero_8bits` from each NAL unit.
/// Bytes before the first start code that are not zero are ignored. Empty NAL units are skipped.
pub fn split_nals(data: &[u8]) -> Vec<&[u8]> {
    let _ = data;
    Vec::new()
}

/// Returns the `nal_unit_type` of a NAL unit (low 5 bits of the header byte).
pub fn nal_unit_type(nal: &[u8]) -> Option<u8> {
    nal.first().map(|b| b & 0x1F)
}

/// One Annex-B access unit converted for VideoToolbox.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessUnit {
    /// Sequence parameter sets found in the access unit (usually zero or one).
    pub sps: Vec<Vec<u8>>,
    /// Picture parameter sets found in the access unit.
    pub pps: Vec<Vec<u8>>,
    /// Remaining NAL units (slices, SEI, …) as an AVCC sample with 4-byte lengths.
    /// Access unit delimiters and parameter sets are not included.
    pub avcc: Vec<u8>,
    /// Whether the access unit contains an IDR slice.
    pub is_idr: bool,
}

impl AccessUnit {
    /// Converts one Annex-B access unit: drops AUDs, extracts SPS/PPS and length-prefixes the
    /// remaining NAL units.
    pub fn from_annex_b(data: &[u8]) -> Result<Self, AnnexBError> {
        let _ = data;
        Err(AnnexBError::Empty)
    }

    /// Whether the access unit carries picture data (at least one NAL besides parameter sets).
    pub fn has_picture(&self) -> bool {
        !self.avcc.is_empty()
    }
}

/// Converts an AVCC sample (big-endian length prefixes of `length_size` bytes, 1..=4) into an
/// Annex-B byte stream with 4-byte start codes.
pub fn avcc_to_annex_b(avcc: &[u8], length_size: usize) -> Result<Vec<u8>, AnnexBError> {
    let _ = (avcc, length_size);
    Ok(Vec::new())
}

/// Iterates the NAL units of an AVCC sample.
pub fn avcc_nals(avcc: &[u8], length_size: usize) -> Result<Vec<&[u8]>, AnnexBError> {
    let _ = (avcc, length_size);
    Ok(Vec::new())
}

/// `profile_idc` of an SPS NAL unit (byte after the NAL header). 100 = High.
pub fn sps_profile_idc(sps: &[u8]) -> Option<u8> {
    let _ = sps;
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const AUD: &[u8] = &[0x09, 0x30];
    const SPS: &[u8] = &[0x67, 0x64, 0x00, 0x28, 0xAC, 0x2B];
    const PPS: &[u8] = &[0x68, 0xEE, 0x38, 0x30];
    const IDR: &[u8] = &[0x65, 0x88, 0x80, 0x41, 0x3F];
    const P: &[u8] = &[0x41, 0x9A, 0x02, 0x22];

    fn annex_b(nals: &[&[u8]], four: bool) -> Vec<u8> {
        let mut v = Vec::new();
        for n in nals {
            if four {
                v.push(0);
            }
            v.extend_from_slice(&[0, 0, 1]);
            v.extend_from_slice(n);
        }
        v
    }

    #[test]
    fn splits_four_byte_start_codes() {
        let s = annex_b(&[AUD, SPS, PPS, IDR], true);
        assert_eq!(split_nals(&s), vec![AUD, SPS, PPS, IDR]);
    }

    #[test]
    fn splits_three_byte_start_codes_and_strips_trailing_zeros() {
        let mut s = annex_b(&[AUD, P], false);
        s.extend_from_slice(&[0, 0, 0]);
        assert_eq!(split_nals(&s), vec![AUD, P]);
    }

    #[test]
    fn ignores_leading_garbage_and_empty_nals() {
        let mut s = vec![0, 0, 0, 0, 0];
        s.extend_from_slice(&annex_b(&[AUD], true));
        s.extend_from_slice(&[0, 0, 1, 0, 0, 1]);
        s.extend_from_slice(P);
        assert_eq!(split_nals(&s), vec![AUD, P]);
        assert!(split_nals(&[]).is_empty());
        assert!(split_nals(&[0, 0, 0, 0]).is_empty());
    }

    #[test]
    fn access_unit_drops_aud_and_extracts_parameter_sets() {
        let au = AccessUnit::from_annex_b(&annex_b(&[AUD, SPS, PPS, IDR], true)).unwrap();
        assert_eq!(au.sps, vec![SPS.to_vec()]);
        assert_eq!(au.pps, vec![PPS.to_vec()]);
        assert!(au.is_idr);
        let mut expect = (IDR.len() as u32).to_be_bytes().to_vec();
        expect.extend_from_slice(IDR);
        assert_eq!(au.avcc, expect);
        assert!(au.has_picture());
    }

    #[test]
    fn access_unit_without_nals_is_an_error() {
        assert_eq!(AccessUnit::from_annex_b(&[0, 0, 0]), Err(AnnexBError::Empty));
        let only_aud = AccessUnit::from_annex_b(&annex_b(&[AUD], true)).unwrap();
        assert!(!only_aud.has_picture());
    }

    #[test]
    fn avcc_round_trip_and_truncation() {
        let au = AccessUnit::from_annex_b(&annex_b(&[AUD, P, P], true)).unwrap();
        assert!(!au.is_idr);
        assert_eq!(avcc_nals(&au.avcc, 4).unwrap(), vec![P, P]);
        assert_eq!(avcc_to_annex_b(&au.avcc, 4).unwrap(), annex_b(&[P, P], true));
        assert_eq!(
            avcc_to_annex_b(&[0, 0, 0, 9, 1], 4),
            Err(AnnexBError::Truncated { offset: 0, len: 9 })
        );
        assert_eq!(avcc_nals(&[0, 2, 0x41, 0x9A], 2).unwrap(), vec![&[0x41, 0x9A][..]]);
        assert!(avcc_nals(&[0, 0, 0], 4).is_err());
    }

    #[test]
    fn profile_idc() {
        assert_eq!(sps_profile_idc(SPS), Some(100));
        assert_eq!(sps_profile_idc(&[0x67]), None);
        assert_eq!(sps_profile_idc(PPS), None);
    }

    #[test]
    fn real_leg3_stream_first_access_unit() {
        let data = std::fs::read(drift_testkit::fixtures::fixtures_dir().join("h264/leg3.h264")).unwrap();
        let nals = split_nals(&data);
        let types: Vec<u8> = nals.iter().filter_map(|n| nal_unit_type(n)).collect();
        assert_eq!(&types[..6], &[9, 7, 8, 5, 9, 1]);
        assert_eq!(types.iter().filter(|t| **t == nal_type::AUD).count(), 42);
        assert_eq!(sps_profile_idc(nals[1]), Some(100));
    }

    /// Strategy: a non-empty NAL unit whose body contains no start-code emulation
    /// (as guaranteed by emulation prevention) and does not end in a zero byte.
    fn nal_strategy() -> impl Strategy<Value = Vec<u8>> {
        (1u8..=23, proptest::collection::vec(any::<u8>(), 0..64), 1u8..=255).prop_map(|(t, mut body, last)| {
            // emulation prevention: never two zeros followed by 0..=3
            let mut out = vec![0x60 | t];
            let mut zeros = 0;
            for b in body.drain(..) {
                if zeros >= 2 && b <= 3 {
                    out.push(3);
                    zeros = 0;
                }
                zeros = if b == 0 { zeros + 1 } else { 0 };
                out.push(b);
            }
            if zeros >= 2 {
                out.push(3);
            }
            out.push(last);
            out
        })
    }

    proptest! {
        #[test]
        fn split_recovers_nals_for_any_start_code_mix(
            nals in proptest::collection::vec(nal_strategy(), 1..8),
            four in proptest::collection::vec(any::<bool>(), 8),
            trailing in proptest::collection::vec(0usize..4, 8),
        ) {
            let mut s = Vec::new();
            for (i, n) in nals.iter().enumerate() {
                if four[i] { s.push(0); }
                s.extend_from_slice(&[0, 0, 1]);
                s.extend_from_slice(n);
                s.extend(std::iter::repeat_n(0u8, trailing[i]));
            }
            let got: Vec<Vec<u8>> = split_nals(&s).into_iter().map(<[u8]>::to_vec).collect();
            prop_assert_eq!(got, nals.clone());
            // AVCC conversion round-trips back to 4-byte start codes.
            let mut avcc = Vec::new();
            for n in &nals {
                avcc.extend_from_slice(&(n.len() as u32).to_be_bytes());
                avcc.extend_from_slice(n);
            }
            let back = avcc_to_annex_b(&avcc, 4).unwrap();
            let again: Vec<Vec<u8>> = split_nals(&back).into_iter().map(<[u8]>::to_vec).collect();
            prop_assert_eq!(again, nals);
        }

        #[test]
        fn split_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
            for nal in split_nals(&data) {
                prop_assert!(!nal.is_empty());
            }
            let _ = AccessUnit::from_annex_b(&data);
            let _ = avcc_to_annex_b(&data, 4);
            let _ = avcc_nals(&data, 2);
        }
    }
}
