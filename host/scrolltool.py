#!/usr/bin/env python3
"""Drift e2e scroll helper (GTK4).

Opens a full-screen window that records every scroll event it receives and keeps
``/tmp/drift_scroll.txt`` up to date with a single line::

    total <dx> <dy> <scrolls> <keys> <motions>

``dx``/``dy`` are GTK's accumulated smooth-scroll deltas (positive dy = content scrolls
down). The e2e tests (M2-3) compare totals of different wheel-unit sequences, so only the
relative values and the sign matter; the key and motion counters tell a failing test whether
input reached the session at all.

Usage: ``scrolltool.py <seconds>``
"""
import sys

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, GLib, Gio  # noqa: E402

OUT = "/tmp/drift_scroll.txt"
total = [0.0, 0.0, 0, 0, 0]


def write():
    with open(OUT, "w") as f:
        f.write("total %.4f %.4f %d %d %d\n" % tuple(total))


def on_scroll(_controller, dx, dy):
    total[0] += dx
    total[1] += dy
    total[2] += 1
    write()
    return True


def on_key(_controller, _keyval, _keycode, _state):
    total[3] += 1
    write()
    return True


def on_motion(_controller, _x, _y):
    total[4] += 1
    write()


def on_activate(app):
    win = Gtk.ApplicationWindow(application=app, title="scrolltool")
    controller = Gtk.EventControllerScroll.new(Gtk.EventControllerScrollFlags.BOTH_AXES)
    controller.connect("scroll", on_scroll)
    win.add_controller(controller)
    keys = Gtk.EventControllerKey.new()
    keys.connect("key-pressed", on_key)
    win.add_controller(keys)
    motion = Gtk.EventControllerMotion.new()
    motion.connect("motion", on_motion)
    win.add_controller(motion)
    win.fullscreen()
    win.present()
    write()
    GLib.timeout_add_seconds(int(float(sys.argv[1])) if len(sys.argv) > 1 else 60, app.quit)


app = Gtk.Application(application_id="dev.drift.scrolltool", flags=Gio.ApplicationFlags.NON_UNIQUE)
app.connect("activate", on_activate)
app.run([])
