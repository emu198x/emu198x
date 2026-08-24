#!/usr/bin/env bash
# Capture a ZX81 screen from ZEsarUX, headless, as a BMP.
#
#   capture.sh <zesarux-binary> <output.bmp> [extra zesarux args...]
#
# Examples:
#   capture.sh ./zesarux-build/zesarux boot.bmp
#   capture.sh ./zesarux-build/zesarux wrx.bmp --wrx "Starfight.p"
#
# `--realvideo` is always passed: without it the ZX81's display is not
# generated the way the hardware does, which is the whole point of comparing
# against it. ZEsarUX's own per-title list turns it on for every WRX program.
#
# Capturing a WRX screen needs three things, and missing any one of them yields
# a picture that looks plausible and is wrong:
#
#   1. `--wrx`, plus the build.sh patch that makes it survive the smartload.
#   2. The `xwindows` driver. `simpletext` reconstructs characters and cannot
#      represent a bitmap; `null` does not populate the buffer at all.
#   3. A display. There is no headless option here — see build.sh.
#
# The tell that WRX is not engaging is exactly 256 distinct 8-pixel cells:
# one per (row-in-character, column), which is the character path rendering a
# uniform display file. Working captures give hundreds more.
#
# `.pbm` and `.scr` are Spectrum-only in ZEsarUX; BMP is the format that works
# for a ZX81.
#
# Set ZESARUX_VO=simpletext for a character-only screen without needing X.
#
# ZRCP_PRE holds newline-separated ZRCP commands to run once the smartload has
# settled, before the screen is saved. Its reason for existing is that most
# ZX81 hi-res demos are a `1 REM <machine code>` that the user starts by typing
# `RAND USR nnnnn`; a smartload alone leaves them sitting at the K cursor. So:
#
#   ZRCP_PRE='set-register PC=4084H' capture.sh ... --wrx "WRX Demo v1.0.p"
#
# The REM body of such a program usually opens with one or two HALT ($76) bytes
# so that LIST stops there rather than printing the code as garbage. Those pad
# bytes are not the entry point — enter past them, or the CPU simply halts.
#
# The `H` suffix is not optional. ZRCP reads `PC=4084` as decimal 4084, which
# is $0FF4, inside the ROM; the emulator carries on and the capture looks like
# a program that drew nothing rather than like a mistyped address.
set -euo pipefail

BIN="${1:?usage: capture.sh <zesarux> <out.bmp> [args...]}"
OUT="${2:?usage: capture.sh <zesarux> <out.bmp> [args...]}"
shift 2

PORT="${ZRCP_PORT:-10000}"
SETTLE="${SETTLE:-12}"

rm -f "$OUT"
"$BIN" --noconfigfile --machine ZX81 --realvideo \
       --vo "${ZESARUX_VO:-xwindows}" --ao null \
       --enable-remoteprotocol --quickexit --exit-after $((SETTLE + 20)) \
       "$@" >/dev/null 2>&1 &
zpid=$!
trap 'kill "$zpid" 2>/dev/null || true' EXIT

sleep "$SETTLE"
if [ -n "${ZRCP_PRE:-}" ]; then
  { printf '%s\n' "$ZRCP_PRE"; sleep 2; printf 'exit\n'; } | nc 127.0.0.1 "$PORT" >/dev/null 2>&1 || true
  sleep "${POST_PRE:-4}"
fi
{ printf 'save-screen %s\n' "$(cd "$(dirname "$OUT")" && pwd)/$(basename "$OUT")"; sleep 3; printf 'exit\n'; } \
  | nc 127.0.0.1 "$PORT" >/dev/null 2>&1 || true
sleep 1

[ -s "$OUT" ] || { echo "no capture written — is the ZRCP port $PORT free?" >&2; exit 1; }
echo "wrote $OUT"
