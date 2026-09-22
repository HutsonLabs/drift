//! # drift-macos
//!
//! Humble-object FFI glue for AppKit and system services (plan §2):
//!
//! | Area | Task |
//! |---|---|
//! | `RemoteView` (layer-hosting NSView + CAMetalLayer, input capture) | M1-6, M2-* |
//! | Remote cursor (`NSCursor`) | M2-5 |
//! | Keychain adapter | M3-2 |
//! | NSPasteboard polling | M5-3 |
//! | Native tabs, occlusion | M6-2, M6-3 |
//! | `NWPathMonitor`, sleep/wake | M7-2 |
//! | Local Network Privacy (errno 65) UX | M1-1, M9-3 |
//!
//! Decision logic lives in pure crates (`drift-input`, `drift-core`); this crate only
//! translates between AppKit and those crates. Every `unsafe` block has a `// SAFETY:`.
