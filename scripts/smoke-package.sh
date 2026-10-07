#!/usr/bin/env bash
set -euo pipefail
package=$1
package_bin=$2
fixture_dir=$3/$package-smoke
mkdir -p "$fixture_dir"
case "$package" in
  emu198x-*) "$package_bin/$package" --help > "$fixture_dir/help.txt"; test -s "$fixture_dir/help.txt" ;;
  *) echo "Unexpected package: $package" >&2; exit 1 ;;
esac
# NES needs no external firmware: exercise a real frame using a tiny NROM
# cartridge authored here. Every other machine still checks native loading.
if [ "$package" = emu198x-nes ]; then
  python3 - "$fixture_dir/smoke.nes" <<'PY'
from pathlib import Path
import sys
header = b'NES\x1a' + bytes([1, 1]) + bytes(10)
prg = bytearray(16384)
prg[:3] = bytes([0x4c, 0x00, 0x80])  # JMP $8000
prg[-6:] = bytes([0x00, 0x80]) * 3   # NMI, reset and IRQ vectors
Path(sys.argv[1]).write_bytes(header + prg + bytes(8192))
PY
  "$package_bin/$package" --headless --rom "$fixture_dir/smoke.nes" --frames 2 --screenshot "$fixture_dir/frame.png" > "$fixture_dir/report.json"
  python3 - "$fixture_dir" <<'PY'
import json
from pathlib import Path
import struct
import sys
root = Path(sys.argv[1])
report = json.loads((root / 'report.json').read_text())
assert report['time'] > 0, report
image = (root / 'frame.png').read_bytes()
assert image[:8] == b'\x89PNG\r\n\x1a\n'
assert struct.unpack('>II', image[16:24]) == (256, 240)
PY
fi
