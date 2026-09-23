//! Session windows, native tabs and the menu bar (tasks **M6-2**, **M1-6**, **M2-4**, **M8-3**).
//!
//! Humble object: every decision comes from [`crate::present`], [`crate::menu`] or the
//! [`SessionManager`](crate::manager::SessionManager); this module only talks to Tauri and
//! AppKit.
//!
//! One session = one `NSWindow` = one native tab (plan §1.8):
//! * the window is built **hidden** with the tabbing identifier `drift.sessions`,
//! * `tabbingMode = Preferred` and `addTabbedWindow:ordered:` join it to the group,
//! * AppKit's own tab bar is hidden; a second, 46-point webview at the top of every window draws
//!   Drift's tab strip (task UI-tabs, [`crate::strip`]) and Rust pushes it the group's tabs,
//! * `newWindowForTab:` is installed on tao's window class and opens a tab,
//! * a `RemoteView` is inserted below the page's `WKWebView`, under the strip; the page is
//!   hidden while a live picture is on screen, made transparent over it for the reconnect
//!   overlay, or shrunk to a corner panel for a HUD ([`crate::present::surface_for`],
//!   [`layout`]),
//! * per-window AppKit state (views, observer, render thread) lives in a main-thread-only map.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use drift_core::{InputEvent, KeyboardPrefs, Size, ViewGeometry};
use drift_gfx::FrameSink;
use drift_input::ScaleMode;
use drift_macos::tabs::{self, TABBING_IDENTIFIER};
use drift_macos::{RemoteView, RemoteViewHandler, WindowEvent as MacWindowEvent, WindowObserver, tauri_glue};
use drift_rdp::{SessionCommand, SessionHandle};
use drift_render::{Compositor, Gpu, LayerTarget, RenderThread};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, Submenu, WINDOW_SUBMENU_ID};
use tauri::webview::WebviewBuilder;
use tauri::{
    AppHandle, EventTarget, LogicalPosition, LogicalSize, Manager as _, Runtime, WebviewUrl,
    WebviewWindowBuilder, Window,
};
use uuid::Uuid;

use crate::commands::AppState;
use crate::manager::Reconnect;
use crate::menu::{MenuAction, MenuEntry, Standard, accelerator, menu_spec};
use crate::present::{self, Focus, Surface};
use crate::profiles::CommandError;
use crate::strip::{self, STRIP_HEIGHT, TabStrip, TabStripChanged, strip_label};
use crate::view::{SessionView, SessionViewChanged};
use tauri_specta::Event as _;

/// Tauri label of the `n`th session window.
pub fn window_label(n: u64) -> String {
    format!("session-{n}")
}

// ---- per-window AppKit state (main thread only) --------------------------------------------

type RenderSink = drift_render::RenderSink<Compositor<LayerTarget>>;

struct WindowPlatform {
    view: Retained<RemoteView>,
    window: Retained<NSWindow>,
    /// The page's `WKWebView` (profiles, prompts, overlays, HUDs).
    web: Retained<NSView>,
    /// The tab strip's `WKWebView`, once it has been added.
    strip: Option<Retained<NSView>>,
    /// What the window shows; drives [`layout`] and keyboard focus.
    surface: Surface,
    /// The last [`TabStrip`] pushed to this window (identical pushes are skipped).
    last_strip: Option<TabStrip>,
    link: Rc<SessionLink>,
    _observer: WindowObserver,
    render: Option<RenderThread<Compositor<LayerTarget>>>,
    #[cfg(feature = "recording")]
    recording: Option<crate::recording::Recording>,
}

thread_local! {
    static PLATFORM: RefCell<HashMap<String, WindowPlatform>> = RefCell::new(HashMap::new());
    /// The shared Metal device and pipelines (plan §2: one per process).
    static GPU: RefCell<Option<Result<Gpu, drift_render::RenderError>>> = const { RefCell::new(None) };
}

/// Forwards `RemoteView` output to the window's session actor (M1-6 wiring).
struct SessionLink {
    handle: RefCell<Option<SessionHandle>>,
}

impl SessionLink {
    fn new() -> Rc<Self> {
        Rc::new(Self { handle: RefCell::new(None) })
    }

    fn set(&self, handle: Option<SessionHandle>) {
        *self.handle.borrow_mut() = handle;
    }

    fn send(&self, cmd: SessionCommand) {
        if let Some(handle) = self.handle.borrow().as_ref() {
            let _ = handle.send(cmd);
        }
    }
}

impl RemoteViewHandler for SessionLink {
    fn input(&self, events: &[InputEvent]) {
        for event in events {
            self.send(SessionCommand::Input(*event));
        }
    }

    fn geometry_changed(&self, geometry: ViewGeometry) {
        self.send(SessionCommand::Resize(geometry));
    }
}

