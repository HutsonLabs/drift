# M5-1 / M5-3 — Clipboard format mapping, sync state and NSPasteboard adapter

- Status: accepted
- Date: 2026-09-22
- Code: `crates/drift-clipboard/src/{formats,sync,poll,pasteboard}.rs`

## Context

Plan §1.6 and M5-1/M5-2/M5-3 fix the wire facts (answer the initial format-list request,
`"image/png"` named format, `CF_UNICODETEXT` UTF-16LE+NUL, CF_DIB/CF_TIFF mappings, 32 MiB cap)
but leave several implementation choices open. The IronRDP `CliprdrBackend` glue (M5-2) lives in
`drift-rdp`; this crate must expose everything it needs as pure, testable logic.

## Decisions

1. **Format ids.** `drift-clipboard` works with raw `u32` ids and optional names
   (`ClipFormat { id, name }`) and does not depend on `ironrdp-cliprdr`; `drift-rdp` maps to and
   from `ironrdp_cliprdr::pdu::ClipboardFormat`. Drift registers its outbound PNG as
   `LOCAL_PNG_FORMAT_ID = 0xC0F0` named `"image/png"` (the id verified in the spike). Inbound
   PNG is recognised **by name only** (g-r-d uses `0xD011`; registered ids are per side).
2. **Outbound list.** Text → `CF_UNICODETEXT`; PNG → `"image/png"` + `CF_DIB` (32 bpp BI_RGB,
   bottom-up, encoded from the PNG on demand); a TIFF-only clipboard → `CF_TIFF`. `Off` → empty
   list; `Text` → text only.
3. **Inbound selection.** At most one text format (`CF_UNICODETEXT`, else `CF_TEXT`) and, when
   images are allowed, at most one image format (`"image/png"` > `CF_TIFF` > `CF_DIB`), fetched
   sequentially (IronRDP allows one Format Data Request in flight and its response carries no
   format id, so `ClipboardSync` tracks the in-flight request). `CF_TEXT` is decoded as UTF-8
   when valid, else Latin-1. `CF_DIB` is decoded with the `image` crate's BMP decoder
   (`new_without_file_header`) and re-encoded as PNG; the decoded size is bounded from the
   header (w×h×4 ≤ 32 MiB) before any allocation.
4. **Dependency.** `image = 0.25` with `default-features = false, features = ["png", "bmp"]`
   (all MIT/Apache/Zlib/BSD; `cargo deny` clean).
5. **Size cap.** 32 MiB (`MAX_CLIPBOARD_BYTES`) on every inbound payload, every outbound item
   and every encoded response. Violations surface as `SyncAction::Rejected(ClipError::TooLarge)`
   (the "rejection event"); the actor forwards it to the UI/log.
6. **Sync state (`ClipboardSync`)**, one per session, driven by `SyncInput` → `Vec<SyncAction>`:
   - Initial format-list request is always answered: local list when focused and allowed,
     otherwise empty. Nothing is advertised before that (`is_ready()`).
   - Echo prevention: `changeCount`s of our own writes (fed back via `LocalWritten`) and the
     contents of a not-yet-acknowledged write are ignored; a remote list whose kinds equal our
     last advertisement within `ECHO_WINDOW = 500 ms` is ignored.
   - Newest wins (timestamps from the injected `Clock`): a local change supersedes an in-flight
     remote fetch (its response is dropped); a new remote copy supersedes our advertisement and
     any in-flight fetch; a completed fetch never overwrites a newer local change.
   - A fetch not answered within `FETCH_TIMEOUT = 5 s` is abandoned on `Tick`.
   - Focus scoping: unfocused sessions ignore remote copies and defer local changes, which they
     advertise on regaining focus. Format Data Requests are answered regardless of focus from
     the last advertised contents.
7. **Polling (M5-3).** `PasteboardWatcher` (pure) + `PasteboardPort` trait. The app polls every
   `POLL_INTERVAL = 250 ms` from a main-thread timer, reading at the most permissive level any
   session needs, and fans each `LocalChange` out to all sessions; each session ignores its own
   writes. A new session is seeded with `PasteboardWatcher::snapshot` as a `LocalChanged`.
8. **NSPasteboard adapter.** `NsPasteboard` (humble object): text via
   `NSPasteboardTypeString`; images via `NSPasteboardTypePNG`/`NSPasteboardTypeTIFF`. A
   TIFF-only pasteboard is converted to PNG with `NSBitmapImageRep` (so Drift can always offer
   `"image/png"`); a PNG write also offers TIFF for older Mac apps.
9. **`macos` feature on by default.** The workspace build (clippy, nextest, coverage) otherwise
   never compiles or smoke-tests the adapter, because nothing enables the feature yet. Pure-only
   consumers can use `default-features = false`.
10. **Fixtures** live in `crates/drift-clipboard/tests/fixtures/` (tiny, crate-private, not
    git-lfs): the captured g-r-d `0xD011` PNG and `CF_UNICODETEXT` sample, and synthetic 3×2 DIBs.

## Consequences

- M5-2 glue in `drift-rdp`: `on_request_format_list` → `InitialFormatListRequested`;
  `on_remote_copy` → `RemoteFormatList`; `on_format_data_response` → `RemoteData`;
  `on_format_data_request` → `RemoteDataRequest`; execute `SendFormatList` with
  `initiate_copy`, `RequestRemoteData` with `initiate_paste`, `SendData` with
  `submit_format_data` (`Err` → `OwnedFormatDataResponse::new_error()`), `WritePasteboard` via
  the main thread then feed back `LocalWritten`; forward `Rejected`; send `Tick` about once a
  second.
- A late response to a timed-out request could be attributed to a newer request of a different
  format; decoding then fails and is reported as a rejection (no crash, no wrong-type write for
  images because PNG/DIB are validated).
