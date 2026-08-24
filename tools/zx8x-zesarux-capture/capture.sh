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
# `.pbm` and `.scr` are Spectrum-only in ZEsarUX; BMP is the format that works
# for a ZX81.
set -euo pipefail

BIN="${1:?usage: capture.sh <zesarux> <out.bmp> [args...]}"
OUT="${2:?usage: capture.sh <zesarux> <out.bmp> [args...]}"
shift 2

PORT="${ZRCP_PORT:-10000}"
SETTLE="${SETTLE:-12}"

rm -f "$OUT"
"$BIN" --noconfigfile --machine ZX81 --realvideo \
       --vo simpletext --ao null \
       --enable-remoteprotocol --quickexit --exit-after $((SETTLE + 20)) \
       "$@" >/dev/null 2>&1 &
zpid=$!
trap 'kill "$zpid" 2>/dev/null || true' EXIT

sleep "$SETTLE"
{ printf 'save-screen %s\n' "$(cd "$(dirname "$OUT")" && pwd)/$(basename "$OUT")"; sleep 3; printf 'exit\n'; } \
  | nc 127.0.0.1 "$PORT" >/dev/null 2>&1 || true
sleep 1

[ -s "$OUT" ] || { echo "no capture written — is the ZRCP port $PORT free?" >&2; exit 1; }
echo "wrote $OUT"