fn with_platform<T>(label: &str, f: impl FnOnce(&mut WindowPlatform) -> T) -> Option<T> {
    PLATFORM.with(|p| p.borrow_mut().get_mut(label).map(f))
}

/// Runs `f` on the main thread (inline when already there) and returns its result.
///
/// From another thread the work takes two hops: tao's `run_on_main_thread` first, so it runs
/// after every message already queued for the event loop (a window built with
/// `WebviewWindowBuilder::build` only exists once tao has handled its creation message), then
/// the main dispatch queue ([`drift_macos::dispatch_main`]), so it runs **outside** tao's event
/// handler: AppKit calls that draw synchronously (joining a tab group, selecting a tab) would
/// otherwise re-enter tao's handler and deadlock on its lock.
pub(crate) fn on_main<R: Runtime, T: Send + 'static>(
    app: &AppHandle<R>,
    f: impl FnOnce(MainThreadMarker) -> T + Send + 'static,
) -> Result<T, CommandError> {
    if let Some(mtm) = MainThreadMarker::new() {
        return Ok(f(mtm));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        drift_macos::dispatch_main(move || {
            let Some(mtm) = MainThreadMarker::new() else { return };
            let _ = tx.send(f(mtm));
        });
    })
    .map_err(|e| CommandError::Platform { message: e.to_string() })?;
    rx.recv().map_err(|_| CommandError::Platform { message: "the main thread went away".into() })
}

// ---- opening windows ------------------------------------------------------------------------

/// Opens a new session window as a tab of the current group (returns immediately; the window is
/// built off the main thread, because `WebviewWindowBuilder::build` and `Window::add_child`
/// dispatch to the event loop, and finished on the main thread).
pub fn open_tab<R: Runtime>(app: &AppHandle<R>) {
    open_tab_with(app, None);
}

/// Opens a tab and, when `autoconnect` is set, immediately connects that profile in it.
pub(crate) fn open_tab_with<R: Runtime>(app: &AppHandle<R>, autoconnect: Option<Uuid>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let label = {
            let state = app.state::<AppState>();
            window_label(state.next_window())
        };
        // Size, title bar, traffic lights and window material come from the `session` window
        // template in tauri.conf.json (`create: false`), so they can be tuned without a rebuild.
        let built = session_template(&app, &label).and_then(|config| {
            WebviewWindowBuilder::from_config(&app, &config)?
                .tabbing_identifier(TABBING_IDENTIFIER)
                // The webview must be able to paint over the live picture: the reconnect overlay
                // dims the last frame and the greeter/statistics HUDs float on top of it (M7-3,
                // M1). Needs Tauri's `macos-private-api`; see
                // docs/adr/M7-3-overlays-over-the-live-picture.md.
                .transparent(true)
                .visible(false)
                .build()
        });
        if let Err(e) = built {
            tracing::error!(%label, error = %e, "could not create the session window");
            return;
        }
        if let Err(e) = prepare_window(&app, label.clone()) {
            tracing::error!(%label, error = %e, "could not prepare the session window");
            return;
        }
        // The tab strip is a webview of its own (UI-tabs): the page is hidden or shrunk to a
        // HUD over the live picture, but the strip must always be there.
        if let Err(e) = add_strip(&app, &label) {
            tracing::error!(%label, error = %e, "could not add the tab strip");
        }
        if let Err(e) = show_window(&app, label.clone()) {
            tracing::error!(%label, error = %e, "could not show the session window");
            return;
        }
        // tao moves the traffic lights when the window first draws: lay out again after that.
        std::thread::sleep(std::time::Duration::from_millis(250));
        let handle = app.clone();
        let _ = on_main(&app, move |_| {
            sync_all_chrome(&handle);
            broadcast_tabs(&handle);
        });
        if let Some(profile) = autoconnect
            && let Err(e) = connect_profile(&app, &label, profile)
        {
            tracing::error!(%label, error = %e, "autoconnect failed");
        }
    });
}

/// Label of the session window template in tauri.conf.json.
pub const SESSION_TEMPLATE: &str = "session";

/// The page shown in every window's tab strip webview (built by `ui/build.ts`).
pub const STRIP_PAGE: &str = "strip.html";

/// The `session` window template, relabelled for one tab. The title bar is transparent with no
/// title; the traffic lights sit in the tab strip row; the vibrancy sits below the RemoteView,
/// which is hidden on the page-only screens.
fn session_template<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
) -> tauri::Result<tauri::utils::config::WindowConfig> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == SESSION_TEMPLATE)
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    config.label = label.to_owned();
    config.create = true;
    Ok(config)
}

/// The Tauri window of session window `label`.
fn window_of<R: Runtime>(app: &AppHandle<R>, label: &str) -> Option<Window<R>> {
    app.get_webview(label).map(|page| page.window())
}

fn platform_error(message: impl Into<String>) -> CommandError {
    CommandError::Platform { message: message.into() }
}

