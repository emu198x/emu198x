#!/usr/bin/env bash
# Capture the WRX fixture from ZEsarUX, for `wrx_fixture.rs` to compare against.
#
#   wrx-fixture.sh <zesarux-binary> <output.bmp>
#
# The fixture is not a program. It is a pattern page plus an `I` pointing at
# it, because `I` above $1F is the whole of what selects WRX — the ROM's own
# display routine then draws the bitmap with the ROM's own timing. Both
# emulators are handed exactly the same two things, so there is no `.p` to
# build, no BASIC to type, and no entry point to find.
#
# The three constants below are duplicated in `wrx_fixture.rs` and must match
# it. They are few enough to read at a glance, which beats generating one from
# the other and hiding the fixture inside a build step.
#
#   page  $60    a RAM page clear of the program, the display file and the stack
#   seed  $01
#   step  $4D    odd, so 256 additions walk all 256 byte values and the page
#                holds no repeats — which is what makes the picture detailed
#                enough to tell the two address paths apart
#
# Writes are chunked. A single `write-memory` carrying all 256 bytes is about a
# thousand characters and ZEsarUX's command parser loses the rest of the line,
# which shows up as `I` never being set rather than as an error.
set -euo pipefail

BIN="${1:?usage: wrx-fixture.sh <zesarux> <out.bmp>}"
OUT="${2:?usage: wrx-fixture.sh <zesarux> <out.bmp>}"

PAGE=0x60
SEED=0x01
STEP=0x4D

commands="$(python3 - "$PAGE" "$SEED" "$STEP" <<'PY'
import sys
page, seed, step = (int(v, 16) for v in sys.argv[1:4])
base = page << 8
v, vals = seed, []
for _ in range(256):
    vals.append(v)
    v = (v + step) & 0xFF
for i in range(0, 256, 32):
    print(f"write-memory {base + i} " + " ".join(str(x) for x in vals[i:i + 32]))
print(f"set-register I={page:02X}H")
PY
)"

ZRCP_PRE="$commands" POST_PRE="${POST_PRE:-6}" \
  exec "$(dirname "$0")/capture.sh" "$BIN" "$OUT" --wrx
