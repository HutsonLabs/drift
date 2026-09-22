//! Surface and cache bookkeeping (pure). Every rectangle and point the server sends is checked
//! here before anything reaches the [`FrameSink`](crate::FrameSink).

use std::collections::BTreeMap;

use drift_core::{Point, Rect, Size};
use ironrdp_egfx::pdu::{PixelFormat, Point as GfxPoint};
use ironrdp_pdu::geometry::ExclusiveRectangle;

use crate::error::GfxError;

/// Cache slots available to the server: `1..=MAX_CACHE_SLOTS`. Drift advertises neither
/// `SMALL_CACHE` nor `THIN_CLIENT`, so the full MS-RDPEGFX budget applies (FreeRDP uses the
/// same bound).
pub(crate) const MAX_CACHE_SLOTS: u16 = 25_600;

/// One server surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Surface {
    pub size: Size<u32>,
    pub format: PixelFormat,
}

/// Surfaces and cache slots of one graphics session.
#[derive(Debug, Default)]
pub(crate) struct GfxState {
    surfaces: BTreeMap<u16, Surface>,
    /// Size of the image held by each occupied cache slot.
    cache: BTreeMap<u16, Size<u32>>,
}

impl GfxState {
    /// Drops every surface and cache slot (`ResetGraphics`); returns the dropped surface ids.
    pub fn reset(&mut self) -> Vec<u16> {
        self.cache.clear();
        std::mem::take(&mut self.surfaces).into_keys().collect()
    }

    /// Registers a new surface.
    pub fn create(&mut self, id: u16, size: Size<u32>, format: PixelFormat) -> Result<(), GfxError> {
        let invalid = |detail: &str| GfxError::InvalidSurface { id, detail: detail.to_owned() };
        if size.width == 0 || size.height == 0 {
            return Err(invalid("empty surface"));
        }
        if self.surfaces.contains_key(&id) {
            return Err(invalid("surface id already exists"));
        }
        self.surfaces.insert(id, Surface { size, format });
        Ok(())
    }

    /// Removes a surface; `false` if it did not exist.
    pub fn delete(&mut self, id: u16) -> bool {
        self.surfaces.remove(&id).is_some()
    }

    /// Looks up a surface.
    pub fn surface(&self, id: u16) -> Result<Surface, GfxError> {
        self.surfaces.get(&id).copied().ok_or(GfxError::UnknownSurface(id))
    }

    /// Checks that `slot` is a valid cache slot number.
    pub fn check_slot(slot: u16) -> Result<(), GfxError> {
        if slot == 0 || slot > MAX_CACHE_SLOTS {
            return Err(GfxError::InvalidCacheSlot(slot));
        }
        Ok(())
    }

    /// Stores an image of `size` in `slot` (replacing any previous entry).
    pub fn cache_store(&mut self, slot: u16, size: Size<u32>) -> Result<(), GfxError> {
        Self::check_slot(slot)?;
        self.cache.insert(slot, size);
        Ok(())
    }

    /// Size of the image in `slot`.
    pub fn cache_entry(&self, slot: u16) -> Result<Size<u32>, GfxError> {
        self.cache.get(&slot).copied().ok_or(GfxError::InvalidCacheSlot(slot))
    }

    /// Frees `slot`; `false` if it was empty.
    pub fn cache_evict(&mut self, slot: u16) -> Result<bool, GfxError> {
        Self::check_slot(slot)?;
        Ok(self.cache.remove(&slot).is_some())
    }
}

/// Converts an `RDPGFX_RECT16` (exclusive right/bottom) into a [`Rect`].
pub(crate) fn rect(r: &ExclusiveRectangle) -> Result<Rect, GfxError> {
    Rect::from_ltrb(u32::from(r.left), u32::from(r.top), u32::from(r.right), u32::from(r.bottom))
        .ok_or_else(|| GfxError::Decode(format!("inverted rectangle {r:?}")))
}

/// Converts an `RDPGFX_POINT16`.
pub(crate) fn point(p: &GfxPoint) -> Point<u32> {
    Point::new(u32::from(p.x), u32::from(p.y))
}

/// `r` must lie entirely inside a surface of `bounds`.
pub(crate) fn ensure_within(surface: u16, bounds: Size<u32>, r: Rect, what: &str) -> Result<(), GfxError> {
    if r.fits_within(bounds) {
        Ok(())
    } else {
        Err(GfxError::OutOfBounds {
            surface,
            detail: format!("{what} {r:?} exceeds {}x{}", bounds.width, bounds.height),
        })
    }
}

/// `r` clipped to a surface of `bounds`; `None` when nothing is left.
pub(crate) fn clip(r: Rect, bounds: Size<u32>) -> Option<Rect> {
    let right = r.right().min(bounds.width);
    let bottom = r.bottom().min(bounds.height);
    let clipped = Rect::from_ltrb(r.x, r.y, right, bottom)?;
    (!clipped.is_empty()).then_some(clipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_and_slots() {
        let mut s = GfxState::default();
        s.create(1, Size::new(4, 4), PixelFormat::XRgb).unwrap();
        assert!(matches!(
            s.create(1, Size::new(4, 4), PixelFormat::XRgb),
            Err(GfxError::InvalidSurface { .. })
        ));
        assert!(matches!(
            s.create(2, Size::new(0, 4), PixelFormat::XRgb),
            Err(GfxError::InvalidSurface { .. })
        ));
        assert_eq!(s.surface(1).unwrap().size, Size::new(4, 4));
        assert_eq!(s.surface(9), Err(GfxError::UnknownSurface(9)));
        assert_eq!(s.cache_store(0, Size::new(1, 1)), Err(GfxError::InvalidCacheSlot(0)));
        assert_eq!(
            s.cache_store(MAX_CACHE_SLOTS + 1, Size::new(1, 1)),
            Err(GfxError::InvalidCacheSlot(25_601))
        );
        s.cache_store(MAX_CACHE_SLOTS, Size::new(2, 3)).unwrap();
        assert_eq!(s.cache_entry(MAX_CACHE_SLOTS).unwrap(), Size::new(2, 3));
        assert_eq!(s.cache_evict(MAX_CACHE_SLOTS), Ok(true));
        assert_eq!(s.cache_evict(MAX_CACHE_SLOTS), Ok(false));
        assert!(s.delete(1));
        assert!(!s.delete(1));
        s.create(3, Size::new(1, 1), PixelFormat::ARgb).unwrap();
        s.cache_store(5, Size::new(1, 1)).unwrap();
        assert_eq!(s.reset(), vec![3]);
        assert!(s.cache_entry(5).is_err());
    }

    #[test]
    fn clipping() {
        assert_eq!(clip(Rect::new(2, 2, 10, 10), Size::new(8, 6)), Some(Rect::new(2, 2, 6, 4)));
        assert_eq!(clip(Rect::new(9, 0, 1, 1), Size::new(8, 6)), None);
        assert_eq!(clip(Rect::new(0, 0, 0, 1), Size::new(8, 6)), None);
        assert!(ensure_within(1, Size::new(4, 4), Rect::new(0, 0, 4, 4), "r").is_ok());
        let e = ensure_within(1, Size::new(4, 4), Rect::new(1, 0, 4, 4), "source").unwrap_err();
        assert!(e.to_string().contains("source"), "{e}");
        assert!(rect(&ExclusiveRectangle { left: 5, top: 0, right: 4, bottom: 1 }).is_err());
    }
}
