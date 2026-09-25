//! The Dock menu (task **UI-windows**, ADR UI-windows-gallery decision 10).
//!
//! Tauri has no Dock-menu API. AppKit asks the application delegate for
//! `applicationDockMenu:` each time the user opens the Dock menu, so [`install`] adds that method
//! to the delegate's **class** at runtime (tao's delegate does not implement it) and answers with
//! an `NSMenu` built from the items a provider returns right then. Clicks call back with the
//! item's id. Everything runs on the main thread; the decisions (which items, what ids) are the
//! app's pure `dock::dock_menu`.

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

/// One entry of the Dock menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockMenuItem {
    /// A disabled section title.
    Header(String),
    /// A command: `id` comes back to the click handler; `key` is the key equivalent shown
    /// (with Command), empty for none.
    Item {
        /// Returned to the click handler.
        id: String,
        /// Title.
        title: String,
        /// Key equivalent (shown with Command), or empty.
        key: String,
    },
    /// A separator.
    Separator,
}

/// The Dock menu could not be installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DockError {
    /// The application has no delegate yet.
    #[error("the application has no delegate")]
    NoDelegate,
    /// The delegate's class already implements `applicationDockMenu:` (and it is not ours).
    #[error("{0} already implements applicationDockMenu:")]
    ClassHasMethod(String),
    /// The Objective-C runtime refused to add the method.
    #[error("could not add applicationDockMenu: to {0}")]
    AddFailed(String),
}

type Provider = Box<dyn Fn() -> Vec<DockMenuItem>>;
type OnClick = Rc<dyn Fn(&str)>;

struct Installed {
    provider: Provider,
    on_click: OnClick,
    target: Retained<DockTarget>,
}

thread_local! {
    /// The provider and click handler (main thread only; replaced by a later `install`).
    static DOCK: RefCell<Option<Installed>> = const { RefCell::new(None) };
    /// Classes we added the method to.
    static CLASSES: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

define_class!(
    // The target of every Dock menu item: forwards the item's id to the click handler.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "DriftDockTarget"]
    struct DockTarget;

    impl DockTarget {
        #[unsafe(method(dockItemClicked:))]
        fn dock_item_clicked(&self, sender: Option<&AnyObject>) {
            let Some(item) = sender.and_then(|s| s.downcast_ref::<NSMenuItem>()) else { return };
            let Some(id) = item.representedObject().and_then(|o| o.downcast::<NSString>().ok()) else {
                return;
            };
            let id = id.to_string();
            // Clone the handler out of the cell: it may re-enter (rebuild menus, show windows).
            let on_click = DOCK.with(|d| d.borrow().as_ref().map(|i| i.on_click.clone()));
            if let Some(on_click) = on_click {
                on_click(&id);
            }
        }
    }
);

impl DockTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: plain `NSObject` initialisation of our subclass.
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// Builds the `NSMenu` for `items`, with every command targeting `target`.
fn build_menu(mtm: MainThreadMarker, items: &[DockMenuItem], target: &DockTarget) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    for entry in items {
        let item = match entry {
            DockMenuItem::Separator => NSMenuItem::separatorItem(mtm),
            DockMenuItem::Header(title) => {
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(title));
                item.setEnabled(false);
                item
            }
            DockMenuItem::Item { id, title, key } => {
                // SAFETY: `dockItemClicked:` is implemented by the target set below.
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(title),
                        Some(sel!(dockItemClicked:)),
                        &NSString::from_str(key),
                    )
                };
                // SAFETY: the target is retained by the installation for the process lifetime.
                unsafe { item.setTarget(Some(target)) };
                // SAFETY: an NSString is a valid represented object.
                unsafe { item.setRepresentedObject(Some(&NSString::from_str(id))) };
                item.setEnabled(true);
                item
            }
        };
        menu.addItem(&item);
    }
    menu
}

/// `-[<delegate class> applicationDockMenu:]`.
extern "C-unwind" fn application_dock_menu(
    _this: &AnyObject,
    _cmd: Sel,
    _app: *mut AnyObject,
) -> *mut NSMenu {
    let Some(mtm) = MainThreadMarker::new() else { return std::ptr::null_mut() };
    let menu = DOCK.with(|d| {
        let installed = d.borrow();
        let installed = installed.as_ref()?;
        let items = (installed.provider)();
        Some(build_menu(mtm, &items, &installed.target))
    });
    match menu {
        Some(menu) => Retained::autorelease_return(menu),
        None => std::ptr::null_mut(),
    }
}

fn our_imp() -> Imp {
    let f: extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject) -> *mut NSMenu = application_dock_menu;
    // SAFETY: an `extern "C-unwind"` function with the `@@:@` method signature, stored as the
    // untyped `Imp` the runtime expects (it is called with exactly these arguments).
    unsafe {
        std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject) -> *mut NSMenu, Imp>(f)
    }
}

/// Installs the Dock menu: `provider` is called (main thread) whenever the user opens it, and
/// `on_click` receives the id of the chosen item. A later call replaces both.
///
/// The method goes on the application delegate's real class (not the `NSKVONotifying_`
/// subclass key-value observing may have swapped in), so it must run after the delegate is
/// set (Tauri's `setup` hook).
pub fn install(
    mtm: MainThreadMarker,
    provider: impl Fn() -> Vec<DockMenuItem> + 'static,
    on_click: impl Fn(&str) + 'static,
) -> Result<(), DockError> {
    let app = NSApplication::sharedApplication(mtm);
    let delegate = app.delegate().ok_or(DockError::NoDelegate)?;
    let object: &AnyObject = delegate.as_ref();
    let isa: &AnyClass = object.class();
    let class = if isa.name().to_bytes().starts_with(b"NSKVONotifying_") {
        isa.superclass().unwrap_or(isa)
    } else {
        isa
    };
    let name = class.name().to_string_lossy().into_owned();
    let key = std::ptr::from_ref(class) as usize;
    let selector = sel!(applicationDockMenu:);
    let already = CLASSES.with(|c| c.borrow().contains(&key));
    if !already {
        if class.instance_method(selector).is_some() {
            return Err(DockError::ClassHasMethod(name));
        }
        // SAFETY: adding a method with a matching `@@:@` type encoding to a registered class;
        // the class pointer comes from the runtime.
        let added = unsafe {
            objc2::ffi::class_addMethod(
                std::ptr::from_ref(class).cast_mut(),
                selector,
                our_imp(),
                c"@@:@".as_ptr(),
            )
        };
        if !added.as_bool() {
            return Err(DockError::AddFailed(name));
        }
        CLASSES.with(|c| c.borrow_mut().push(key));
    }
    let installed =
        Installed { provider: Box::new(provider), on_click: Rc::new(on_click), target: DockTarget::new(mtm) };
    DOCK.with(|d| *d.borrow_mut() = Some(installed));
    Ok(())
}
