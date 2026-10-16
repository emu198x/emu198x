"""Check the real Windows main-thread startup path with its unchanged PE reserve."""

import argparse
import hashlib
import json
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--expect-overflow', action='store_true')
args = parser.parse_args()
args.output.mkdir(parents=True)
report = {'passed': False, 'expect_overflow': args.expect_overflow}
try:
    if sys.platform != 'win32':
        raise ValueError('Native Windows is required')
    image = args.binary.read_bytes()
    if image[:2] != b'MZ':
        raise ValueError('Expected PE image')
    pe = struct.unpack_from('<I', image, 0x3C)[0]
    if image[pe:pe + 4] != b'PE\0\0' or struct.unpack_from('<H', image, pe + 4)[0] != 0x8664:
        raise ValueError('Expected x86-64 PE image')
    if struct.unpack_from('<H', image, pe + 24)[0] != 0x20B:
        raise ValueError('Expected PE32+')
    reserve = struct.unpack_from('<Q', image, pe + 24 + 72)[0]
    report.update(executable_sha256=hashlib.sha256(image).hexdigest(), pe_stack_reserve_bytes=reserve)
    if reserve != 1024 * 1024:
        raise ValueError('Regression requires the original 1 MiB stack reserve')
    script = args.output / 'startup.json'
    script.write_text('[{"action":"query_cpu"}]\n', encoding='utf-8')
    with tempfile.TemporaryDirectory(prefix='spectrum-startup-') as folder:
        rom = Path(folder) / 'synthetic.rom'
        rom.write_bytes(bytes(16384))
        result = subprocess.run([str(args.binary.resolve()), '--machine', 'spectrum_48k', '--rom', str(rom), '--script', str(script.resolve())], capture_output=True, text=True, encoding='utf-8', timeout=15, check=False)
    (args.output / 'stdout.txt').write_text(result.stdout, encoding='utf-8')
    (args.output / 'stderr.txt').write_text(result.stderr, encoding='utf-8')
    report['exit_code'] = result.returncode
    if args.expect_overflow:
        if result.returncode != 0xC00000FD or 'has overflowed its stack' not in result.stderr:
            raise ValueError('Baseline did not reproduce the expected stack overflow')
    else:
        if result.returncode:
            raise ValueError(f'Startup failed: {result.returncode}')
        data = json.loads(result.stdout)
        if data['time'] != 0 or len(data['observations']) != 1:
            raise ValueError('Expected one observation at reset')
        observation = data['observations'][0]
        if observation['kind'] != 'query_cpu' or observation['registers']['pc'] != '$0000':
            raise ValueError('Startup did not return reset CPU state')
    report['passed'] = True
except Exception as error:
    report['error'] = str(error)
    raise
finally:
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
