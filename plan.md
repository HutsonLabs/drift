# Drift — Delivery Plan (verified)

Drift is a macOS-only Tauri 2 app written in Rust. It connects to **GNOME Remote Desktop (g-r-d) on Wayland, GNOME 50+**, over RDP.

Every technical assumption in this plan was **tested on 2026-09-21/22** against a real GNOME 50 host (`homelab@10.1.2.40`) and on a macOS 27 Apple Silicon dev machine. §1 lists what was verified and where the evidence is. The plan contains **no fallback branches**: each decision below is the one that was proven to work.

Read §0–§5 before starting any task.

---

## 0. Ground rules (non-negotiable)

| Rule | Meaning |
|---|---|
| **TDD always** | Red → Green → Refactor for every task. First write the listed failing tests, run them, and watch them fail *for the right reason*. Then write the smallest implementation that passes. Then refactor while staying green. Production code only merges together with a test that failed before that code existed. |
| **Cargo first** | Use `cargo` for build, test, lint, run, bundle and automation (`cargo xtask …`). The Tauri CLI is `cargo tauri` (`cargo install tauri-cli --version "^2" --locked`). |
| **Bun, never npm** | JS/TS exists only for the thin webview UI in `ui/`. Use `bun install`, `bun test`, `bun run build`. No `npm`, `npx`, `yarn` or `pnpm` in scripts, CI or docs. |
| **macOS only** | Targets are `aarch64-apple-darwin` (primary) and `x86_64-apple-darwin` (universal bundle). Minimum macOS **14.0**. No code paths, stubs or CI for any other OS. |
| **Thin UI** | All logic lives in Rust. TypeScript renders state and sends intents back. |
| **Humble objects** | FFI and main-thread glue (AppKit, Metal, VideoToolbox, NSPasteboard, Network.framework) stays as thin as possible. Decision logic lives in pure, unit-tested Rust. |
| **Optimised deps in dev** | `[profile.dev.package."*"] opt-level = 3` and `[profile.dev.package.drift-codec] opt-level = 3`. This is verified: a debug-built decoder acks frames late, and g-r-d then throttles the stream from 60 fps to about 30 fps. |

### TDD protocol per task
1. Create branch `task/<ID>-<slug>`.
2. Write the **Red** tests, run them, and confirm they fail. Commit as `test(<ID>): …`.
3. Implement until green. Commit as `feat(<ID>): …`.
4. Refactor, then `cargo xtask ci` must pass.
5. Open a PR. It lists the Red tests with their failing output, and ticks the task checkbox here.
6. Record any decision not already fixed in this file as an ADR in `docs/adr/`.

### Test layers
| Layer | Tooling | Runs in CI |
|---|---|---|
| Unit / property | `cargo nextest`, `proptest`, `insta` | every PR |
| Wire fixtures | Real bytes captured from g-r-d 50.2 (§1.7), replayed through parsers and state machines | every PR |
| Loopback integration | `drift-testkit::FakeServer` (built on `ironrdp-server` plus scripted PDUs) on 127.0.0.1 | every PR |
| GPU golden images | Offscreen Metal render → readback → compare with PNG (per-channel tolerance ≤ 2) | every PR (macOS arm64 runners have Metal) |
| UI logic | `bun test` + `happy-dom` | every PR |
| E2E vs. real GNOME 50 | `cargo xtask e2e` through **SSH local forwards** (see §5; required because of macOS Local Network Privacy, §1.9) | nightly + before each milestone sign-off |
| Manual acceptance | `docs/acceptance.md` checklist per milestone | per milestone |

Coverage gate (`cargo llvm-cov`): **≥ 85 %** line coverage for `drift-core`, `drift-input`, `drift-clipboard`, `drift-gfx` and `drift-rdp::redirect`/`rdstls`. FFI crates need smoke tests only.

---

## 1. Verified facts (source of truth)

The evidence lives in `~/code/drift-spikes/` on the dev machine. It is imported into the repo in task M0-3.
- `probe2/`: a working reference client that implements redirection, RDSTLS, clipboard, input, resize and suppress on top of IronRDP.
- `tauri-spike/`: Metal view, tabs and NV12 zero-copy experiments.
- `gnome-remote-desktop/` (tag 50.2), `gdm/` (tag 50.1), `IronRDP/` (rev `b149f50`) and `ref/` (FreeRDP 3.31.1 `rdstls.c`, `redirection.c`).
- `ironrdp-drift.patch`.

### 1.1 Host under test
- AnduinOS 2.0.3 (Ubuntu 26.04 base), kernel 7.0, **GNOME Shell 50.1, gdm3 50.1, gnome-remote-desktop 50.2** (built with FreeRDP 3), Ryzen 5 3400GE with Radeon Vega 11.
- **VAAPI H.264 encode works** (`[HWAccel.VAAPI] Successfully initialized … radeonsi`) and Vulkan works.
- Daemons found:
  - System daemon (Remote Login) on **:3389**.
  - The user's Desktop Sharing daemon on **:3390**; port negotiation moved it because 3389 was taken.
  - Headless daemons for the test users on **:3391** and **:3392**, also negotiated.

### 1.2 Connection modes (all three verified end to end)
| Mode | Server side | Client auth | Reconnect behaviour |
|---|---|---|---|
| **Remote Login** | System daemon; GDM greeter, then a user session | Leg 1: NLA (CredSSP/NTLM) with the **system** RDP credentials. Legs 2 and 3: **RDSTLS** with one-time credentials from a Server Redirection PDU | Always goes back to the **GDM greeter**. After the user logs in again, GDM hands the connection to the **same existing session**. GDM 50.1 has no credential-injection API (`CreateRemoteDisplay(a{sv})`, no credentials), so the greeter step can't be skipped. |
| **Headless session** | A persistent per-user headless GNOME session (`gnome-headless-session@<user>.service`) with the g-r-d headless daemon (`grdctl --headless`) | NLA with fixed per-user RDP credentials | **Direct and seamless:** reconnect lands straight back in the same session with the same windows. |
| **Desktop Sharing** | The user daemon mirroring an existing session's monitor | NLA with the Desktop Sharing credentials | Direct. **No Display Control channel is offered**, so there's no remote resize and the client scales to fit. |

### 1.3 Remote Login wire flow (exact)
1. **Leg 1.** Connect to :3389 with protocols `SSL|HYBRID`; the server selects HYBRID. NLA uses the system credentials. The client must set `RNS_UD_CS_SUPPORT_DYNVC_GFX_PROTOCOL`, otherwise the server closes the connection ("Client did not advertise support for the Graphics Pipeline"). About **2.4 s** after activation the server sends an Enhanced Security **Server Redirection PDU**:
   - Share Control `pduType` = `0xA`, then `pad2Octets`, then `RDP_SERVER_REDIRECTION_PACKET`.
   - The packet's `flags` field is `0x0400`.
   - `redirFlags=0x1C016`, which is `LB_LOAD_BALANCE_INFO | LB_USERNAME | LB_PASSWORD | LB_PASSWORD_IS_PK_ENCRYPTED | LB_REDIRECTION_GUID | LB_TARGET_CERTIFICATE`.
   - The fields appear in `redirFlags` bit order, each as a `u32` length plus bytes:
     - LB info is ASCII `"Cookie: msts=<u32 decimal>\r\n"`.
     - Username is UTF-16LE, 16 chars, one-time.
     - Password is a 34-byte opaque blob; forward it verbatim and never decrypt it.
     - GUID is 50 bytes (UTF-16 base64).
     - Target certificate is 3256 bytes. It holds UTF-16LE base64 text; decoded, it's a list of `{u32 type, u32 encoding, u32 len, data}`. Type `0x20`, encoding 1 is DER, and it is **byte-identical to the TLS certificate presented on the next leg**.
   - No target address is sent, so the client reconnects to the same host and port.