/// Attaches the RemoteView below the page and registers the window (main thread; the window is
/// still hidden and has only its page webview).
fn prepare_window<R: Runtime>(app: &AppHandle<R>, label: String) -> Result<(), CommandError> {
    let app2 = app.clone();
    on_main(app, move |mtm| -> Result<(), CommandError> {
        let window =
            window_of(&app2, &label).ok_or_else(|| platform_error(format!("window {label} is gone")))?;
        let (view, web) = tauri_glue::attach(&window, KeyboardPrefs::default())
            .map_err(|e| platform_error(e.to_string()))?;
        let link = SessionLink::new();
        view.set_handler(link.clone());
        let ns = tauri_glue::ns_window(&window).map_err(|e| platform_error(e.to_string()))?;
        tabs::prepare_for_tabs(&ns);
        let observer = {
            let link = link.clone();
            let app = app2.clone();
            let label = label.clone();
            // Tab switches, new or closed tabs and full screen all change the layout; each of
            // them also changes occlusion, key state or full screen.
            WindowObserver::new(&ns, move |event| {
                match event {
                    MacWindowEvent::Occlusion { visible } => link.send(SessionCommand::SetVisible(visible)),
                    MacWindowEvent::Key(focused) => link.send(SessionCommand::Focus(focused)),
                    MacWindowEvent::FullScreen(_) | MacWindowEvent::Resized => {}
                }
                sync_chrome(&app, &label);
                if let MacWindowEvent::Key(true) = event {
                    // "Move Tab to New Window" and friends regroup tabs; the key window changes.
                    broadcast_tabs(&app);
                }
            })
        };
        // `newWindowForTab:` (process-wide; the first installation wins).
        let handle = app2.clone();
        if let Err(e) = tabs::install_new_window_for_tab(&ns, move || open_tab(&handle)) {
            tracing::debug!(error = %e, "newWindowForTab: not installed");
        }
        drift_macos::view::install_key_up_monitor(mtm);
        PLATFORM.with(|p| {
            p.borrow_mut().insert(
                label.clone(),
                WindowPlatform {
                    view,
                    window: ns,
                    web,
                    strip: None,
                    surface: Surface::Webview,
                    last_strip: None,
                    link,
                    _observer: observer,
                    render: None,
                    #[cfg(feature = "recording")]
                    recording: None,
                },
            );
        });
        Ok(())
    })?
}

/// Adds the tab strip webview to window `label` (off the main thread: `add_child` waits for
/// the event loop).
fn add_strip<R: Runtime>(app: &AppHandle<R>, label: &str) -> Result<(), CommandError> {
    let window = window_of(app, label).ok_or_else(|| platform_error(format!("window {label} is gone")))?;
    let builder =
        WebviewBuilder::new(strip_label(label), WebviewUrl::App(STRIP_PAGE.into())).transparent(true);
    // `layout` owns the frame from here on; this is only the initial size.
    window
        .add_child(builder, LogicalPosition::new(0.0, 0.0), LogicalSize::new(1280.0, STRIP_HEIGHT))
        .map_err(|e| platform_error(e.to_string()))?;
    Ok(())
}

/// Finds the strip's `WKWebView`, joins the tab group and shows the window (main thread).
fn show_window<R: Runtime>(app: &AppHandle<R>, label: String) -> Result<(), CommandError> {
    let app2 = app.clone();
    on_main(app, move |_| -> Result<(), CommandError> {
        let window =
            window_of(&app2, &label).ok_or_else(|| platform_error(format!("window {label} is gone")))?;
        let page =
            app2.get_webview(&label).ok_or_else(|| platform_error(format!("window {label} is gone")))?;
        let content = tauri_glue::content_view(&window).map_err(|e| platform_error(e.to_string()))?;
        let (ns, web) = with_platform(&label, |plat| {
            plat.strip = drift_macos::webview::find_webviews(&content)
                .into_iter()
                .find(|v| !std::ptr::eq(Retained::as_ptr(v), Retained::as_ptr(&plat.web)));
            (plat.window.clone(), plat.web.clone())
        })
        .ok_or_else(|| platform_error(format!("window {label} is not registered")))?;
        if let Some(group) = group_leader(&label) {
            tabs::add_tab(&group, &ns);
        }
        tabs::hide_native_tab_bar(&ns);
        ns.makeKeyAndOrderFront(None);
        window.show().map_err(|e| platform_error(e.to_string()))?;
        sync_chrome(&app2, &label);
        broadcast_tabs(&app2);
        tauri_glue::show_webview(&window, &page, &web).map_err(|e| platform_error(e.to_string()))
    })?
}

