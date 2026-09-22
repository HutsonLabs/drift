//! Session windows, native tabs and the menu bar (tasks **M6-2**, **M1-6**, **M2-4**, **M8-3**).
//!
//! Humble object: every decision comes from [`crate::present`], [`crate::menu`] or the
//! [`SessionManager`](crate::manager::SessionManager); this module only talks to Tauri and
//! AppKit.
//!
//! One session = one `NSWindow` = one native tab (plan §1.8):
//! * the window is built **hidden** with the tabbing identifier `drift.sessions`,
//! * `tabbingMode = Preferred` and `addTabbedWindow:ordered:` join it to the group,
//! * `newWindowForTab:` (the tab bar's "+") is installed on tao's window class and opens a tab,
//! * a `RemoteView` is inserted below the `WKWebView`; the webview is hidden while a live
//!   picture is on screen, made transparent over it for the reconnect overlay, or shrunk to a
//!   corner panel for a HUD ([`crate::present::surface_for`], [`show_hud`]),
//! * per-window AppKit state (view, observer, render thread) lives in a main-thread-only map.

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
use objc2::{MainThreadMarker, Message as _, msg_send};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, Submenu, WINDOW_SUBMENU_ID};
use tauri::{AppHandle, EventTarget, Manager as _, Runtime, Webview, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;

use crate::commands::AppState;
use crate::manager::Reconnect;
use crate::menu::{MenuAction, MenuEntry, Standard, accelerator, menu_spec};
use crate::present::{self, Surface};
use crate::profiles::CommandError;
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
pub(crate) fn on_main<R: Runtime, T: Send + 'static>(
    app: &AppHandle<R>,
    f: impl FnOnce(MainThreadMarker) -> T + Send + 'static,
) -> Result<T, CommandError> {
    if let Some(mtm) = MainThreadMarker::new() {
        return Ok(f(mtm));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let _ = tx.send(f(mtm));
    })
    .map_err(|e| CommandError::Platform { message: e.to_string() })?;
    rx.recv().map_err(|_| CommandError::Platform { message: "the main thread went away".into() })
}

// ---- opening windows ------------------------------------------------------------------------

/// Opens a new session window as a tab of the current group (returns immediately; the window is
/// built off the main thread, because `WebviewWindowBuilder::build` dispatches to the event
/// loop, and finished on the main thread).
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
        let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
            .title(present::NEW_SESSION_TITLE)
            .inner_size(1280.0, 800.0)
            .min_inner_size(480.0, 320.0)
            .tabbing_identifier(TABBING_IDENTIFIER)
            // The webview must be able to paint over the live picture: the reconnect overlay
            // dims the last frame and the greeter/statistics HUDs float on top of it (M7-3,
            // M1). Needs Tauri's `macos-private-api`; see
            // docs/adr/M7-3-overlays-over-the-live-picture.md.
            .transparent(true)
            .visible(false)
            .build();
        match built {
            Ok(_) => {
                if let Err(e) = finish_window(&app, label.clone()) {
                    tracing::error!(%label, error = %e, "could not finish the session window");
                    return;
                }
                if let Some(profile) = autoconnect
                    && let Err(e) = connect_profile(&app, &label, profile)
                {
                    tracing::error!(%label, error = %e, "autoconnect failed");
                }
            }
            Err(e) => tracing::error!(%label, error = %e, "could not create the session window"),
        }
    });
}