2. **Leg 2 (GDM greeter).** Send the X.224 Connection Request with routing token `Cookie: msts=<token>\r\n` and requested protocols `SSL|RDSTLS` (0x5); the server selects RDSTLS (0x4). After TLS:
   - The server sends RDSTLS capabilities `01 00 01 00 01 00 03 00` (supported versions v1|v2).
   - The client sends an AuthRequest: `u16 version=1`, `u16 type=2`, `u16 dataType=1`, then `u16 len`+GUID, `u16 len`+username (UTF-16 with NUL), `u16 len`+domain (`""` gives `02 00 00 00`), and `u16 len`+password blob.
   - The server answers with an AuthResponse: `version=1, type=4, dataType=1, u32 result`. `0` means success.
   - The client info carries `INFO_AUTOLOGON` and an empty password.
   - The greeter appears about 3 s later.
3. **Leg 3 (user session).** After the user logs in at the greeter, a second redirection arrives with **the same routing token** and new one-time credentials. RDSTLS happens again and the user session appears.
   - Reusing those one-time credentials later fails with RDSTLS result **`0x52E` (LOGON_FAILURE)**. They are single-use and must never be persisted.
   - A new client **takes over** a session even while the previous client's TCP connection is still open (the dead-peer case was verified with a `SIGSTOP`ed client).
4. Legs connect in 115–240 ms each.

### 1.4 Graphics
- The **graphics pipeline is mandatory**. Caps advertised as **`[V8_1{AVC420_ENABLED}, V8{}]`** are confirmed as V8.1, and the server sends **AVC420** when it has hardware encode.
  - Advertising any V10.x set **without** `AVC_DISABLED` makes the server choose **AVC444v2**.
  - Advertising V10.x **with** `AVC_DISABLED` makes the server choose V10.7 and use **RFX Progressive**, even when V8.1 AVC420 is also offered, because the server prefers the highest version.
  - **So Drift advertises exactly `[V8_1{AVC420_ENABLED}, V8{}]`.**
- g-r-d chooses the codec per surface: AVC444v2 if negotiated, otherwise AVC420 if a VAAPI/NVENC session can be created, otherwise **RFX Progressive** (`CAPROGRESSIVE`). Progressive was verified with V8, V8.1 without AVC, and V10.7 with `AVC_DISABLED`, and IronRDP's progressive decoder renders the greeter correctly.
- **The AVC420 bitstream is Annex-B** (start codes, with an AUD `09 30` first), High profile, level 4.0, no B-frames, **no colour description in the VUI**.
  - The encoder shader is `Y = (54R + 183G + 18B) >> 8`, `U = ((-29R - 99G + 128B) >> 8) + 128` and `V = ((128R - 116G - 12B) >> 8) + 128`. That means **BT.709, full range**, and our YUV→RGB shader must hard-code it.
  - IronRDP's `ironrdp_egfx::decode` path assumes length-prefixed AVC and fails on g-r-d streams. **Drift does not use that path** (see §2).
- **VideoToolbox hardware-decodes** the captured g-r-d stream (398/398 frames, High profile, level 4.0). IOSurface-backed NV12 `CVPixelBuffer` → `CVMetalTextureCache` → Metal is verified zero-copy (the Y and UV planes are shared with the GPU).
- Throughput: a full-screen, every-frame repaint at 1280×800 rendered **58.9 fps** in the session, and g-r-d delivered **60.0 fps** of AVC420 at about 1.2 Mbit/s (synthetic content). g-r-d's target refresh is 60 Hz. It throttles when unacknowledged frames reach `max(2, min(rtt·60+2, 60))`, so **frame acks must go out immediately after present**.
- **Suppress Output** (`allowDisplayUpdates=0`) means zero frames arrive. On allow, the server re-sends a correct full frame. **Refresh Rect is not supported** (g-r-d sets `FreeRDP_RefreshRect=FALSE`), so never rely on it.
- **Display Control (DISP)** is available in Remote Login and Headless modes. Each resize produces `ResetGraphics` plus a **new surface id**. Width is forced even (1281 became 1280); height 801 was accepted.
- **Retina:** 2560×1600 with `DesktopScaleFactor=200` gives GNOME at 2× (identical layout, pixel-doubled), and pointer bitmaps are 86×86 instead of 43×43.

### 1.5 Input
- Fast-path **scancodes** work: Ctrl+Alt+T, Shift chords (typing gives "ABC" whether sent in one batch or split), Enter and Print Screen (extended `0x37`). **Unicode** keyboard events also work, including typing the GDM password.
- g-r-d maps scancodes to evdev keycodes and **ignores the client's keyboard layout**, so the character produced depends on the *remote* XKB layout. Unicode events are layout-independent. g-r-d de-duplicates pressed keys and releases held keys when the client disconnects.
- Absolute mouse movement and clicks work in desktop pixels.
- **Wheel:** g-r-d computes `step = value/120 × 10 px` as a fractional value. 30 events of +12 scrolled the same distance as 3 events of +120 (10 terminal lines), so **fractional units give smooth trackpad scrolling**. +120 is scroll up. Horizontal wheel (`PTR_FLAGS_HWHEEL`) is sign-inverted inside g-r-d, so the sign is fixed by an e2e test (M2-3).

### 1.6 Clipboard (CLIPRDR)
- The client **must answer the server's initial format-list request** (by sending its own list, possibly empty). Otherwise IronRDP's `Cliprdr` never reaches the ready state and ignores remote copies.
- Remote → local, as observed:
  - Text copied in GNOME Text Editor arrives as `[CF_TEXT(1), CF_UNICODETEXT(13)]`.
  - An image copied with the GNOME screenshot tool arrives as the named format **`"image/png"`** (id `0xD011`), a valid PNG.
- g-r-d's source also maps `CF_DIB` ↔ image/bmp, `CF_TIFF` ↔ image/tiff, `"image/jpeg"`, `"image/gif"`, `"HTML Format"`, and `FileGroupDescriptorW` ↔ uri-list.
- Local → remote: advertising `CF_UNICODETEXT` plus a registered `"image/png"` works. GTK4 in the session read the text, and the PNG as a 320×200 texture.
- A clipboard change made by an **unfocused** Wayland client isn't propagated. That's Wayland/mutter behaviour, not ours; real apps (Text Editor, the screenshot tool) work.
- IronRDP's `CB_TEMP_DIRECTORY` PDU triggers a server warning ("header told length is 520, but actually read 0"). It's harmless; fix it in our fork (M0-2).

### 1.7 Captured fixtures (import in M0-3)
- `leg2.h264`, the greeter stream: 2 frames.
- `leg3.h264`, full-screen motion: 398 frames at 1280×800.
- A redirection PDU and a target-certificate container (their credentials are one-time and already invalid, but sanitize them anyway).
- `tls_cert_leg*.der`, RDSTLS capabilities and response bytes.
- `remote_clip_d011.bin` (PNG) and CF_UNICODETEXT samples.
- Screenshots: greeter, desktop, and 200% Retina.

