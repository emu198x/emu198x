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
#     `save-screen` returns a black image.
#   - `simpletext` reconstructs *characters*, which is enough for the boot
#     screen but cannot represent a WRX bitmap. Pixel captures need the
#     `xwindows` driver and a display. Xvfb will not start on macOS when
#     XQuartz holds the socket, so this uses the real X display, which means a
#     window appears while capturing.
set -euo pipefail

SRC="${ZESARUX_SRC:-$HOME/Projects/198x/emulators/zx-spectrum/zesarux/src}"
OUT="${1:-./zesarux-build}"

[ -d "$SRC" ] || { echo "no ZEsarUX source at $SRC" >&2; exit 1; }

mkdir -p "$OUT"
cp -R "$SRC/." "$OUT/"
cd "$OUT"

# --- Local patch: make --wrx survive the smartload ---------------------------
#
# `--wrx` is applied during startup (`start.c`, around the comment "Algun
# parametro que se resetea con reset_cpu y/o set_machine"), but a file given on
# the command line is smartloaded about 200 lines *later*, and the load resets
# the flag. So `zesarux --wrx game.p` starts the program with WRX off.
#
# The symptom is precise and worth recognising: the screen renders with exactly
# **256 distinct 8-pixel cells**, one per (row-in-character, column) pair, which
# is what the character path produces from a uniform display file. With WRX
# actually on, the same programs render 421 and 666.
#
# Re-applying it after the load fixes it. This is a local diagnostic patch, not
# something upstream has been told about.
python3 - <<'PATCH'
import pathlib
p = pathlib.Path("start.c")
s = p.read_text()
old = """    if (quickload_inicial.v==1) {
        debug_printf(VERBOSE_INFO,"Smartloading %s",quickload_nombre);
        quickload(quickload_nombre);
    }
"""
if old in s and "Re-enabling WRX after smartload" not in s:
    s = s.replace(old, old + """
    // Emu198x local patch: --wrx is applied before the smartload above and the
    // load resets it. Re-apply so the flag survives.
    if (command_line_wrx.v) {
        debug_printf(VERBOSE_INFO,"Re-enabling WRX after smartload");
        enable_wrx();
    }
""", 1)
    p.write_text(s)
    print("patched start.c: WRX re-applied after smartload")
else:
    print("start.c not patched (already applied, or upstream changed)")
PATCH

./configure \
  --disable-sdl --disable-pulse --disable-alsa --disable-coreaudio --disable-dsp \
  --disable-cocoa --disable-curses --disable-cursesw

make -j"$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"

./zesarux --version | head -1
echo "built: $OUT/zesarux"
