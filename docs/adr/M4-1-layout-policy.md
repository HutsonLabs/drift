# M4-1 — Monitor layout policy details

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-core/src/layout.rs`

## Context

Plan M4-1 fixes the shape of `desired_layout(ViewGeometry, DisplayPrefs, caps) -> MonitorLayout`
(Retina → points × 2 at `DesktopScaleFactor=200`, otherwise points at 100; even width; clamp to
`[200, 8192]` and the server's max area; `DeviceScaleFactor` from {100, 140, 180}) but leaves a few
details open.

## Decision

- **Retina threshold:** the view counts as Retina when `backing_scale ≥ 1.5` *and* the profile's
  `retina` preference is on. The multiplier is exactly 2 (macOS only has 1× and 2× backing
  stores), so the requested size equals the drawable and M4-3 can present 1:1.
- **`DeviceScaleFactor`:** the valid value nearest to the desktop scale (ties go low):
  100 → 100, 200 → 180.
- **Rounding:** points × factor are rounded to the nearest pixel, then clamped, then the width is
  rounded *down* to even (1281 → 1280, matching what g-r-d does itself).
- **Max area:** `MaxMonitorAreaFactorA × MaxMonitorAreaFactorB × MaxNumMonitors`
  (saturating). If exceeded, both sides are scaled by `sqrt(max/area)` (aspect kept), then
  shrunk pixel by pixel until the area fits. If a server's max area is below 200 × 200 the
  minimum size is returned (MS-RDPEDISP forbids smaller; such a server cannot be satisfied).
- **`adaptive` is not consulted** by `desired_layout`; whether to send a layout at all
  (adaptive off, Desktop Sharing without DISP) is the resize driver's decision (M4-2).
- Physical size and orientation are not part of `MonitorLayout`; the DISP encoder (M4-2) sends
  physical size 0 (ignored by MS-RDPEDISP) and orientation 0.

## Consequences

A proptest checks every result against an independent MS-RDPEDISP validator for arbitrary (also
NaN/negative/huge) geometry and server caps.