### 1.8 macOS side (tauri 2.11.6, wry 0.55.1, tao 0.35.3, objc2 0.6.4, objc2-* 0.3.2)
- A layer-hosting `NSView` subclass (`define_class!`) with a `CAMetalLayer`, placed via `addSubview:positioned:Below relativeTo:WKWebView` in `webview_window.ns_view()`, works. Rendering from a background thread hit 180/180 frames in 3 s with `contentsScale=2`. `drawableSize` must be updated in `setFrameSize:` and `viewDidChangeBackingProperties`.
- Hide the webview with `tauri::Webview::hide()`/`show()` (not `WebviewWindow::hide()`, which hides the whole window), then `makeFirstResponder(remoteView)`.
- keyDown, keyUp, flagsChanged, mouse events and precise `scrollWheel` (`hasPreciseScrollingDeltas=true`) all reach the view. **Cmd and Ctrl combos go through `performKeyEquivalent:` before the menu.** Returning YES claims them: Cmd+C would otherwise be eaten by the Edit menu, and Ctrl+Tab by AppKit.
- **Native tabs:** `WebviewWindowBuilder::tabbing_identifier`, plus `NSWindow.setTabbingMode(Preferred)` or `addTabbedWindow:ordered:`. The identifier alone isn't enough, because the system preference defaulted to "in full screen only". `newWindowForTab:` must be added to the **`TaoWindow` class** (not tao's KVO subclass). Cmd+T works as a Tauri menu accelerator.
- **Occlusion:** only the selected tab reports `occlusionState ∋ Visible`, and `NSWindowDidChangeOcclusionStateNotification` fires on every tab switch. **Background tabs keep rendering at 60 fps unless Drift pauses them.**
- **Local Network Privacy:** freshly built ad-hoc-signed binaries get `EHOSTUNREACH` (errno 65) connecting to LAN hosts, while `ssh`, `nc` and loopback are unaffected. The app needs `NSLocalNetworkUsageDescription`, a **stable signing identity** (so the permission survives rebuilds), and specific UI for errno 65. Test binaries reach the host through SSH forwards on 127.0.0.1.
- The **VideoToolbox H.264 encoder** is available: `h264_videotoolbox` produced 1080p60 High profile, 180/180 frames.

### 1.9 Host setup facts
- Desktop Sharing credentials live in the user's GNOME **keyring**. They can only be set from an unlocked session (GNOME Settings), not over SSH ("Cannot create an item in a locked collection").
- Headless-daemon credentials fall back to a GKeyFile (no TPM on this host), which works over SSH.
- Desktop Sharing of a *headless remote-login* session fails when no remote-login client is attached ("Failed to record monitor: Unknown monitor"). It works while one is attached, and for physical sessions. The user daemon needs a TLS cert; Settings generates one, or use `openssl` plus `grdctl rdp set-tls-cert/key`.
- **`gnome-headless-session@.service` as shipped fails on AnduinOS 2.0.3** ("Failed: The connection is closed", exit 70). The cause is `DynamicUser=yes`. The verified fix is a drop-in: `[Service] DynamicUser=no` and `User=gdm`. With it, the session starts and the headless daemon listens.
- Remote Login greeter sessions left behind by clients that disconnect *at the greeter* stay alive (4 leftovers were seen). Drift must send a graceful shutdown when a greeter tab closes (M3-4).

### 1.10 IronRDP facts
- The crates.io releases (`ironrdp-connector 0.10.0` and friends) are **older than git HEAD** and lack `support_dyn_vc_gfx_protocol`, `with_load_balance_info`, and more. **Drift pins IronRDP git rev `b149f500b85124c513646494335fb6cee525d897`** through a Drift fork.
- The fork adds three things: (a) request RDSTLS when load-balance info is set and NLA is off (5 lines, in `ironrdp-drift.patch`); (b) Server Redirection PDU decoding (share type 0xA currently fails with "unexpected share control PDU type"); (c) an RDSTLS client exchange. Each change goes upstream as a PR (M0-2).
- Public APIs verified for reuse:
  - `ironrdp_egfx::pdu::{GfxPdu::*, CapabilitySet, …}`
  - `ironrdp_graphics::{progressive::ProgressiveDecoder, zgfx, planar, clearcodec}`
  - `ironrdp_dvc::{DvcProcessor, DrdynvcClient}`
  - `ironrdp_displaycontrol::client::DisplayControlClient`
  - `ironrdp_cliprdr::{CliprdrClient, backend::CliprdrBackend}`
  - `ironrdp_session::ActiveStage` (`encode_resize`, `encode_static(SuppressOutput)`, `process_fastpath_input`)
  - `ironrdp_blocking` and `ironrdp_tokio` connect helpers.

---

## 2. Architecture (fixed decisions)

```
┌──────────────────────────── Drift.app ─────────────────────────────────────┐
│ NSWindow per session, native tab group (tabbingIdentifier "drift.sessions")│
│ ┌──────────────────────────┐   ┌─────────────────────────────────────────┐ │
│ │ WKWebView (ui/, bun)     │   │ RemoteView: NSView + CAMetalLayer       │ │
│ │ connect/profiles, cert   │   │ key/flags/mouse/scroll capture,         │ │
│ │ prompt, reconnect overlay│   │ performKeyEquivalent claims combos      │ │
│ └──────────┬───────────────┘   └────────────┬───────────▲────────────────┘ │
│   tauri-specta IPC                          │InputEvent │present           │
│ ┌──────────▼────────────────────────────────▼───────────┴────────────────┐ │
│ │ SessionManager (drift-app) → one SessionActor per tab (tokio task)     │ │
│ └──────────┬─────────────────────────────────────────────────────────────┘ │
│ ┌──────────▼──── SessionActor ───────────────────────────────────────────┐ │
│ │ drift-rdp: TCP/TLS(pin) → NLA | RDSTLS → redirect loop → ActiveStage   │ │
│ │  ├ drift-gfx: own DvcProcessor "Microsoft::Windows::RDS::Graphics"     │ │
│ │  │   ironrdp_egfx::pdu + zgfx; caps [V8_1 AVC420, V8]; ack-after-present│ │
│ │  │   ├ AVC420 → drift-video (VideoToolbox, NV12 IOSurface)             │ │
│ │  │   └ Progressive/Planar/Uncompressed → drift-codec (ironrdp-graphics)│ │
│ │  ├ DisplayControlClient (resize/scale)                                 │ │
│ │  └ CliprdrClient ↔ drift-clipboard ↔ NSPasteboard                      │ │
│ │ drift-render: Metal compositor on a per-session render thread          │ │
│ │ drift-video::encode: VTCompressionSession → MP4 (feature "recording")  │ │
│ └────────────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────────┘
```

