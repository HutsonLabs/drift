//! Confirmation alerts (task **UI-windows**, ADR UI-windows-gallery decisions 4 and 13).
//!
//! Closing a live session window asks with a document-modal sheet on that window
//! ([`confirm_sheet`]); quitting with sessions open asks once with an app-modal alert
//! ([`confirm`]). The texts come from the app's pure `present` module. Both run on the main
//! thread and must be called **outside** tao's event handler (`dispatch_main`): the sheet
//! animation and the modal loop draw synchronously.

use std::cell::Cell;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSModalResponse, NSWindow};
use objc2_foundation::NSString;

/// The words of a two-button confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertText {
    /// Bold first line.
    pub message: String,
    /// Smaller explanation.
    pub informative: String,
    /// The default button (Return).
    pub confirm: String,
    /// The cancel button (Esc).
    pub cancel: String,
}

fn alert(mtm: MainThreadMarker, text: &AlertText) -> Retained<NSAlert> {
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(&text.message));
    alert.setInformativeText(&NSString::from_str(&text.informative));
    alert.addButtonWithTitle(&NSString::from_str(&text.confirm));
    let cancel = alert.addButtonWithTitle(&NSString::from_str(&text.cancel));
    // Esc answers Cancel whatever its title is.
    cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    alert
}

/// Shows `text` as a sheet on `window`; `done(true)` when the user confirms, `done(false)` on
/// cancel. Returns at once.
pub fn confirm_sheet(window: &NSWindow, text: &AlertText, done: impl FnOnce(bool) + 'static) {
    let mtm = MainThreadMarker::from(window);
    let alert = alert(mtm, text);
    let done = Cell::new(Some(done));
    let handler = RcBlock::new(move |response: NSModalResponse| {
        if let Some(done) = done.take() {
            done(response == NSAlertFirstButtonReturn);
        }
    });
    alert.beginSheetModalForWindow_completionHandler(window, Some(&handler));
}

/// Asks `text` app-modally and returns whether the user confirmed (blocks in a modal loop).
pub fn confirm(mtm: MainThreadMarker, text: &AlertText) -> bool {
    alert(mtm, text).runModal() == NSAlertFirstButtonReturn
}