/// Another session window to join (the key window, else the first one).
fn group_leader(except: &str) -> Option<Retained<NSWindow>> {
    PLATFORM.with(|p| {
        let map = p.borrow();
        let mut labels: Vec<&String> = map.keys().filter(|l| l.as_str() != except).collect();
        labels.sort();
        let key = map.iter().find(|(l, plat)| l.as_str() != except && plat.window.isKeyWindow());
        key.map(|(_, plat)| plat.window.clone())
            .or_else(|| labels.first().and_then(|l| map.get(*l)).map(|plat| plat.window.clone()))
    })
}

/// Number of tabs in the group of window `label` (main thread only).
pub fn tab_count<R: Runtime>(app: &AppHandle<R>, label: &str) -> Option<usize> {
    let _ = app;
    with_platform(label, |plat| tabs::tab_count(&plat.window))
}

/// The label of the key session window (the tab the user is looking at).
pub(crate) fn key_window_label() -> Option<String> {
    PLATFORM.with(|p| {
        p.borrow().iter().find(|(_, plat)| plat.window.isKeyWindow()).map(|(label, _)| label.clone())
    })
}

// ---- session wiring (called by the host) -----------------------------------------------------

/// Creates the tab's render thread over its `RemoteView`'s `CAMetalLayer` and returns the
/// [`FrameSink`] for the session actor.
pub(crate) fn create_render_sink<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
) -> Result<Box<dyn FrameSink>, CommandError> {
    let label = label.to_owned();
    on_main(app, move |_| {
        // One Gpu (device + compiled shaders) for the whole process, as the plan requires.
        let gpu = GPU.with(|gpu| gpu.borrow_mut().get_or_insert_with(Gpu::system_default).clone());
        let gpu = gpu.map_err(|e| CommandError::Platform { message: e.to_string() })?;
        with_platform(&label, |plat| {
            let target = LayerTarget::new(&gpu, plat.view.metal_layer());
            let thread = RenderThread::spawn(&label, move || Compositor::new(gpu, target))
                .map_err(|e| CommandError::Platform { message: e.to_string() })?;
            let sink: RenderSink = thread.sink();
            plat.render = Some(thread);
            Ok(Box::new(sink) as Box<dyn FrameSink>)
        })
        .unwrap_or_else(|| Err(CommandError::Platform { message: format!("window {label} is gone") }))
    })?
}

/// Points the window's input at its new session actor and reports the current geometry.
pub(crate) fn attach_session<R: Runtime>(app: &AppHandle<R>, label: &str, handle: SessionHandle) {
    let label = label.to_owned();
    let _ = on_main(app, move |_| {
        with_platform(&label, |plat| {
            plat.link.set(Some(handle));
            plat.link.send(SessionCommand::Resize(plat.view.geometry()));
            plat.link.send(SessionCommand::SetVisible(drift_macos::window::is_visible(&plat.window)));
            plat.link.send(SessionCommand::Focus(plat.window.isKeyWindow()));
        });
    });
}

/// Applies a new [`SessionView`] to the window: emits it to the page, updates the title and
/// the tab strips, and switches between the page and the live picture.
pub(crate) fn apply_view<R: Runtime>(app: &AppHandle<R>, label: &str, view: &SessionView) {
    let _ = SessionViewChanged(view.clone())
        .emit_to(app, EventTarget::webview_window(label))
        .inspect_err(|e| tracing::warn!(error = %e, "could not emit the session view"));
    let title = present::window_title(Some(view));
    let accessibility_label = present::accessibility_label(Some(view));
    let surface = present::surface_for(view);
    let desktop = match view.state {
        drift_core::SessionState::Connected { desktop, .. } => Some(desktop),
        _ => None,
    };
    let label = label.to_owned();
    let app2 = app.clone();
    let _ = on_main(app, move |_| {
        let (Some(window), Some(page)) = (window_of(&app2, &label), app2.get_webview(&label)) else { return };
        // Never shown in the (hidden) title bar; AppKit lists it in the Window menu.
        let _ = window.set_title(&title);
        let Some(web) = with_platform(&label, |plat| {
            plat.view.set_accessibility_label(&accessibility_label);
            match desktop {
                Some(size) => plat.view.set_desktop(size, ScaleMode::Fit),
                None => plat.view.clear_desktop(),
            }
            plat.surface = surface;
            plat.view.setHidden(surface == Surface::Webview);
            plat.web.clone()
        }) else {
            return;
        };
        sync_chrome(&app2, &label);
        let glue = |e: drift_macos::tauri_glue::GlueError| platform_error(e.to_string());
        let switched: Result<(), CommandError> = match surface {
            Surface::Remote => {
                with_platform(&label, |plat| tauri_glue::show_remote(&window, &page, &plat.view))
                    .unwrap_or(Ok(()))
                    .map_err(glue)
            }
            // With no picture to show, the opaque Metal view would cover the window's vibrancy
            // (hidden above); the overlay keeps it for the dimmed last frame.
            Surface::Webview | Surface::Overlay => {
                tauri_glue::show_webview(&window, &page, &web).map_err(glue)
            }
            // The page is a corner panel (`layout`); everything else still reaches the picture,
            // which keeps the keyboard.
            Surface::Hud(_) => page.show().map_err(|e| platform_error(e.to_string())).map(|()| {
                with_platform(&label, |plat| drift_macos::webview::focus_remote(&plat.window, &plat.view));
            }),
        };
        if let Err(e) = switched {
            tracing::warn!(error = %e, "could not switch the window surface");
        }
        broadcast_tabs(&app2);
    });
}