Decisions:
1. **RDP stack.** IronRDP at rev `b149f50` through the fork `drift/ironrdp` (branch `drift-main`), consumed as a cargo git dependency pinned by `rev`. Bump the rev only in a dedicated PR that re-runs the full e2e suite.
2. **GFX client is Drift's own** (`drift-gfx`). It reuses IronRDP's PDU codecs, ZGFX and progressive/planar decoders, but does its own surface and cache state and GPU compositing. That gives zero-copy VideoToolbox and handles Annex-B correctly. `ironrdp_egfx::client::GraphicsPipelineClient` is not used.
3. **Rendering** uses native Metal (`objc2-metal 0.3`, `objc2-quartz-core 0.3`, `objc2-core-video 0.3`). The YUV→RGB shader is **BT.709 full range**.
4. **Input capture** happens in the RemoteView NSView. By default, key events are sent as **scancodes** (positional, so shortcuts work and the remote layout decides characters). A per-profile option, **"Type using Mac layout"**, sends printable characters without Ctrl/Cmd modifiers as Unicode events, which is layout-independent (verified), plus IME/dead-key text via `NSTextInputClient insertText:`.
5. **Tabs** are native NSWindow tabbing as described in §1.8.
6. **Webview visibility.** The webview is shown only when there's no live picture (connect form, cert prompt, reconnect overlay, greeter-wait hint). Session actions live in the native menu bar.
7. **Secrets.** Passwords go in the Keychain (`security-framework`), keyed by `profile_id` + role (`rdp-system`, `rdp-user`, `linux-login`). One-time redirect credentials live only in memory. TLS pinning is TOFU on the SHA-256 of the leaf DER, shown in the same `aa:bb:…` format that `grdctl status` prints, so users can compare the two.
8. **Codec caps** are fixed at `[V8_1{AVC420_ENABLED}, V8{}]` (§1.4). AVC444 is out of scope for v1.

### Repository layout
```
drift/
├─ Cargo.toml  rust-toolchain.toml  .cargo/config.toml (xtask alias, MACOSX_DEPLOYMENT_TARGET=14.0)
├─ xtask/                 ci | e2e | bundle | bindings | host-setup-check | import-fixtures
├─ crates/
│  ├─ drift-core/         pure: profiles, SessionState FSM, ReconnectPolicy, layout policy, config model
│  ├─ drift-input/        pure: kVK→scancode map, modifier diffing, unicode routing, scroll accumulator, coords
│  ├─ drift-clipboard/    pure: format mapping/conversion, CLIPRDR sync state; `macos` feature: NSPasteboard
│  ├─ drift-gfx/          GFX DVC processor, surfaces/caches/frames, ack policy (pure + FrameSink trait)
│  ├─ drift-codec/        progressive/planar/uncompressed → BGRA tiles (wraps ironrdp-graphics)
│  ├─ drift-video/        VideoToolbox decode (Annex-B→AVCC, NV12 IOSurface) + encode + MP4
│  ├─ drift-render/       Metal compositor, shaders, offscreen golden harness
│  ├─ drift-rdp/          connect (TLS pin, NLA, RDSTLS), redirect loop, SessionActor, channel wiring
│  ├─ drift-macos/        RemoteView, cursor, occlusion, NWPathMonitor, sleep/wake, Keychain, LNP errors
│  └─ drift-testkit/      FakeServer, fixtures loader, RecordingFrameSink, ManualClock, golden utils
├─ src-tauri/             crate "drift-app": commands, SessionManager, windows/tabs, menus
├─ ui/                    bun + TypeScript (no framework) → ui/dist
├─ tests/e2e/             real-host tests (#[ignore], run by `cargo xtask e2e`)
├─ fixtures/              (git-lfs) h264/, pdus/, clipboard/, goldens/  — from §1.7
├─ host/                  drift-host-setup.sh + systemd drop-in (verified, §5.2)
└─ docs/                  adr/, acceptance.md, gnome-host-setup.md
```

---

## 3. Core contracts (land in M0-5, before parallel work)

```rust
// drift-core
pub enum ConnectMode { RemoteLogin, Headless, DesktopSharing }

pub struct ConnectionProfile {
    pub id: Uuid, pub name: String, pub host: String, pub port: u16, // RemoteLogin default 3389
    pub mode: ConnectMode,
    pub rdp_username: String,            // system creds (RemoteLogin) or daemon creds (Headless/Sharing)
    pub linux_username: Option<String>,  // RemoteLogin: shown in greeter hint only
    pub cert_pin: Option<CertFingerprint>,
    pub keyboard: KeyboardPrefs,         // cmd_as: CmdAs::{Super, Ctrl}; type_with_mac_layout: bool
    pub display: DisplayPrefs,           // adaptive: bool, retina: bool
    pub clipboard: ClipboardPrefs,       // Off | Text | TextAndImages
}

pub enum SessionState {
    Idle,
    Connecting { leg: u8, stage: ConnectStage },   // leg 1..=3
    AwaitingGreeterLogin,                           // RemoteLogin, greeter visible
    Connected { desktop: DesktopSize, scale: u32 },
    Reconnecting { attempt: u32, next_in: Duration, reason: DisconnectReason },
    Disconnected { reason: DisconnectReason },
    Failed { reason: DisconnectReason },
}

pub enum DisconnectReason {
    Network, TlsEof, ServerShutdown, Timeout,          // retryable
    AuthFailed, RdstlsFailed(u32), CertMismatch,       // not retryable
    ProtocolError(String), RedirectLoop, UserClosed, LoggedOffRemotely, LocalNetworkDenied,
}

pub enum InputEvent {
    Key { scancode: u8, extended: bool, down: bool },
    Unicode { ch: u16, down: bool },
    MouseMove { x: u16, y: u16 }, MouseButton { button: MouseButton, down: bool, x: u16, y: u16 },
    Wheel { horizontal: bool, units: i16 },   // fractional-capable: any value in -255..=255
    SyncToggles { caps: bool, num: bool }, ReleaseAll,
}

pub struct ViewGeometry { pub points: Size<f64>, pub backing_scale: f64 }

/// drift-gfx → drift-render. Called on the session render thread.
pub trait FrameSink: Send {
    fn reset(&mut self, output: Size<u32>);
    fn create_surface(&mut self, id: u16, size: Size<u32>);
    fn delete_surface(&mut self, id: u16);
    fn map_surface_to_output(&mut self, id: u16, origin: Point<u32>);
    fn blit_bgra(&mut self, id: u16, rect: Rect, stride: usize, data: &[u8]);
    fn blit_nv12(&mut self, id: u16, frame: &Nv12Frame, regions: &[Rect]);   // Nv12Frame wraps CVPixelBuffer
    fn solid_fill(&mut self, id: u16, color: Bgra, rects: &[Rect]);
    fn surface_to_surface(&mut self, src: u16, dst: u16, rect: Rect, dests: &[Point<u32>]);
    fn surface_to_cache(&mut self, id: u16, rect: Rect, slot: u16);
    fn cache_to_surface(&mut self, slot: u16, id: u16, dests: &[Point<u32>]);
    fn evict_cache(&mut self, slot: u16);
    fn end_frame(&mut self, frame_id: u32, presented: Box<dyn FnOnce() + Send>); // ack sent in `presented`
    fn set_visible(&mut self, visible: bool);
}

pub trait Clock: Send + Sync { fn now(&self) -> Instant; }
```

---

## 4. Workstreams

| Stream | Owns | Crates |
|---|---|---|
| **A — Protocol** | connect, NLA/RDSTLS, redirection, GFX state, channels, actor | `drift-rdp`, `drift-gfx`, the IronRDP fork |
| **B — Media** | codecs, VideoToolbox, Metal, encoder | `drift-codec`, `drift-video`, `drift-render` |
| **C — Platform** | NSView input, cursor, pasteboard, keychain, network/sleep/occlusion, LNP | `drift-macos`, `drift-input`, `drift-clipboard` |
| **D — App** | Tauri shell, tabs, menus, UI, SessionManager, config | `src-tauri`, `ui/`, `drift-core` |
| **E — Quality/Infra** | CI, xtask, testkit, fixtures, host setup, e2e, packaging | `xtask`, `drift-testkit`, `host/`, `tests/e2e` |