/// Attaches the RemoteView, joins the tab group and shows the window (main thread).
fn finish_window<R: Runtime>(app: &AppHandle<R>, label: String) -> Result<(), CommandError> {
    let app2 = app.clone();
    on_main(app, move |mtm| -> Result<(), CommandError> {
        let window = app2
            .get_webview_window(&label)
            .ok_or_else(|| CommandError::Platform { message: format!("window {label} is gone") })?;
        let view = tauri_glue::attach(&window, KeyboardPrefs::default())
            .map_err(|e| CommandError::Platform { message: e.to_string() })?;
        let link = SessionLink::new();
        view.set_handler(link.clone());
        let ns = ns_window(&window)?;
        tabs::prepare_for_tabs(&ns);
        let observer = {
            let link = link.clone();
            WindowObserver::new(&ns, move |event| match event {
                MacWindowEvent::Occlusion { visible } => link.send(SessionCommand::SetVisible(visible)),
                MacWindowEvent::Key(focused) => link.send(SessionCommand::Focus(focused)),
            })
        };
        // The tab bar's "+" (process-wide; the first installation wins).
        let handle = app2.clone();
        if let Err(e) = tabs::install_new_window_for_tab(&ns, move || open_tab(&handle)) {
            tracing::debug!(error = %e, "newWindowForTab: not installed");
        }
        drift_macos::view::install_key_up_monitor(mtm);
        if let Some(group) = group_leader(&label) {
            tabs::add_tab(&group, &ns);
        }
        PLATFORM.with(|p| {
            p.borrow_mut().insert(
                label.clone(),
                WindowPlatform {
                    view,
                    window: ns.clone(),
                    link,
                    _observer: observer,
                    render: None,
                    #[cfg(feature = "recording")]
                    recording: None,
                },
            );
        });
        ns.makeKeyAndOrderFront(None);
        let _ = window.show();
        tauri_glue::show_webview(&window).map_err(|e| CommandError::Platform { message: e.to_string() })
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

fn ns_window<R: Runtime>(window: &tauri::WebviewWindow<R>) -> Result<Retained<NSWindow>, CommandError> {
    let ptr =
        window.ns_window().map_err(|e| CommandError::Platform { message: e.to_string() })?.cast::<NSWindow>();
    // SAFETY: tao returns its live `NSWindow*`; we are on the main thread and retain it.
    unsafe { ptr.as_ref() }
        .map(|w| w.retain())
        .ok_or_else(|| CommandError::Platform { message: "no NSWindow".into() })
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

/// Applies a new [`SessionView`] to the window: emits it to the webview, updates the title and
/// subtitle, and switches between the webview and the live picture.
pub(crate) fn apply_view<R: Runtime>(app: &AppHandle<R>, label: &str, view: &SessionView) {
    let _ = SessionViewChanged(view.clone())
        .emit_to(app, EventTarget::webview_window(label))
        .inspect_err(|e| tracing::warn!(error = %e, "could not emit the session view"));
    let title = present::window_title(Some(view));
    let subtitle = present::window_subtitle(Some(view));
    let surface = present::surface_for(view);
    let desktop = match view.state {
        drift_core::SessionState::Connected { desktop, .. } => Some(desktop),
        _ => None,
    };
    let label = label.to_owned();
    let app2 = app.clone();
    let _ = on_main(app, move |_| {
        let Some(window) = app2.get_webview_window(&label) else { return };
        let _ = window.set_title(&title);
        with_platform(&label, |plat| {
            plat.window.setSubtitle(&NSString::from_str(&subtitle));
            match desktop {
                Some(size) => plat.view.set_desktop(size, ScaleMode::Fit),
                None => plat.view.clear_desktop(),
            }
        });
        let glue = |e: drift_macos::tauri_glue::GlueError| CommandError::Platform { message: e.to_string() };
        let switched: Result<Option<()>, CommandError> = match surface {
            Surface::Remote => {
                with_platform(&label, |plat| tauri_glue::show_remote(&window, &plat.view).map_err(glue))
                    .transpose()
            }
            Surface::Webview | Surface::Overlay => {
                fill_window_with_webview(&window);
                tauri_glue::show_webview(&window).map_err(glue).map(Some)
            }
            Surface::Hud(hud) => with_platform(&label, |plat| show_hud(&window, &plat.view, hud)).transpose(),
        };
        if let Err(e) = switched {
            tracing::warn!(error = %e, "could not switch the window surface");
        }
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

// ---- overlays over the live picture (M7-3, M1) ------------------------------------------------

/// The window's `WKWebView` (main thread).
fn webview_view<R: Runtime>(window: &tauri::WebviewWindow<R>) -> Option<Retained<NSView>> {
    let ptr = window.ns_view().ok()?.cast::<NSView>();
    // SAFETY: tao returns its live content `NSView*`; we are on the main thread and retain it.
    let content = unsafe { ptr.as_ref() }?.retain();
    drift_macos::webview::find_webview(&content)
}

/// Gives the web view its superview's bounds back (a HUD shrinks it to a corner panel).
fn fill_window_with_webview<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    let Some(web) = webview_view(window) else { return };
    // SAFETY: `superview` returns the (retained) parent or nil; we are on the main thread.
    let Some(parent) = (unsafe { web.superview() }) else { return };
    web.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    web.setFrame(parent.bounds());
}

/// Floats a HUD panel over the live picture.
///
/// The web view is shrunk to [`present::hud_frame`] and pinned to its corner, so AppKit hit-tests
/// everything outside the panel down to the `RemoteView` — which also keeps first responder, so
/// the remote desktop still receives every key.
fn show_hud<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    view: &RemoteView,
    hud: present::Hud,
) -> Result<(), CommandError> {
    let platform = |message: String| CommandError::Platform { message };
    let web = webview_view(window).ok_or_else(|| platform("no WKWebView in the window".into()))?;
    // SAFETY: `superview` returns the (retained) parent or nil; we are on the main thread.
    let parent =
        unsafe { web.superview() }.ok_or_else(|| platform("the WKWebView has no superview".into()))?;
    let bounds = parent.bounds();
    let frame = present::hud_frame(Size::new(bounds.size.width, bounds.size.height), hud, parent.isFlipped());
    let wv: &Webview<R> = window.as_ref();
    wv.show().map_err(|e| platform(e.to_string()))?;
    web.setAutoresizingMask(autoresize_mask(frame.flexible, parent.isFlipped()));
    web.setFrame(NSRect::new(
        NSPoint::new(bounds.origin.x + frame.x, bounds.origin.y + frame.y),
        NSSize::new(frame.width, frame.height),
    ));
    let ns = ns_window(window)?;
    drift_macos::webview::focus_remote(&ns, view);
    Ok(())
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
            if let Some(window) = app2.get_webview_window(&label2) {
                let _ = window.destroy();
            }
        });
        app.state::<AppState>().finish_closing(&label);
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

/// Connects `profile` in `label`'s window (command, menu and autoconnect entry point).
pub(crate) fn connect_profile<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    profile: Uuid,
) -> Result<(), CommandError> {
    let state = app.state::<AppState>();
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
            if let Some(label) = label {
                with_platform(&label, |plat| tabs::select_tab(&plat.window, usize::from(n) - 1));
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