/// Applies a [`SessionView`] to a real window, for the `overlays_ui` test.
///
/// In production only the `SessionHost` calls [`apply_view`]; the test drives it directly to
/// check what AppKit really does with the overlay surfaces.
#[cfg(feature = "macos-ui-tests")]
pub fn apply_view_for_tests<R: Runtime>(app: &AppHandle<R>, label: &str, view: &SessionView) {
    apply_view(app, label, view);
}

// ---- layout: tab strip, page, picture and HUDs (UI-tabs, M7-3, M1) ---------------------------

/// Lays window `label` out ([`layout`], main thread). A no-op while the window's platform state
/// is being set up or borrowed.
fn sync_chrome<R: Runtime>(app: &AppHandle<R>, label: &str) {
    let _ = app;
    if MainThreadMarker::new().is_none() {
        return;
    }
    PLATFORM.with(|p| {
        let Ok(map) = p.try_borrow() else { return };
        if let Some(plat) = map.get(label) {
            layout(plat);
        }
    });
}

/// [`sync_chrome`] for every session window (main thread).
fn sync_all_chrome<R: Runtime>(app: &AppHandle<R>) {
    let labels: Vec<String> =
        PLATFORM.with(|p| p.try_borrow().map(|m| m.keys().cloned().collect()).unwrap_or_default());
    for label in labels {
        sync_chrome(app, &label);
    }
}

/// Places the tab strip, the picture and the page ([`present::chrome`]):
///
/// * AppKit's tab bar is hidden again (a window gets a fresh one whenever the group changes),
/// * the strip webview is the top [`STRIP_HEIGHT`] points, hidden in full screen,
/// * the `RemoteView` fills the rest,
/// * the page fills the same area for the page-only screens and the reconnect overlay, or is
///   shrunk to [`present::hud_frame`] and pinned to its corner for a HUD — AppKit then hit-tests
///   everything outside the panel straight down to the picture.
fn layout(plat: &WindowPlatform) {
    if tabs::hide_native_tab_bar(&plat.window) {
        tracing::debug!("hid AppKit's tab bar");
    }
    // SAFETY: `superview` returns the (retained) parent or nil; we are on the main thread.
    let Some(parent) = (unsafe { plat.view.superview() }) else { return };
    let bounds = parent.bounds();
    let flipped = parent.isFlipped();
    let full_screen = plat.window.styleMask().contains(NSWindowStyleMask::FullScreen);
    let chrome = present::chrome(full_screen);
    let (width, height) = (bounds.size.width, bounds.size.height);
    let content_height = (height - chrome.content_top).max(1.0);
    let content = NSRect::new(
        NSPoint::new(
            bounds.origin.x,
            bounds.origin.y + present::band_y(height, chrome.content_top, content_height, flipped),
        ),
        NSSize::new(width, content_height),
    );
    let fill = NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable;
    plat.view.setAutoresizingMask(fill);
    plat.view.setFrame(content);
    if let Some(strip) = &plat.strip {
        strip.setHidden(chrome.strip <= 0.0);
        if chrome.strip > 0.0 {
            // Pinned to the top edge: the margin below it grows with the window.
            let below = if flipped {
                NSAutoresizingMaskOptions::ViewMaxYMargin
            } else {
                NSAutoresizingMaskOptions::ViewMinYMargin
            };
            strip.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | below);
            strip.setFrame(NSRect::new(
                NSPoint::new(
                    bounds.origin.x,
                    bounds.origin.y + present::band_y(height, 0.0, chrome.strip, flipped),
                ),
                NSSize::new(width, chrome.strip),
            ));
        }
    }
    match plat.surface {
        Surface::Webview | Surface::Overlay => {
            plat.web.setAutoresizingMask(fill);
            plat.web.setFrame(content);
        }
        Surface::Hud(hud) => {
            let frame = present::hud_frame(Size::new(width, content_height), hud, flipped);
            plat.web.setAutoresizingMask(autoresize_mask(frame.flexible, flipped));
            plat.web.setFrame(NSRect::new(
                NSPoint::new(content.origin.x + frame.x, content.origin.y + frame.y),
                NSSize::new(frame.width, frame.height),
            ));
        }
        Surface::Remote => {}
    }
}