```
M0 ─→ M1 ─┬─→ M2 input ───────────┐
          ├─→ M3 remote login ────┤
          ├─→ M4 adaptive/retina ─┼─→ M6 tabs ─→ M7 auto-reconnect ─→ M9 ship
          ├─→ M5 clipboard ───────┘
          └─→ M8 H.264 encoder ─────────────────────────────────────────↗
```

---

## 5. Environment

### 5.1 Dev machine
```bash
rustup show                                        # rust-toolchain.toml (tested with 1.97.1)
cargo install tauri-cli --version "^2" --locked
cargo install cargo-nextest cargo-llvm-cov cargo-deny cargo-fuzz --locked
brew install bun git-lfs ffmpeg                    # ffmpeg only for fixture tooling
(cd ui && bun install)
cargo xtask ci
cargo tauri dev
```
- Sign dev builds with a stable "Apple Development" identity (`APPLE_SIGNING_IDENTITY`, used by `cargo tauri dev` via `tauri.conf.json > bundle > macOS > signingIdentity`). The Local Network permission then persists across rebuilds (§1.8).

### 5.2 GNOME 50 test host (`homelab@10.1.2.40`, already configured)
State after the spike:

| Item | Value |
|---|---|
| Remote Login | system daemon :3389 (system RDP creds are set by the host owner; fetch with `sudo grdctl --system status --show-credentials`) |
| Test user 1 | `drifttest`: Linux login; also has a headless daemon on :3391 |
| Test user 2 | `drifttest2`: persistent headless session (`gnome-headless-session@drifttest2.service`, drop-in applied), headless daemon :3392 |
| Drop-in | `/etc/systemd/system/gnome-headless-session@.service.d/drift-test.conf` → `DynamicUser=no`, `User=gdm` |
| Helpers | `/home/drifttest/{cliptool.py,anim.py,clip_in.png}` (GTK4 clipboard helper, full-screen animation) |
| Secrets (dev Mac only) | `~/code/drift-spikes/secrets/` (mode 600). The e2e suite reads the same data from env vars. |

`host/drift-host-setup.sh` (task M0-4) turns these verified steps into an idempotent script:
```bash
# Remote Login (system daemon)
sudo grdctl --system rdp set-tls-key  <key.pem>
sudo grdctl --system rdp set-tls-cert <cert.pem>
sudo grdctl --system rdp set-credentials <sys-user> <sys-pass>
sudo grdctl --system rdp enable && sudo systemctl enable --now gnome-remote-desktop.service

# Headless persistent session for <user>
sudo install -D -m644 host/gnome-headless-session-dropin.conf \
     /etc/systemd/system/gnome-headless-session@.service.d/drift.conf   # DynamicUser=no, User=gdm
sudo systemctl daemon-reload && sudo systemctl enable --now gnome-headless-session@<user>.service
sudo -u <user> env XDG_RUNTIME_DIR=/run/user/$UID DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$UID/bus sh -c '
  grdctl --headless rdp set-tls-cert ~/.local/share/gnome-remote-desktop/certificates/rdp-tls.crt
  grdctl --headless rdp set-tls-key  ~/.local/share/gnome-remote-desktop/certificates/rdp-tls.key
  grdctl --headless rdp set-credentials <rdp-user> <rdp-pass>
  grdctl --headless rdp set-port <fixed-port> && grdctl --headless rdp disable-port-negotiation
  grdctl --headless rdp disable-view-only && grdctl --headless rdp enable
  systemctl --user enable --now gnome-remote-desktop-headless.service'

# Desktop Sharing: done by the user in GNOME Settings → System → Remote Desktop → Desktop Sharing
# (credentials live in the login keyring; cannot be set over SSH).
```

### 5.3 E2E harness
- `cargo xtask e2e` opens `ssh -N -L 1339x:localhost:339x homelab@10.1.2.40` for every daemon port. Tests connect to `127.0.0.1:1339x` using TLS server name `10.1.2.40`. This is verified, and required because of Local Network Privacy.
- Credentials come from env vars: `DRIFT_E2E_SYS_USER/PASS`, `DRIFT_E2E_LOGIN_USER/PASS` (`drifttest`), and `DRIFT_E2E_HL_USER/PASS` plus `DRIFT_E2E_HL_PORT` (`drifttest2`). A `redact` layer strips all of them from logs; a test asserts that.
- Remote-state helpers run over the same SSH connection:
  - launch `anim.py` for motion;
  - launch `cliptool.py` or GNOME Text Editor for clipboard tests;
  - `loginctl` to assert session reuse.
- Scripted GDM login (Remote Login tests only): click the user tile or pick "Not listed?", type the password as Unicode events, press Enter. The ported `probe2` step runner provides this as `drift-testkit::e2e::Script`.
- CI: GitHub Actions `macos-15` arm64 runs `cargo xtask ci`. Nightly e2e runs on a self-hosted macOS runner with SSH access to the host.

---

## 6. Milestones & tasks

**Red** = tests to write first · **Green** = implementation · **Done** = acceptance.

### M0 — Foundation
- [ ] **M0-1 Workspace scaffold** (E). Workspace, toolchain, lints, the crates from §2, `ui/` via `bun init`, `src-tauri` via `cargo tauri init` with the npm references removed, and the profile settings from §0.
  **Red:** an xtask test that fails if there are any npm/yarn/pnpm lockfiles or `npm`/`npx` invocations in scripts, workflows or docs.
  **Done:** `cargo xtask ci` is green and `cargo tauri dev` opens a window.
- [x] **M0-2 IronRDP fork** (A). Fork at `b149f50` with branch `drift-main`. Apply `ironrdp-drift.patch` (RDSTLS request), then port three things from `probe2` into IronRDP style: `ServerRedirectionPdu` decode in `ironrdp-pdu` (share type 0xA), a `ActiveStageOutput::ServerRedirect` surfaced by `ironrdp-session`, and an `rdstls` client module in `ironrdp-connector`. Fix the `CB_TEMP_DIRECTORY` length. Open an upstream PR for each change.
  **Red:**
  - decode tests with the captured redirection PDU (sanitized), checking flags `0x1C016`, LB `Cookie: msts=…\r\n`, a 16-char user, a 34-byte password, a 50-byte GUID and the cert container;
  - RDSTLS encode tests against byte vectors from §1.3;
  - an AuthResponse `0x52E` decode test;
  - a `CB_TEMP_DIRECTORY` round-trip test.

  **Done:** Drift's workspace depends on the fork by `rev`, and the fork's own tests pass.
- [x] **M0-3 Fixtures import** (E). `cargo xtask import-fixtures` copies the §1.7 artefacts from `~/code/drift-spikes` into `fixtures/` (LFS), runs the sanitizer, and writes `fixtures/README.md` with provenance.
  **Red:**
  - a sanitizer test showing no known secret appears as bytes, UTF-8 or UTF-16LE;
  - a fixture-manifest checksum test.
- [x] **M0-4 Host setup script** (E). Write `host/drift-host-setup.sh` and the drop-in from §5.2, plus `cargo xtask host-setup-check`, which verifies ports, daemons, headless session and certs over SSH.
  **Red:** a shellcheck clean run; a `host-setup-check` unit test on parsed `grdctl status`/`ss -ltnp` samples captured from the host.
  **Done:** running the script on the homelab changes nothing (idempotent), and `host-setup-check` is green.
- [ ] **M0-5 Core contracts** (D, reviewed by all). The §3 types, plus `RecordingFrameSink` and `ManualClock` in `drift-testkit`.
  **Red:** a TOML round-trip test for `ConnectionProfile`; a `SessionState` transition table test (invalid transitions return `Err`); an exhaustive `DisconnectReason::is_retryable` table test.
