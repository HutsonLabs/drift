//! Native window tabs (task **M6-2**, platform part; plan §1.8).
//!
//! Each session is its own `NSWindow`. The tabbing identifier alone is not enough (the system
//! preference "Prefer tabs when opening documents" defaults to *in full screen only*), so every
//! session window gets `tabbingMode = Preferred` and is joined explicitly with
//! `addTabbedWindow:ordered:`. The "+" button in the tab bar sends `newWindowForTab:` up the
//! responder chain; tao's window class does not implement it, so
//! [`install_new_window_for_tab`] adds it to the **real** window class (`TaoWindow`), not to the
//! `NSKVONotifying_` subclass that key-value observing installs.
//!
//! Drift draws its own tab strip (task UI-tabs), so AppKit's tab bar is hidden with
//! [`hide_native_tab_bar`] while the windows stay one native group: the Window menu listing,
//! Cmd+1…9 and full screen keep working. `toggleTabBar:` cannot hide the bar once a group has
//! more than one tab, but the bar is an ordinary titlebar accessory view controller, and hiding
//! that (public `NSTitlebarAccessoryViewController.hidden`) collapses it to the title bar row.

use std::sync::{Mutex, OnceLock};

use drift_core::SessionState;
use objc2::Message as _;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::sel;
use objc2_app_kit::{NSWindow, NSWindowOrderingMode, NSWindowTabbingMode};
use objc2_foundation::NSString;

/// The tabbing identifier shared by all session windows.
pub const TABBING_IDENTIFIER: &str = "drift.sessions";

/// The window title (= tab title): a state glyph and the profile name.
///
/// | State | Glyph |
/// |---|---|
/// | `Connected` | `●` |
/// | `AwaitingGreeterLogin` | `◐` |
/// | `Connecting` | `◌` |
/// | `Reconnecting` | `↻` |
/// | `Idle`, `Disconnected` | `○` |
/// | `Failed` | `⚠` |
pub fn tab_title(profile_name: &str, state: &SessionState) -> String {
    let glyph = match state {
        SessionState::Connected { .. } => '●',
        SessionState::AwaitingGreeterLogin => '◐',
        SessionState::Connecting { .. } => '◌',
        SessionState::Reconnecting { .. } => '↻',
        SessionState::Idle | SessionState::Disconnected { .. } => '○',
        SessionState::Failed { .. } => '⚠',
    };
    format!("{glyph} {}", display_name(profile_name))
}

/// A profile name as tabs show it: control characters become spaces, surrounding whitespace is
/// trimmed and a blank name reads "Untitled".
pub fn display_name(profile_name: &str) -> String {
    let cleaned: String = profile_name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let name = cleaned.trim();
    if name.is_empty() { "Untitled".to_owned() } else { name.to_owned() }
}

/// Makes `window` a tab-group member candidate: sets [`TABBING_IDENTIFIER`] and
/// `NSWindowTabbingMode::Preferred`.
pub fn prepare_for_tabs(window: &NSWindow) {
    window.setTabbingIdentifier(&NSString::from_str(TABBING_IDENTIFIER));
    window.setTabbingMode(NSWindowTabbingMode::Preferred);
}

/// Adds `new` as a tab to `group`'s tab group, after `group`'s tab. Selecting (and showing) it
/// is up to the caller (`makeKeyAndOrderFront:`).
pub fn add_tab(group: &NSWindow, new: &NSWindow) {
    group.addTabbedWindow_ordered(new, NSWindowOrderingMode::Above);
}

/// Number of tabs in `window`'s group (1 for a window without tabs).
pub fn tab_count(window: &NSWindow) -> usize {
    window.tabbedWindows().map_or(1, |w| w.count())
}

/// Selects the tab at `index` (0-based) in `window`'s group; `false` if out of range.
pub fn select_tab(window: &NSWindow, index: usize) -> bool {
    let Some(group) = window.tabGroup() else { return false };
    let windows = group.windows();
    if index >= windows.count() {
        return false;
    }
    group.setSelectedWindow(Some(&windows.objectAtIndex(index)));
    true
}