/// Screen-direction flexibility as an AppKit autoresizing mask.
fn autoresize_mask(flexible: present::Flexible, flipped: bool) -> NSAutoresizingMaskOptions {
    let mut mask = NSAutoresizingMaskOptions::empty();
    if flexible.left {
        mask |= NSAutoresizingMaskOptions::ViewMinXMargin;
    }
    if flexible.right {
        mask |= NSAutoresizingMaskOptions::ViewMaxXMargin;
    }
    // "Min Y" is the bottom edge in AppKit's default space and the top edge in a flipped one.
    let (top, bottom) = if flipped {
        (NSAutoresizingMaskOptions::ViewMinYMargin, NSAutoresizingMaskOptions::ViewMaxYMargin)
    } else {
        (NSAutoresizingMaskOptions::ViewMaxYMargin, NSAutoresizingMaskOptions::ViewMinYMargin)
    };
    if flexible.top {
        mask |= top;
    }
    if flexible.bottom {
        mask |= bottom;
    }
    mask
}

/// Shows a remote pointer shape on the window.
pub(crate) fn apply_cursor<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    shape: drift_macos::CursorShape,
    scale: u32,
) {
    let label = label.to_owned();
    let _ = on_main(app, move |_| {
        with_platform(&label, |plat| plat.view.set_cursor_shape(&shape, scale));
    });
}

/// Releases the window's session resources (render thread, handle, cursor).
pub(crate) fn release_session<R: Runtime>(app: &AppHandle<R>, label: &str) {
    let label = label.to_owned();
    let _ = on_main(app, move |_| {
        with_platform(&label, |plat| {
            #[cfg(feature = "recording")]
            if let Some(recording) = plat.recording.take() {
                recording.stop();
            }
            plat.view.release_all();
            plat.link.set(None);
            plat.view.clear_desktop();
            if let Some(render) = plat.render.take() {
                render.shutdown();
            }
        });
    });
}

// ---- the tab strip (UI-tabs) --------------------------------------------------------------------

/// The labels of `window`'s tab group, leading to trailing (windows Drift does not know, such as
/// one being destroyed, are skipped).
fn group_labels(map: &HashMap<String, WindowPlatform>, window: &NSWindow) -> Vec<String> {
    tabs::tab_windows(window)
        .iter()
        .filter_map(|member| {
            map.iter()
                .find(|(_, plat)| std::ptr::eq(Retained::as_ptr(&plat.window), Retained::as_ptr(member)))
                .map(|(label, _)| label.clone())
        })
        .collect()
}

/// The strip of every window, from AppKit's tab groups and the session manager (main thread).
fn strips<R: Runtime>(app: &AppHandle<R>) -> Vec<(String, TabStrip)> {
    let state = app.state::<AppState>();
    let live = state.sessions.live_profiles();
    PLATFORM.with(|p| {
        let Ok(map) = p.try_borrow() else { return Vec::new() };
        map.iter()
            .map(|(label, plat)| {
                let tabs = group_labels(&map, &plat.window)
                    .iter()
                    .map(|l| {
                        strip::tab_item(l, state.sessions.view(l).as_ref(), state.sessions.profile_id(l))
                    })
                    .collect();
                (label.clone(), TabStrip::new(tabs, label, live.clone()))
            })
            .collect()
    })
}

/// The tab strip of window `label` (main thread); `None` for an unknown window.
pub fn tab_strip<R: Runtime>(app: &AppHandle<R>, label: &str) -> Option<TabStrip> {
    strips(app).into_iter().find(|(l, _)| l == label).map(|(_, strip)| strip)
}

/// Pushes every window's [`TabStrip`] to its strip webview and its page (the page marks the
/// connections that are open in another tab), skipping windows whose strip did not change.
fn broadcast_tabs<R: Runtime>(app: &AppHandle<R>) {
    for (label, strip) in strips(app) {
        let changed = PLATFORM.with(|p| {
            let Ok(mut map) = p.try_borrow_mut() else { return true };
            map.get_mut(&label).is_some_and(|plat| {
                let changed = plat.last_strip.as_ref() != Some(&strip);
                plat.last_strip = Some(strip.clone());
                changed
            })
        });
        if !changed {
            continue;
        }
        for target in [EventTarget::webview(strip_label(&label)), EventTarget::webview_window(&label)] {
            let _ = TabStripChanged(strip.clone())
                .emit_to(app, target)
                .inspect_err(|e| tracing::warn!(error = %e, "could not emit the tab strip"));
        }
    }
}

/// Selects tab `label` (a click in the strip, or connecting a profile that is already open):
/// that window becomes the group's selected tab and the key window.
pub fn select_tab<R: Runtime>(app: &AppHandle<R>, label: &str) -> Result<(), CommandError> {
    let label = label.to_owned();
    on_main(app, move |_| {
        // Outside the map's borrow: selecting makes the window key, whose observer lays it out.
        let window = with_platform(&label, |plat| plat.window.clone())
            .ok_or_else(|| platform_error(format!("there is no tab {label}")))?;
        tabs::select_window(&window);
        Ok(())
    })?
}