- [x] **M0-6 CI** (E). Workflow running `cargo xtask ci` with caches, and `cargo-deny` (MIT/Apache/BSD/ISC/Zlib/Unicode allowed; GPL denied).
  **Red:** a canary branch with a failing test turns CI red.

### M1 — First light (Headless mode first: the simplest verified path)
- [x] **M1-1 Connect** (A). `drift-rdp::connect`: TCP, then TLS with a custom rustls verifier (TOFU/pin on the leaf SHA-256), then NLA, then capabilities with `support_dyn_vc_gfx_protocol=true`, `platform=MACINTOSH`, and `client_name` = the host name. Map errno 65 to `DisconnectReason::LocalNetworkDenied`.
  **Red:** loopback tests with `FakeServer` for:
  - success;
  - wrong password → `AuthFailed`;
  - pin mismatch → `CertMismatch`;
  - unknown cert → `CertificatePrompt` event, then wait;
  - timeout via `ManualClock`;
  - an injected `EHOSTUNREACH` → `LocalNetworkDenied`.

  **Done:** e2e `e2e_headless_connects` (:3392) reaches `Connected{1280x800}`.
- [x] **M1-2 GFX state machine** (A). `drift-gfx` implements `DvcProcessor` for `Microsoft::Windows::RDS::Graphics`:
  - caps `[V8_1{AVC420_ENABLED}, V8{}]` exactly;
  - RDP_SEGMENTED_DATA + ZGFX decompression;
  - dispatch of every `GfxPdu` to `FrameSink` and codecs;
  - ack policy: `FrameAcknowledge` is sent from the `presented` callback, `queueDepth` is reported honestly, and while hidden the client sends `SUSPEND_FRAME_ACKNOWLEDGEMENT` and relies on Suppress Output.

  **Red:**
  - a caps-advertise byte test that must equal the verified set;
  - replaying fixture PDU streams into `RecordingFrameSink` with an `insta` snapshot;
  - ResetGraphics followed by a new surface id tears down the old surface;
  - an unknown codec gives a `ProtocolError`, not a panic;
  - a `cargo fuzz` target for GfxPdu+ZGFX (5 minutes nightly).
- [x] **M1-3 AVC420 via VideoToolbox** (B). `drift-video::decode`:
  - parse the `RFX_AVC420_METABLOCK` (region rects + quant/quality);
  - convert **Annex-B → AVCC**, dropping AUD NALs;
  - build `CMVideoFormatDescription` from SPS/PPS and rebuild it when they change;
  - run `VTDecompressionSession` with `RealTime` and `EnableLowLatency`, outputting `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` backed by an IOSurface.

  **Red:**
  - Annex-B splitter proptests (all start-code lengths, trailing zeros);
  - metablock parse tests on fixture bytes;
  - decoding `fixtures/h264/leg3.h264` gives 398 frames, and frame N matches an ffmpeg-decoded golden with PSNR ≥ 40 dB;
  - an SPS change triggers a session rebuild.
- [x] **M1-4 CPU codecs** (B). `drift-codec` wraps `ironrdp_graphics::{progressive, planar}` plus uncompressed into BGRA tiles, with a rayon tile pool.
  **Red:**
  - a greeter progressive replay (captured with caps `v81noavc`) whose final image matches `fixtures/goldens/greeter.png` at PSNR ≥ 45 dB;
  - a malformed-input proptest that never panics;
  - a criterion bench: a 64×64 progressive tile in < 100 µs (release build).
- [x] **M1-5 Metal compositor** (B). `drift-render` implements `FrameSink`:
  - one BGRA8 `MTLTexture` per surface;
  - an NV12→RGB **BT.709 full-range** fragment shader that writes only the region rects;
  - blit encoders for surface-to-surface and cache operations;
  - presentation to `CAMetalLayer` on a per-session render thread;
  - `presented` fires from the command buffer completion handler.

  **Red:**
  - goldens for solid fill, BGRA blit at an offset, overlapping surface-to-surface, cache round-trip, region-limited NV12 (pixels outside the regions stay untouched) and reset;
  - a colour-accuracy test: known RGB → g-r-d's integer encoder formula (§1.4) → our shader → RGB within ±2;
  - `presented` is called exactly once per frame.
- [x] **M1-6 RemoteView + UI** (C + D). `drift-macos::RemoteView` as in §1.8: layer-hosting, drawable size tracking, first responder, and hiding the webview on `Connected`. Minimal UI: a profile form (mode, host, port, user, password) and a certificate prompt that shows the fingerprint in grdctl format.
  **Red:**
  - `drift-core::profile::validate` table tests;
  - `bun test` for form rendering from state;
  - a bindings-staleness check;
  - a fingerprint formatting test that matches `grdctl status` output (`f3:e7:a2:…`).

  **Done (manual M1):** Headless-mode connect to drifttest2 shows a live, correctly coloured desktop. `anim.py` shows ≥ 55 fps in the stats overlay.

### M2 — Keyboard, mouse, trackpad
- [x] **M2-1 Keymap & modifiers** (C). Map `kVK_*` to set-1 scancodes plus the extended flag, for ANSI, ISO and JIS keyboards. Diff `flagsChanged` into left/right modifier transitions. `CmdAs::Super` (`0x5B` extended) is the default; `CmdAs::Ctrl` sends `0x1D`.
  **Red:**
  - a table over every `kVK_*` constant;
  - a proptest that down/up events stay balanced for any flagsChanged sequence;
  - focus loss produces `ReleaseAll` for exactly the keys currently held;
  - Caps Lock produces `SyncToggles`.
- [ ] **M2-2 Unicode typing mode** (C). With `type_with_mac_layout`, printable characters without Ctrl/Cmd modifiers, plus `NSTextInputClient` text (dead keys, IME), go out as Unicode down/up pairs. Chords always use scancodes.
  **Red:**
  - routing table tests covering `é` via dead key, `ß`, `Ctrl+C`, `Cmd+V` and emoji (surrogate pairs sent as two UTF-16 units);
  - e2e `e2e_unicode_typing`: type `Grüße ✓` into gedit and read it back through the clipboard (M5).
- [ ] **M2-3 Pointer & scroll** (C). `view_to_desktop` covers 1:1, fit and letterbox, and Retina. The scroll accumulator converts precise deltas into **fractional wheel units** (default 1 point = 2 units, configurable) and clamps each event to ±255. Line-based mouse wheels send ±120 per line. Natural-scrolling direction is respected.
  **Red:**
  - mapping table tests;
  - a proptest that emitted units equal accumulated units with no drift;
  - a clamp/split test;
  - e2e `e2e_scroll`: `seq 1 400` in a terminal, 5×(+120) scrolls 15–17 lines up (the verified 16), and 30×(+12) matches 3×(+120);
  - e2e `e2e_hscroll`, which establishes and locks in the HWHEEL sign.
