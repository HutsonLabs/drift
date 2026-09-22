//! A CPU-side BGRA8 image (read-backs, the reference model, goldens).

use drift_core::{Rect, Size};

/// Tightly packed BGRA8 pixels (`width * 4` bytes per row, rows top to bottom).
#[derive(Clone, PartialEq, Eq)]
pub struct BgraImage {
    /// Dimensions in pixels.
    pub size: Size<u32>,
    /// `width * height * 4` bytes.
    pub data: Vec<u8>,
}

impl std::fmt::Debug for BgraImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BgraImage({}x{})", self.size.width, self.size.height)
    }
}

impl BgraImage {
    /// An image filled with one BGRA colour.
    pub fn filled(size: Size<u32>, bgra: [u8; 4]) -> Self {
        let n = size.width as usize * size.height as usize;
        Self { size, data: bgra.repeat(n) }
    }

    fn offset(&self, x: u32, y: u32) -> Option<usize> {
        (x < self.size.width && y < self.size.height)
            .then(|| (y as usize * self.size.width as usize + x as usize) * 4)
    }

    /// BGRA at `(x, y)`, or transparent black when out of range.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        match self.offset(x, y) {
            Some(o) => [self.data[o], self.data[o + 1], self.data[o + 2], self.data[o + 3]],
            None => [0; 4],
        }
    }

    /// Sets `(x, y)`; ignored when out of range.
    pub fn set_pixel(&mut self, x: u32, y: u32, bgra: [u8; 4]) {
        if let Some(o) = self.offset(x, y) {
            self.data[o..o + 4].copy_from_slice(&bgra);
        }
    }

    /// Copies out `rect` (which must lie inside the image) as a new image.
    pub fn crop(&self, rect: Rect) -> BgraImage {
        let mut out = BgraImage::filled(rect.size(), [0; 4]);
        for y in 0..rect.height {
            for x in 0..rect.width {
                out.set_pixel(x, y, self.pixel(rect.x + x, rect.y + y));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_and_crop() {
        let mut i = BgraImage::filled(Size::new(3, 2), [1, 2, 3, 4]);
        i.set_pixel(2, 1, [9, 9, 9, 9]);
        i.set_pixel(3, 0, [7; 4]);
        assert_eq!(i.pixel(2, 1), [9, 9, 9, 9]);
        assert_eq!(i.pixel(5, 5), [0; 4]);
        let c = i.crop(Rect::new(1, 1, 2, 1));
        assert_eq!(c.data, vec![1, 2, 3, 4, 9, 9, 9, 9]);
        assert!(format!("{i:?}").contains("3x2"));
    }
}