/// Hands the keyboard back to whatever window `label`'s surface says owns it
/// ([`present::focus_for`]); the strip calls this whenever it receives focus, so a click on a
/// tab never leaves the keyboard in the strip.
pub(crate) fn focus_content<R: Runtime>(app: &AppHandle<R>, label: &str) -> Result<(), CommandError> {
    let label = label.to_owned();
    on_main(app, move |_| {
        with_platform(&label, |plat| match present::focus_for(plat.surface) {
            Focus::Remote => drift_macos::webview::focus_remote(&plat.window, &plat.view),
            Focus::Page => drift_macos::webview::focus_webview(&plat.window, &plat.web),
        });
    })
}

// ---- closing and quitting --------------------------------------------------------------------

/// Closes a tab: the session is closed gracefully (2 s cap), then the window is destroyed.
pub(crate) fn close_tab<R: Runtime>(app: &AppHandle<R>, label: &str) {
    let state = app.state::<AppState>();
    if !state.begin_closing(label) {
        return;
    }
    let closing = state.sessions.close(label);
    let app = app.clone();
    let label = label.to_owned();
    tauri::async_runtime::spawn(async move {
        if !closing.await {
            tracing::warn!(%label, "session did not close within the cap; abandoning it");
        }
        let app2 = app.clone();
        let label2 = label.clone();
        let _ = on_main(&app, move |_| {
            PLATFORM.with(|p| p.borrow_mut().remove(&label2));
            if let Some(window) = window_of(&app2, &label2) {
                let _ = window.destroy();
            }
        });
        app.state::<AppState>().finish_closing(&label);
        let app2 = app.clone();
        let _ = on_main(&app, move |_| broadcast_tabs(&app2));
    });
}

/// Quit: closes every session gracefully within the cap, then exits.
pub(crate) fn quit<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    if !state.begin_quit() {
        return;
    }
    let shutdown = state.sessions.shutdown();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let report = shutdown.await;
        tracing::info!(graceful = report.graceful, abandoned = report.abandoned, "shutdown complete");
        app.exit(0);
    });
}

/// Last-resort shutdown when macOS terminates the app without asking (Dock ▸ Quit).
pub(crate) fn shutdown_blocking<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    if state.sessions.live_sessions() == 0 || !state.begin_quit() {
        return;
    }
    let shutdown = state.sessions.shutdown();
    let report = tauri::async_runtime::block_on(shutdown);
    tracing::info!(graceful = report.graceful, abandoned = report.abandoned, "shutdown on exit");
}

// ---- intents ----------------------------------------------------------------------------------

/// Connects `profile` in `label`'s window (command, menu and autoconnect entry point): the
/// Connection Manager tab becomes the session. If another tab already has a live session for
/// `profile`, that tab is selected instead (UI-tabs board 1).
pub(crate) fn connect_profile<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    profile: Uuid,
) -> Result<(), CommandError> {
    let state = app.state::<AppState>();
    if let Some(other) = state.sessions.live_window_for(profile, label) {
        return select_tab(app, &other);
    }
    let entry = state.profiles.get(profile)?;
    state.sessions.open(label, entry.profile)
}

/// "Reconnect": tells a live session to retry now, or starts the window's profile again.
pub(crate) fn reconnect<R: Runtime>(app: &AppHandle<R>, label: &str) -> Result<(), CommandError> {
    let state = app.state::<AppState>();
    match state.sessions.reconnect_now(label)? {
        Reconnect::Sent => Ok(()),
        Reconnect::Reopen(profile) => connect_profile(app, label, profile),
    }
}

// ---- menu ----------------------------------------------------------------------------------

/// Builds the menu bar from [`menu_spec`].
pub(crate) fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for spec in menu_spec() {
        let submenu = if spec.is_window_menu {
            Submenu::with_id(app, WINDOW_SUBMENU_ID, &spec.title, true)?
        } else {
            Submenu::new(app, &spec.title, true)?
        };
        for entry in spec.entries {
            match entry {
                MenuEntry::Separator => submenu.append(&PredefinedMenuItem::separator(app)?)?,
                MenuEntry::Standard(item) => submenu.append(&predefined(app, item)?)?,
                MenuEntry::Action { action, title, shortcut } => {
                    let mut item = MenuItemBuilder::with_id(action.id(), title);
                    if let Some(shortcut) = shortcut {
                        item = item.accelerator(accelerator(shortcut));
                    }
                    submenu.append(&item.build(app)?)?;
                }
            }
        }
        menu.append(&submenu)?;
    }
    Ok(menu)
}

