#!/usr/bin/env bash
# fs-uae-cross-check.sh — boot fs-uae with the same ROM + ADF our Amiga
# MCP uses, capture a screenshot after the boot stabilises, then drop a
# side-by-side comparison PNG next to it.
#
# Used to verify whether our emulator's WB 3.1 palette (RGB primaries
# at COLOR 0..3, grey ramp at COLOR 17..31) matches what real Amiga
# OS produces with the same disk + ROM, or whether we're missing
# something in OS init / trackdisk / filesystem.
#
# Requirements:
#   - fs-uae on $PATH (`brew install fs-uae`)
#   - macOS `screencapture` + `osascript` (built in)
#   - ImageMagick `magick` for side-by-side compose (optional)
#
# Usage:
#   tools/fs-uae-cross-check.sh
#
# Output (in /tmp/amiga-cross-check):
#   fs-uae.png        — fs-uae screenshot after boot
#   emu198x.png       — our emulator at the same point
#   side-by-side.png  — composed comparison (if magick)

set -euo pipefail

ROM="$HOME/.emu198x/roms/commodore-amiga/kick31a1200.rom"
ADF="$HOME/.emu198x/media/commodore-amiga/wb31/Workbench v3.1 rev 40.42 (1996)(ESCOM)(M10)(Disk 2 of 6)(Workbench).adf"
OUT="/tmp/amiga-cross-check"
BOOT_WAIT=45  # seconds — past our cck 58M (~frame 410) palette settle
EMU198X="$(cd "$(dirname "$0")/.." && pwd)/target/release/emu198x-amiga"

mkdir -p "$OUT"

if [[ ! -x "$EMU198X" ]]; then
    echo "Build emu198x-amiga first: cargo build --release -p emu198x-amiga" >&2; exit 1
fi
if [[ ! -f "$ROM" ]]; then echo "Missing KS 3.1 A1200 ROM: $ROM" >&2; exit 1; fi
if [[ ! -f "$ADF" ]]; then echo "Missing WB 3.1 Disk 2 ADF: $ADF" >&2; exit 1; fi

pkill -f fs-uae 2>/dev/null && sleep 1 || true

# ─── fs-uae side ──────────────────────────────────────────────────
echo "▶ Launching fs-uae (A1200, KS 3.1, WB 3.1 Disk 2)…"
WIN_X=20
WIN_Y=40
fs-uae \
    --amiga_model=A1200 \
    --kickstart_file="$ROM" \
    --floppy_drive_0="$ADF" \
    --floppy_drive_0_sounds=off \
    --floppy_drive_speed=800 \
    --fullscreen=0 \
    --window_x_offset=$WIN_X \
    --window_y_offset=$WIN_Y \
    >"$OUT/fs-uae.stdout" 2>"$OUT/fs-uae.stderr" &
FS_UAE_PID=$!

sleep 3
echo "  fs-uae PID: $FS_UAE_PID — booting for ${BOOT_WAIT}s…"
sleep "$BOOT_WAIT"

# Get the fs-uae window id via CoreGraphics (Swift one-liner; no
# Accessibility prompt, survives overlapping windows). `screencapture
# -l <id>` then grabs only that window's pixels — even when other
# apps are sitting in front of it.
echo "▶ Resolving fs-uae window id + capturing…"
WIN_ID=$(/usr/bin/swift - 2>/dev/null <<'SWIFT'
import Cocoa
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let windows = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for w in windows {
    let owner = (w[kCGWindowOwnerName as String] as? String) ?? ""
    let name  = (w[kCGWindowName  as String] as? String) ?? ""
    if owner.lowercased().contains("fs-uae") || name.lowercased().contains("amiga") {
        if let num = w[kCGWindowNumber as String] as? Int {
            print(num); exit(0)
        }
    }
}
exit(2)
SWIFT
)

if [[ -n "$WIN_ID" ]]; then
    screencapture -l "$WIN_ID" -x -o "$OUT/fs-uae.png"
    echo "  captured window id $WIN_ID → $OUT/fs-uae.png"
else
    echo "  Swift window-id lookup failed; falling back to region capture"
    screencapture -R "${WIN_X},${WIN_Y},800,650" -x -o "$OUT/fs-uae.png"
fi

kill "$FS_UAE_PID" 2>/dev/null || true
wait "$FS_UAE_PID" 2>/dev/null || true
pkill -f fs-uae 2>/dev/null || true

# ─── emu198x side ─────────────────────────────────────────────────
echo "▶ Booting our emulator and capturing PNG at the matching point…"
ZIP="$HOME/Projects/198x/assets/amiga/Operating Systems/Workbench/Workbench v3.1 rev 40.42 (1996)(ESCOM)(M10)(Disk 2 of 6)(Workbench).zip"
printf '%s\n%s\n%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":300}}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"insert_media\",\"arguments\":{\"path\":\"$ZIP\"}}}" \
    '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":3000}}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"dump_framebuffer\",\"arguments\":{\"path\":\"$OUT/emu198x.png\"}}}" \
    | "$EMU198X" --mcp >"$OUT/emu198x.json" 2>"$OUT/emu198x.stderr"
echo "  $OUT/emu198x.png"

# ─── compose ──────────────────────────────────────────────────────
if command -v magick >/dev/null 2>&1 && [[ -f "$OUT/fs-uae.png" && -f "$OUT/emu198x.png" ]]; then
    echo "▶ Composing side-by-side comparison…"
    # Normalise heights so panels line up; let magick scale the
    # taller one to match.
    # Use a built-in macOS font; magick's default lookup is shaky.
    FONT="/System/Library/Fonts/Helvetica.ttc"
    [[ -f "$FONT" ]] || FONT="/System/Library/Fonts/HelveticaNeue.ttc"
    [[ -f "$FONT" ]] || FONT=""
    magick \
        \( "$OUT/emu198x.png" -resize x720 -bordercolor "#333" -border 2 \) \
        \( "$OUT/fs-uae.png"  -resize x720 -bordercolor "#333" -border 2 \) \
        +append \
        -bordercolor white -border "20x44" \
        ${FONT:+-font "$FONT"} -fill black -pointsize 22 \
        -gravity NorthWest -annotate +30+12 'emu198x' \
        -gravity North     -annotate +0+12  'KS 3.1 A1200 + WB 3.1 Disk 2' \
        -gravity NorthEast -annotate +30+12 'fs-uae' \
        "$OUT/side-by-side.png"
    echo "  $OUT/side-by-side.png"
fi

echo
echo "Done. Artefacts in $OUT:"
ls -la "$OUT"/*.png 2>/dev/null || true
