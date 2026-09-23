#!/usr/bin/env python3
"""Fetch a pinned, matched Open ROMs set, notices, and corresponding source.

Local prototype inputs only; generated files are ignored by Git. No original
Commodore firmware or hybrid ROM is fetched or embedded in the WASM module.
"""
import hashlib
import json
from pathlib import Path
import urllib.request

root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / 'scripts/open-roms.json').read_text())
revision = manifest['revision']
out = root / 'example/roms'
out.mkdir(exist_ok=True)
base = f'https://raw.githubusercontent.com/MEGA65/open-roms/{revision}/'
for name, entry in manifest['files'].items():
    data = urllib.request.urlopen(base + entry['source'], timeout=60).read()
    if hashlib.sha256(data).hexdigest() != entry['sha256']:
        raise SystemExit(f'Checksum mismatch: {name}')
    (out / name).write_bytes(data)
    print(f'{name}: verified')
# Keep the exact corresponding source beside the binaries, including per-file
# exceptions and build instructions. These are separate, user-replaceable files.
source = urllib.request.urlopen(
    f'https://codeload.github.com/MEGA65/open-roms/tar.gz/{revision}', timeout=60
).read()
(out / 'open-roms-source.tar.gz').write_bytes(source)
for name in ['COPYING', 'COPYING.LESSER']:
    url = 'https://www.gnu.org/licenses/' + ('gpl-3.0.txt' if name == 'COPYING' else 'lgpl-3.0.txt')
    (out / name).write_bytes(urllib.request.urlopen(url, timeout=60).read())
(out / 'PROVENANCE.txt').write_text(
    f'MEGA65 Open ROMs, unchanged generic BASIC/KERNAL and Open ROMs character set.\n'
    f'Source: https://github.com/MEGA65/open-roms/tree/{revision}\n'
    'Licence: LGPL-3.0-or-later, with per-file exceptions; see LICENSE and source.\n'
    'Corresponding source: open-roms-source.tar.gz\n'
    'Do not mix BASIC/KERNAL from different builds or original firmware.\n'
)
print(f'Installed local prototype inputs at {out}')