- [ ] **M2-4 Shortcut routing** (C + D). `performKeyEquivalent:` returns YES for every combo except an allow-list (Cmd+T, Cmd+W, Cmd+Q, Cmd+1…9, Cmd+Shift+[ ], Cmd+`), which goes to the menu. Add a "Send Ctrl+Alt+Del" menu item.
  **Red:**
  - allow-list tests;
  - a loopback test in which `FakeServer` receives the exact fast-path sequence for a scripted session, and never sees an allow-listed combo.
- [x] **M2-5 Remote cursor** (C). Decode fast-path pointer updates (color/large/new/cached/null/position) into an `NSCursor` with the hotspot. At scale 200 the server already sends 2× bitmaps (86×86), so the image size is `bitmap/scale` in points.
  **Red:** decode tests on fixture pointer PDUs (43×43 and 86×86); cache eviction; a points-size computation test.
  **Done (manual M2):** typing, shortcuts, click, drag, right-click, smooth two-finger scrolling in Files and Firefox, and cursor shape changes all work.

### M3 — Remote Login (GDM)
- [x] **M3-1 Redirect loop** (A). The actor handles `ServerRedirect` from the fork. It closes the transport, then reconnects to the same host and port (or `TargetNetAddress` if present) with the routing token and protocols `SSL|RDSTLS`. After TLS it verifies **the leaf DER equals the target certificate from the container**, runs RDSTLS with the one-time credentials, and continues. Credentials are zeroized after use. A cap of 4 redirects per connection attempt yields `RedirectLoop`. The state goes `Connecting{leg}` → `AwaitingGreeterLogin` when leg 2 activates → `Connected` when leg 3 activates.
  **Red:**
  - FSM tests covering leg sequencing and loop protection;
  - a target-cert mismatch gives `CertMismatch`;
  - a loopback test with two `FakeServer`s (redirect, then RDSTLS);
  - RDSTLS `0x52E` gives `RdstlsFailed(0x52E)` with no retry;
  - e2e `e2e_remote_login`: greeter, then scripted login as drifttest, then desktop.
- [ ] **M3-2 Credentials UX** (D + C). A Remote Login profile stores the **system RDP credentials**. The Linux password is entered by the user in the greeter, with an optional Keychain-stored "Linux password" that Drift types into the focused greeter password field only after an explicit per-profile opt-in. Headless and Sharing profiles store one set of RDP credentials.
  **Red:** Keychain adapter tests using a test-only service name; UI state tests showing the mode switches the visible fields.
- [x] **M3-3 Session reuse** (A).
  **Red:**
  - e2e `e2e_login_reuses_session`: `loginctl` shows the same session id before and after disconnect → greeter → login;
  - e2e `e2e_takeover_stale`: the first client is `SIGSTOP`ed and a second client still reaches the same session.
- [x] **M3-4 Greeter hygiene** (A). Closing a tab while in `AwaitingGreeterLogin` sends a graceful Shutdown Request and disconnect, so no greeter session is left behind.
  **Red:** e2e `e2e_no_greeter_leak`: the `loginctl` greeter count is unchanged after 3 open/close cycles.
  **Done (manual M3):** a Remote Login profile goes greeter → desktop; disconnect → reconnect → greeter → same desktop.

### M4 — Adaptive size & Retina
- [x] **M4-1 Layout policy** (D). `desired_layout(ViewGeometry, DisplayPrefs, caps) -> MonitorLayout`. Retina on means physical pixels (points × 2) with `DesktopScaleFactor=200`; off means points with 100. Width is even, dimensions are clamped to [200, 8192] and to the server's max area. `DeviceScaleFactor` is picked from {100, 140, 180}.
  **Red:** table tests (MBA 13" at 2×, external 1×, odd sizes such as 1281×801 → 1280×801, tiny and 6K); a proptest that the result satisfies MS-RDPEDISP.
- [x] **M4-2 Resize driver** (A). A 250 ms trailing debounce on `setFrameSize`/`viewDidChangeBackingProperties` sends DISP only when the layout changes. It handles ResetGraphics plus the new surface. In **DesktopSharing** mode (no DISP channel) it goes straight to `ScaleMode::Fit`, with no timeout guessing.
  **Red:**
  - `ManualClock` debounce tests (a burst gives 1 PDU; no change gives 0);
  - a loopback reset test;
  - a mode-based Fit selection test;
  - e2e `e2e_resize` (1600×1000@100, then 2560×1600@200, then 1281×801, each producing a matching ResetGraphics).
- [x] **M4-3 Crisp present** (B). When the drawable equals the desktop, sampling is nearest and bit-exact; otherwise linear with an aspect-preserving letterbox.
  **Red:** a bit-exact 1:1 golden; a scaled golden; a letterbox colour golden.
  **Done (manual M4):** a window resize reflows GNOME in under 0.5 s, text is pixel-sharp on Retina, and moving to a 1× display re-layouts.

### M5 — Clipboard (text + images, both directions)
- [x] **M5-1 Format mapping** (C). The pure mapping, per §1.6:

  | Source | Local ↔ remote |
  |---|---|
  | Text | `public.utf8-plain-text` ↔ `CF_UNICODETEXT` (UTF-16LE with NUL, LF↔CRLF), accepting `CF_TEXT` only when Unicode is absent |
  | Images | `public.png` ↔ named `"image/png"`; `public.tiff` ↔ `CF_TIFF`; `CF_DIB` → PNG decoded via the `image` crate |

  Outbound, Drift advertises `CF_UNICODETEXT` + `"image/png"` (+ `CF_DIB` for compatibility).
  **Red:**
  - text round-trip proptests (surrogates, CRLF);
  - DIB decode fixtures (24/32 bpp, top-down and bottom-up);
  - the PNG fixture passes through unchanged;
  - a size cap (32 MiB) gives a rejection event.
- [x] **M5-2 CLIPRDR sync** (A). A `CliprdrBackend` that **answers the initial format-list request immediately** (the current local list, or empty). On a remote copy it eagerly fetches the preferred format, then writes the pasteboard. Loop prevention tracks our own `changeCount` writes and echoed lists. Only the focused tab syncs.
  **Red:**
  - a state test showing a missing initial list response gives no ready state, and with the fix, ready;
  - an echo-loop test;
  - a newest-wins race test with `ManualClock`;
  - e2e `e2e_clip_remote_text` (Text Editor Ctrl+A/C → `"copy-me-…"`);
  - e2e `e2e_clip_remote_image` (Print Screen → Enter → PNG);
  - e2e `e2e_clip_local_text_image` (advertise, then `cliptool.py read` in the session, which reports the text and 320×200).
- [x] **M5-3 NSPasteboard adapter** (C). Poll `changeCount` every 250 ms on the main thread; read and write text, PNG and TIFF; per-profile Off / Text / Text+Images.
  **Red:** a smoke test on `NSPasteboard pasteboardWithUniqueName`; a focus-scoping test with fakes.

### M6 — Multi-session tabs
- [x] **M6-1 SessionManager** (D). Maps sessions to windows, routes events, closes cleanly, and shuts down gracefully on quit (2 s cap).
  **Red:** fake-actor tests (open ×3, close the middle one, events reach only their own window); quit completes within the cap; no leaked handles.
- [x] **M6-2 Native tab group** (D + C). Each session window is created hidden, then `setTabbingMode(Preferred)` + `addTabbedWindow:ordered:` join it to the group. `newWindowForTab:` is installed on the `TaoWindow` class and routes to "new tab". Menu items: Cmd+T, Cmd+W, Cmd+1…9. Tab title is the profile name plus a state glyph.
  **Red:** a title formatter test; a window-group integration test (spike approach: open 3 windows, assert `tabbedWindows.count == 3`) run under `cargo test -p drift-app --features macos-ui-tests`.
- [x] **M6-3 Background throttling** (A + B + C). On an occlusion notification (non-visible), send **Suppress Output (allow=0)**, stop the render thread, and suspend acks. On visible, send allow with the full rect; the server sends a full frame (verified), so no Refresh Rect is needed.
  **Red:**
  - a loopback PDU-order test;
  - a renderer paused → zero presents test;
  - e2e `e2e_suppress_resume`: zero frames while suppressed, and after allow the frame shows the post-change desktop.

  **Done (manual M6):** 3 sessions (Remote Login + Headless + Sharing) in one tab group, instant switching, and each background tab at < 2 % CPU.

### M7 — Auto-reconnect with backoff
- [x] **M7-1 Policy** (D). A pure `ReconnectPolicy` with exponential full-jitter backoff: base 500 ms, ×2, capped at 30 s. It's unlimited while the network is reachable, with a user-visible cap (default 20). The attempt count resets after 60 s stable. Retryability comes from `DisconnectReason` (§3).
  **Red:** seeded-RNG delay tables; a bounds proptest; an exhaustive classification test; reset-after-stable.
- [x] **M7-2 Triggers** (C). An `NWPathMonitor` wrapper and `NSWorkspace.didWakeNotification` feed a pure `TriggerMerger`: offline pauses, online retries immediately, wake retries immediately, and duplicates are debounced.
  **Red:** `TriggerMerger` tables.
- [x] **M7-3 Mode-specific resume** (A + D).
  - **Headless and DesktopSharing** reconnect straight into the session (verified), keeping the last frame dimmed under an overlay: "Reconnecting in N s… [Now] [Cancel]".
  - **RemoteLogin** reconnects the full chain and stops at the greeter in `AwaitingGreeterLogin`, with the overlay "Session is still running — log in to resume". With the M3-2 opt-in, Drift types the stored Linux password into the greeter field. After login the same session resumes (verified).
  - After any reconnect Drift sends `ReleaseAll` plus `SyncToggles`.

  **Red:**
  - a loopback kill-socket test (exact state sequence);
  - an auth failure during reconnect gives `Failed` with no loop;
  - cancel stops the timers;
  - e2e `e2e_reconnect_headless`: kill the SSH forward, restore it, and the same session returns with no greeter;
  - e2e `e2e_reconnect_remote_login`: reaches the greeter, then login gives the same `loginctl` session.

  **Done (manual M7):** Wi-Fi off/on and sleep/wake reconnect automatically; `systemctl restart gnome-remote-desktop` on the host → reconnect.

### M8 — H.264 encoder (future session recording)
- [x] **M8-1 Composite capture** (B). Render the composite into an IOSurface-backed BGRA texture from a `CVPixelBufferPool` (zero copy); only enabled while recording.
  **Red:** the captured buffer equals the on-screen composite (golden); the pool doesn't grow over 1 000 frames.
- [x] **M8-2 Encoder** (B). `VTCompressionSession`: hardware, High profile, real-time, no reordering, 8 Mbit/s, 2 s GOP, variable frame rate on `Clock` timestamps.
  **Red:** encode 120 synthetic frames, decode them back with `drift-video::decode`, and require PSNR ≥ 35 dB; SPS says High; PTS monotonic; keyframe interval honoured; a resize rebuilds the encoder and emits a keyframe.
- [x] **M8-3 MP4 + hook** (B + D). `AVAssetWriter` in passthrough mode. `StartRecording`/`StopRecording` go behind the `recording` cargo feature and the hidden menu item "Debug ▸ Record Session (experimental)".
  **Red:** `AVAsset` reads back the right duration, track and dimensions; stop without start gives an error; a disk-full injection stops gracefully with an event.

### M9 — Hardening & ship
- [ ] **M9-1 Performance** (B). Budgets on an M1 base model:
  - **60 fps at 1280×800** and ≥ 55 fps at 2560×1600 with `anim.py`;
  - decode + present < 8 ms p95;
  - input-to-wire < 2 ms p99;
  - ≤ 300 MB RSS per session.

  `cargo xtask e2e --bench` reports these; a regression of more than 10 % fails the nightly run.
- [ ] **M9-2 Robustness** (A). Nightly fuzzing of the GFX, ZGFX, redirection, RDSTLS, CLIPRDR and pointer parsers. Malformed server input never panics; the actor boundary maps it to `ProtocolError`.
- [ ] **M9-3 Security** (E).
  - Credentials never appear in logs (a redaction test on e2e logs).
  - One-time redirect credentials are zeroized.
  - TOFU pins cover both the system daemon and the redirect target.
  - App Sandbox (`network.client`), Hardened Runtime, and `NSLocalNetworkUsageDescription`.
  - The errno 65 screen links to System Settings › Privacy & Security › Local Network.
- [ ] **M9-4 Polish & accessibility** (D). VoiceOver labels; mode-specific errors with next steps (for example "Desktop Sharing credentials must be set in GNOME Settings on the host", or "Headless session not running — see host/drift-host-setup.sh").
- [ ] **M9-5 Packaging** (E). `cargo xtask bundle` builds a universal .app and .dmg, with Developer ID signing, notarytool, and the Tauri updater with a signed manifest.
  **Done:** clean macOS 14 and macOS 27 machines install, pass Gatekeeper, prompt for Local Network once, and pass `docs/acceptance.md` in full against the homelab.

---

## 7. Feature → task traceability
| Feature | Tasks | Verified basis |
|---|---|---|
| 1. Full GDM/RDP connection | M1-1, M3-1…4 | §1.3 |
| 2. GPU-rendered frames | M1-2…5, M4-3 | §1.4, §1.8 |
| 3. Keyboard, mouse, trackpad scroll | M2-1…5 | §1.5, §1.8 |
| 4. Multi-session tabs | M6-1…3 | §1.8, §1.4 (suppress) |
| 5. Auto-reconnect with backoff | M7-1…3 | §1.2, §1.3 (takeover, reuse) |
| 6. H.264 encoder | M8-1…3 | §1.8 (VT encoder) |
| 7. Clipboard sync (text + images) | M5-1…3 | §1.6 |
| 8. Adaptive screen size | M4-1, M4-2 | §1.4 (DISP) |
| 9. Retina support | M4-1, M4-3, M2-5 | §1.4 (scale 200, 2× cursors) |

## 8. Known product constraints (by design, verified; not risks)
- **Remote Login reconnects always pass through the GDM greeter** (GDM 50.1 has no credential-passing API). The session itself persists and is reused. Users who want reconnects with no interaction use **Headless** mode.
- **Desktop Sharing can't resize the remote display** (no DISP channel), so Drift scales to fit.
- Desktop Sharing credentials must be set in GNOME Settings on the host (keyring).
- Headless sessions on AnduinOS 2.0.3 need the systemd drop-in in `host/`.
- The character produced by scancode typing follows the *remote* keyboard layout; "Type using Mac layout" switches printable keys to Unicode.

## 9. Out of scope (v1)
Audio (RDPSND/AUDIN), camera, drive/printer/smartcard redirection, file clipboard, multi-monitor, AVC444, relative mouse / pointer lock, system-wide keyboard grab, VNC, GNOME < 50, non-macOS platforms.

## 10. Definition of Done (every task)
- The Red tests existed and failed first (proof is in the PR); everything is green now.
- `cargo xtask ci` passes: fmt, clippy `-D warnings`, nextest, coverage gate, deny, `bun test`, `bun run build`, bindings fresh.
- No `unwrap`/`expect` on network data; every `unsafe` block has a `// SAFETY:` comment.
- The affected e2e tests pass against the homelab (for M1+ tasks that touch the protocol).
- Public items are documented, an ADR exists if a new decision was made, and the checkbox here is ticked.
- No npm artefacts and no non-macOS code.
