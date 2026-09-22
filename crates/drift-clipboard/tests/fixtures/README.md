# drift-clipboard test fixtures

Small, non-sensitive clipboard payloads used by the M5-1/M5-3 tests (kept in the crate, not in
the git-lfs `fixtures/` tree, because they are tiny and only this crate reads them).

| File | Origin | Content |
|---|---|---|
| `remote_clip_d011.png` | Captured from g-r-d 50.2 (spike, plan §1.6/§1.7): GNOME screenshot tool copy, named format `"image/png"` id `0xD011` | 320×200 RGBA PNG (805 bytes) |
| `remote_cf_unicodetext.bin` | Captured from g-r-d 50.2: GNOME Text Editor copy, `CF_UNICODETEXT` | UTF-16LE `copy-me-9137` + NUL |
| `dib_24_bottomup.bin` | Synthetic | `CF_DIB`, `BITMAPINFOHEADER`, 24 bpp, BI_RGB, height +2 |
| `dib_24_topdown.bin` | Synthetic | 24 bpp, BI_RGB, height −2 |
| `dib_32_bottomup.bin` | Synthetic | 32 bpp, BI_RGB (4th byte 0, must be ignored), height +2 |
| `dib_32_topdown_bitfields.bin` | Synthetic | 32 bpp, BI_BITFIELDS (R `0x00FF0000`, G `0x0000FF00`, B `0x000000FF`), height −2 |

All DIBs are 3×2 pixels; top row red, green, blue; bottom row white, black, `(10,20,30)`.
