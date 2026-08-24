#!/usr/bin/env bash
# Build ZEsarUX headless, for use as a ZX8x reference alongside MAME.
#
# Why ZEsarUX and not the others #295/#297 name:
#
#   EightyOne  Delphi for Windows. Not buildable here. Its source is still the
#              best reference *to read* — it is where the WRX address formation
#              came from — but it will not run on this machine.
#   zxsp       macOS, but an Xcode GUI application with no headless mode.
#   ZEsarUX    C, configure-based, and has a remote command protocol with a
#              `save-screen` command. This one works.
#
# Builds from the vendored copy under emulators/, into a directory you name,
# leaving the vendored tree untouched — it is reference-only.
#
# Two configure notes, both learned the hard way:
#   - The Cocoa driver fails to link on arm64 (`_joystickAction` unresolved),
#     so it is disabled. Nothing here needs a GUI.
#   - The `null` video driver does not populate the screen buffer, so
#     `save-screen` returns a black image. Capture with `simpletext`.
set -euo pipefail

SRC="${ZESARUX_SRC:-$HOME/Projects/198x/emulators/zx-spectrum/zesarux/src}"
OUT="${1:-./zesarux-build}"

[ -d "$SRC" ] || { echo "no ZEsarUX source at $SRC" >&2; exit 1; }

mkdir -p "$OUT"
cp -R "$SRC/." "$OUT/"
cd "$OUT"

./configure \
  --disable-sdl --disable-pulse --disable-alsa --disable-coreaudio --disable-dsp \
  --disable-cocoa --disable-xwindows --disable-curses --disable-cursesw

make -j"$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"

./zesarux --version | head -1
echo "built: $OUT/zesarux"
