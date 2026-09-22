#!/usr/bin/env python3
"""Drift e2e clipboard helper (GTK4).

Usage::

    cliptool.py read [seconds]
    cliptool.py write-text <text> [seconds]
    cliptool.py write-image <png path> [seconds]

Results are appended to ``/tmp/drift_clip_result.txt`` (override with ``DRIFT_CLIP_OUT``)::

    formats: <GdkContentFormats>
    text: '<pasted text>'
    image: <width>x<height>
    set text
    set image <width>x<height>

The write modes set the clipboard **when a key is pressed in the window**, exactly like a user
pressing Ctrl+C: Wayland only grants the selection to a client that can show a recent input
serial, and GNOME's focus-stealing prevention leaves a freshly launched window unfocused
anyway (plan §1.6). The e2e test therefore clicks the window and then presses a key.
"""
import os
import sys

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gtk, Gdk, GLib, Gio  # noqa: E402

OUT = os.environ.get("DRIFT_CLIP_OUT", "/tmp/drift_clip_result.txt")
mode = sys.argv[1] if len(sys.argv) > 1 else "read"
seconds = int(float(sys.argv[3])) if len(sys.argv) > 3 else 60


def log(line):
    with open(OUT, "a") as f:
        f.write(line + "\n")


def read_clipboard(app, clipboard):
    formats = clipboard.get_formats()
    log("formats: " + formats.to_string())

    def text_done(cb, result):
        try:
            log("text: " + repr(cb.read_text_finish(result)))
        except Exception as error:  # noqa: BLE001 - reported to the test
            log("text err: %s" % error)
        if formats.contain_gtype(Gdk.Texture) or "image/png" in formats.to_string():
            def image_done(cb, result):
                try:
                    texture = cb.read_texture_finish(result)
                    texture.save_to_png("/tmp/drift_clip.png")
                    log("image: %dx%d" % (texture.get_width(), texture.get_height()))
                except Exception as error:  # noqa: BLE001
                    log("image err: %s" % error)
                app.quit()
            cb.read_texture_async(None, image_done)
        else:
            app.quit()

    clipboard.read_text_async(None, text_done)


def write_clipboard(clipboard):
    if mode == "write-text":
        clipboard.set(sys.argv[2])
        log("set text")
    else:
        # `new_for_bytes` advertises exactly `image/png`, which is the format g-r-d maps to
        # CLIPRDR (plan §1.6); a texture provider advertises the GdkTexture GType instead and
        # mutter then announces no image mime type at all.
        texture = Gdk.Texture.new_from_filename(sys.argv[2])
        with open(sys.argv[2], "rb") as png:
            data = GLib.Bytes.new(png.read())
        provider = Gdk.ContentProvider.new_for_bytes("image/png", data)
        clipboard.set_content(provider)
        log("set image %dx%d" % (texture.get_width(), texture.get_height()))


def on_activate(app):
    window = Gtk.ApplicationWindow(application=app, title="cliptool")
    window.set_default_size(600, 400)
    window.present()
    clipboard = window.get_display().get_clipboard()
    if mode == "read":
        GLib.timeout_add(1500, lambda: read_clipboard(app, clipboard) or False)
        return
    written = [False]

    def write_once():
        if not written[0]:
            written[0] = True
            log("focused: %s" % window.is_active())
            write_clipboard(clipboard)
        return False

    def on_key(_controller, _keyval, _keycode, _state):
        write_once()
        return True

    keys = Gtk.EventControllerKey.new()
    keys.connect("key-pressed", on_key)
    window.add_controller(keys)
    GLib.timeout_add_seconds(seconds, app.quit)


app = Gtk.Application(application_id="dev.drift.cliptool", flags=Gio.ApplicationFlags.NON_UNIQUE)
app.connect("activate", on_activate)
app.run([])