/// The windows of `window`'s tab group, leading to trailing (just `window` without a group).
pub fn tab_windows(window: &NSWindow) -> Vec<Retained<NSWindow>> {
    match window.tabGroup() {
        Some(group) => group.windows().iter().collect(),
        None => vec![window.retain()],
    }
}

/// Selects `window`'s tab in its group and makes it the key window.
pub fn select_window(window: &NSWindow) {
    if let Some(group) = window.tabGroup() {
        group.setSelectedWindow(Some(window));
    }
    window.makeKeyAndOrderFront(None);
}

/// Hides AppKit's tab bar in `window` (idempotent); returns `true` if it was showing.
///
/// Drift adds no titlebar accessories of its own, so every accessory is the tab bar. A window
/// gets a fresh, visible one whenever it joins a group, so call this after every change to the
/// group (Drift does it whenever it lays a window out).
pub fn hide_native_tab_bar(window: &NSWindow) -> bool {
    let mut was_visible = false;
    for accessory in window.titlebarAccessoryViewControllers().iter() {
        if !accessory.isHidden() {
            accessory.setHidden(true);
            was_visible = true;
        }
    }
    was_visible
}

/// `newWindowForTab:` could not be installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TabError {
    /// The class already implements `newWindowForTab:` (and it is not ours).
    #[error("{0} already implements newWindowForTab:")]
    ClassHasMethod(String),
    /// The Objective-C runtime refused to add the method.
    #[error("could not add newWindowForTab: to {0}")]
    AddFailed(String),
}

type NewTabHandler = Box<dyn Fn() + Send + Sync>;

/// The process-wide "+" handler (the first one installed wins).
static NEW_TAB_HANDLER: OnceLock<NewTabHandler> = OnceLock::new();

/// Classes we added the method to (by class pointer).
static INSTALLED_CLASSES: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// `-[TaoWindow newWindowForTab:]`.
extern "C-unwind" fn new_window_for_tab(_this: &AnyObject, _cmd: Sel, _sender: *mut AnyObject) {
    if let Some(handler) = NEW_TAB_HANDLER.get() {
        handler();
    }
}

fn our_imp() -> Imp {
    let f: extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject) = new_window_for_tab;
    // SAFETY: an `extern "C-unwind"` function with the `v@:@` method signature, stored as the
    // untyped `Imp` the runtime expects (it is called with exactly these arguments).
    unsafe { std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject), Imp>(f) }
}

/// Adds `newWindowForTab:` to `window`'s real class, calling `handler` (on the main thread)
/// when the tab bar's "+" is clicked. Idempotent for the same class; the handler is process-wide
/// and the first one installed is kept.
///
/// Key-value observing replaces a window's isa with an `NSKVONotifying_<Class>` subclass; the
/// method goes on `<Class>` so every window of that class responds.
pub fn install_new_window_for_tab(
    window: &NSWindow,
    handler: impl Fn() + Send + Sync + 'static,
) -> Result<(), TabError> {
    let isa: &AnyClass = window.class();
    let class = if isa.name().to_bytes().starts_with(b"NSKVONotifying_") {
        isa.superclass().unwrap_or(isa)
    } else {
        isa
    };
    let name = class.name().to_string_lossy().into_owned();
    let _ = NEW_TAB_HANDLER.set(Box::new(handler));
    let selector = sel!(newWindowForTab:);
    let key = std::ptr::from_ref(class) as usize;
    let mut installed = INSTALLED_CLASSES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if installed.contains(&key) {
        return Ok(());
    }
    if class.instance_method(selector).is_some() {
        return Err(TabError::ClassHasMethod(name));
    }
    // SAFETY: adding a method with a matching `v@:@` type encoding to a registered class; the
    // class pointer comes from the runtime and `class_addMethod` is thread-safe.
    let added = unsafe {
        objc2::ffi::class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            selector,
            our_imp(),
            c"v@:@".as_ptr(),
        )
    };
    if !added.as_bool() {
        return Err(TabError::AddFailed(name));
    }
    installed.push(key);
    Ok(())
}
