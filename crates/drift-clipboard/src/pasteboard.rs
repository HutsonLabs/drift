//! NSPasteboard adapter (task **M5-3**, feature `macos`). A humble object: all decisions live
//! in [`crate::poll`], [`crate::sync`] and [`crate::formats`].
//!
//! Call it from the main thread: the app polls every [`crate::poll::POLL_INTERVAL`] from a
//! main-thread timer and feeds [`crate::poll::PasteboardWatcher`] results to the sessions.
//!
//! Representations: text is `public.utf8-plain-text`; images are `public.png` and
//! `public.tiff`. A TIFF-only pasteboard (e.g. copied from Preview) is converted to PNG with
//! `NSBitmapImageRep`, and PNG writes also offer TIFF for older Mac apps.

use drift_core::ClipboardPrefs;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSPasteboard, NSPasteboardType,
    NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF,
};
use objc2_foundation::{NSData, NSDictionary, NSString};

use crate::poll::PasteboardPort;
use crate::{ClipboardContents, ClipboardItem};

/// An `NSPasteboard` implementing [`PasteboardPort`].
#[derive(Debug)]
pub struct NsPasteboard {
    pb: Retained<NSPasteboard>,
    unique: bool,
}

fn string_type() -> &'static NSPasteboardType {
    // SAFETY: AppKit exports this constant; it is initialised before any Rust code runs and
    // never mutated.
    unsafe { NSPasteboardTypeString }
}

fn png_type() -> &'static NSPasteboardType {
    // SAFETY: as for `string_type`.
    unsafe { NSPasteboardTypePNG }
}

fn tiff_type() -> &'static NSPasteboardType {
    // SAFETY: as for `string_type`.
    unsafe { NSPasteboardTypeTIFF }
}

/// Re-encodes image `data` (any format `NSBitmapImageRep` reads) as PNG.
fn to_png(data: &NSData) -> Option<Vec<u8>> {
    let rep = NSBitmapImageRep::imageRepWithData(data)?;
    let props = NSDictionary::<NSBitmapImageRepPropertyKey, AnyObject>::new();
    // SAFETY: the properties dictionary is empty, so no value can have the wrong type.
    let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }?;
    Some(png.to_vec())
}

/// Re-encodes PNG `data` as TIFF.
fn to_tiff(data: &NSData) -> Option<Retained<NSData>> {
    NSBitmapImageRep::imageRepWithData(data)?.TIFFRepresentation()
}

impl NsPasteboard {
    /// The system general pasteboard (what Cmd+C / Cmd+V use).
    pub fn general() -> Self {
        Self { pb: NSPasteboard::generalPasteboard(), unique: false }
    }

    /// A private pasteboard with a unique name (tests); released globally on drop.
    pub fn unique() -> Self {
        Self { pb: NSPasteboard::pasteboardWithUniqueName(), unique: true }
    }

    /// The underlying pasteboard (for diagnostics and tests).
    pub fn raw(&self) -> &NSPasteboard {
        &self.pb
    }
}

impl PasteboardPort for NsPasteboard {
    fn change_count(&self) -> i64 {
        i64::try_from(self.pb.changeCount()).unwrap_or(i64::MAX)
    }

    fn read(&self, level: ClipboardPrefs) -> ClipboardContents {
        let mut items = Vec::new();
        if level == ClipboardPrefs::Off {
            return ClipboardContents { items };
        }
        if let Some(text) = self.pb.stringForType(string_type()) {
            items.push(ClipboardItem::Text(text.to_string()));
        }
        if level == ClipboardPrefs::TextAndImages {
            if let Some(png) = self.pb.dataForType(png_type()) {
                items.push(ClipboardItem::Png(png.to_vec()));
            } else if let Some(tiff) = self.pb.dataForType(tiff_type()) {
                if let Some(png) = to_png(&tiff) {
                    items.push(ClipboardItem::Png(png));
                }
                items.push(ClipboardItem::Tiff(tiff.to_vec()));
            }
        }
        ClipboardContents { items }
    }

    fn write(&self, contents: &ClipboardContents) -> i64 {
        self.pb.clearContents();
        let has_tiff = contents.items.iter().any(|i| matches!(i, ClipboardItem::Tiff(_)));
        for item in &contents.items {
            match item {
                ClipboardItem::Text(t) => {
                    self.pb.setString_forType(&NSString::from_str(t), string_type());
                }
                ClipboardItem::Png(p) => {
                    let data = NSData::with_bytes(p);
                    self.pb.setData_forType(Some(&data), png_type());
                    if !has_tiff && let Some(tiff) = to_tiff(&data) {
                        self.pb.setData_forType(Some(&tiff), tiff_type());
                    }
                }
                ClipboardItem::Tiff(t) => {
                    self.pb.setData_forType(Some(&NSData::with_bytes(t)), tiff_type());
                }
            }
        }
        self.change_count()
    }
}

impl Drop for NsPasteboard {
    fn drop(&mut self) {
        if self.unique {
            // SAFETY: `-[NSPasteboard releaseGlobally]` takes no arguments and returns void; it
            // is not bound by objc2-app-kit 0.3. `self.pb` is a valid, retained pasteboard.
            let () = unsafe { objc2::msg_send![&*self.pb, releaseGlobally] };
        }
    }
}