fn predefined<R: Runtime>(app: &AppHandle<R>, item: Standard) -> tauri::Result<PredefinedMenuItem<R>> {
    match item {
        Standard::About => PredefinedMenuItem::about(app, None, None),
        Standard::Services => PredefinedMenuItem::services(app, None),
        Standard::Hide => PredefinedMenuItem::hide(app, None),
        Standard::HideOthers => PredefinedMenuItem::hide_others(app, None),
        Standard::ShowAll => PredefinedMenuItem::show_all(app, None),
        Standard::Undo => PredefinedMenuItem::undo(app, None),
        Standard::Redo => PredefinedMenuItem::redo(app, None),
        Standard::Cut => PredefinedMenuItem::cut(app, None),
        Standard::Copy => PredefinedMenuItem::copy(app, None),
        Standard::Paste => PredefinedMenuItem::paste(app, None),
        Standard::SelectAll => PredefinedMenuItem::select_all(app, None),
        Standard::Minimize => PredefinedMenuItem::minimize(app, None),
        Standard::Zoom => PredefinedMenuItem::maximize(app, None),
        Standard::Fullscreen => PredefinedMenuItem::fullscreen(app, None),
    }
}

/// Runs a menu action (main thread).
pub(crate) fn run_menu_action<R: Runtime>(app: &AppHandle<R>, action: MenuAction) {
    let label = key_window_label();
    match action {
        MenuAction::NewTab => open_tab(app),
        MenuAction::Quit => quit(app),
        MenuAction::CloseTab => {
            if let Some(label) = label {
                close_tab(app, &label);
            }
        }
        MenuAction::SelectTab(n) => {
            if let Some(window) = label.and_then(|label| with_platform(&label, |plat| plat.window.clone())) {
                tabs::select_tab(&window, usize::from(n) - 1);
            }
        }
        MenuAction::PreviousTab | MenuAction::NextTab => {
            if let Some(label) = label {
                with_platform(&label, |plat| select_sibling_tab(&plat.window, action));
            }
        }
        MenuAction::SendCtrlAltDel => {
            if let Some(label) = label {
                with_platform(&label, |plat| plat.view.send_ctrl_alt_del());
            }
        }
        MenuAction::Reconnect => {
            if let Some(label) = label
                && let Err(e) = reconnect(app, &label)
            {
                tracing::info!(error = %e, "menu: reconnect");
            }
        }
        MenuAction::Disconnect => {
            if let Some(label) = label
                && let Err(e) = app.state::<AppState>().sessions.disconnect(&label)
            {
                tracing::info!(error = %e, "menu: disconnect");
            }
        }
        MenuAction::ToggleStats => {
            if let Some(label) = label
                && let Err(e) = app.state::<AppState>().sessions.toggle_stats(&label)
            {
                tracing::info!(error = %e, "menu: show statistics");
            }
        }
        MenuAction::ToggleRecording => toggle_recording(app, label.as_deref()),
    }
}

fn select_sibling_tab(window: &NSWindow, action: MenuAction) {
    let object: &AnyObject = window.as_ref();
    // SAFETY: `selectNextTab:` / `selectPreviousTab:` are NSWindow methods (main thread);
    // both take a sender and return nothing.
    unsafe {
        if action == MenuAction::NextTab {
            let _: () = msg_send![object, selectNextTab: std::ptr::null::<AnyObject>()];
        } else {
            let _: () = msg_send![object, selectPreviousTab: std::ptr::null::<AnyObject>()];
        }
    }
}

#[cfg(feature = "recording")]
fn toggle_recording<R: Runtime>(app: &AppHandle<R>, label: Option<&str>) {
    let Some(label) = label else { return };
    let profile_name = app
        .state::<AppState>()
        .sessions
        .view(label)
        .map_or_else(|| "session".to_owned(), |view| view.profile_name);
    with_platform(label, |plat| match plat.recording.take() {
        Some(recording) => {
            if let Some(render) = &plat.render {
                render.with(drift_render::Compositor::stop_recording);
            }
            recording.stop();
        }
        None => {
            let Some(render) = &plat.render else {
                tracing::info!("Record Session: no live session in this tab");
                return;
            };
            match crate::recording::start(render, &profile_name) {
                Ok(recording) => plat.recording = Some(recording),
                Err(e) => tracing::error!(error = %e, "could not start recording"),
            }
        }
    });
}

#[cfg(not(feature = "recording"))]
fn toggle_recording<R: Runtime>(app: &AppHandle<R>, label: Option<&str>) {
    let _ = (app, label);
}

/// Writes remote clipboard contents to the general pasteboard, but only while the window is the
/// key one (plan M5-2: only the focused tab syncs).
pub(crate) fn write_pasteboard<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    contents: drift_clipboard::ClipboardContents,
) {
    let label = label.to_owned();
    let _ = on_main(app, move |_| {
        let key = with_platform(&label, |plat| plat.window.isKeyWindow()).unwrap_or(false);
        if key {
            use drift_clipboard::poll::PasteboardPort as _;
            drift_clipboard::pasteboard::NsPasteboard::general().write(&contents);
        }
    });
}
