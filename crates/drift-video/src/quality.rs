//! Picture-quality metrics (pure): PSNR for golden comparisons of decoded frames.

use drift_core::Nv12Planes;

/// Peak signal-to-noise ratio in dB between two 8-bit sample buffers of equal length.
///
/// Returns `None` if the lengths differ or the buffers are empty, and `f64::INFINITY` if they
/// are identical.
pub fn psnr(a: &[u8], b: &[u8]) -> Option<f64> {
    let _ = (a, b);
    None
}

/// PSNR over all samples (Y and interleaved CbCr) of two NV12 pictures of the same size.
pub fn psnr_nv12(a: &Nv12Planes, b: &Nv12Planes) -> Option<f64> {
    let _ = (a, b);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use drift_core::Size;

    #[test]
    fn identical_is_infinite() {
        assert_eq!(psnr(&[1, 2, 3], &[1, 2, 3]), Some(f64::INFINITY));
    }

    #[test]
    fn known_value() {
        // MSE = 1 -> 10*log10(255^2) = 48.1308...
        let p = psnr(&[10, 10, 10, 10], &[11, 9, 11, 9]).unwrap();
        assert!((p - 48.1308).abs() < 1e-3, "{p}");
        // MSE = 100 -> 28.1308
        let p = psnr(&[0, 0], &[10, 10]).unwrap();
        assert!((p - 28.1308).abs() < 1e-3, "{p}");
    }

    #[test]
    fn mismatched_or_empty() {
        assert_eq!(psnr(&[], &[]), None);
        assert_eq!(psnr(&[1], &[1, 2]), None);
    }

    #[test]
    fn nv12_combines_planes() {
        let s = Size::new(2, 2);
        let a = Nv12Planes::new(s, vec![0; 4], vec![0; 2]).unwrap();
        let b = Nv12Planes::new(s, vec![0; 4], vec![0, 6]).unwrap();
        // one sample of six differs by 6 -> MSE 6
        let expect = 10.0 * (255.0f64 * 255.0 / 6.0).log10();
        assert!((psnr_nv12(&a, &b).unwrap() - expect).abs() < 1e-9);
        let c = Nv12Planes::new(Size::new(4, 2), vec![0; 8], vec![0; 4]).unwrap();
        assert_eq!(psnr_nv12(&a, &c), None);
    }
}
