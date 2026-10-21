#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Fresh component observations. This never qualifies hardware or ROM loading."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import platform
import shutil
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
PIN = '27d2c30d86d3f4877e24dd3dd97d7ebd51bab7c6'


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--zxsp', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    source = args.zxsp.resolve()
    if not (source / 'Source/Uni/Items/Fdc/Fdc765.cpp').is_file():
        parser.error('missing prerequisite: zxsp Fdc765.cpp')
    compiler = shutil.which('clang++')
    if compiler is None:
        parser.error('missing prerequisite: clang++')
    commit = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if commit != PIN:
        parser.error(f'blocked dependency: requires zxsp {PIN}, got {commit}')
    args.output.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location('prior_sk', HERE.parent / '2026-10-21-read-all-skipped/capture.py')
    assert spec and spec.loader
    prior = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prior)
    packet = {
        'qualified': False, 'status': 'blocked_dependency',
        'scope': 'source-built zxsp controller component with authored rotating-byte host; no CPU, ROM or real drive',
        'execution': 'fresh', 'producer': 'source_build',
        'reference': {'repository': 'https://github.com/Megatokio/zxsp', 'commit': commit, 'files': {}},
        'host': {'platform': platform.platform(), 'python': platform.python_version(), 'cwd': str(Path.cwd()),
                 'environment': {k: os.environ.get(k) for k in ['PATH', 'SDKROOT', 'MACOSX_DEPLOYMENT_TARGET', 'ASAN_OPTIONS', 'UBSAN_OPTIONS']}},
        'compiler': {'path': compiler, 'sha256': sha(Path(compiler).resolve()),
                     'version': subprocess.check_output([compiler, '--version'], text=True)},
        'recipe': {}, 'runs': [],
    }
    for path in sorted([HERE / 'capture.py', HERE / 'probe.cpp', HERE.parent / '2026-10-21-read-all-skipped/capture.py', *HERE.joinpath('host').rglob('*.h')]):
        packet['recipe'][str(path.relative_to(HERE.parent))] = sha(path)
    with tempfile.TemporaryDirectory(prefix='zxsp-sk-') as temp:
        build = Path(temp)
        shutil.copytree(HERE / 'host', build, dirs_exist_ok=True)
        for name in ['Fdc765.cpp', 'Fdc765.h']:
            relative = f'Source/Uni/Items/Fdc/{name}'
            path = source / relative
            original = subprocess.check_output(['git', '-C', str(source), 'show', f'{PIN}:{relative}'])
            if path.read_bytes() != original:
                raise ValueError(f'changed reference source: {relative}')
            shutil.copyfile(path, build / name)
            packet['reference']['files'][relative] = sha(path)
        exe = build / 'probe'
        command = [compiler, '-std=c++17', '-O1', '-g', '-fsanitize=address,undefined', '-fno-sanitize-recover=all', '-I', str(build), str(HERE / 'probe.cpp'), str(build / 'Fdc765.cpp'), '-o', str(exe)]
        result = subprocess.run(command, capture_output=True, text=True, timeout=60, check=False)
        (args.output / 'compile.stdout').write_text(result.stdout)
        (args.output / 'compile.stderr').write_text(result.stderr)
        packet['build'] = {'command': command, 'exit_code': result.returncode}
        result.check_returncode()
        packet['executable_sha256'] = sha(exe)
        cases = [(op, sk, matching, two, False) for op in [6, 12] for sk in [False, True] for matching in [False, True] for two in [False, True]]
        cases += [(op, True, False, True, True) for op in [6, 12]]
        for repeat in range(2):
            for op, sk, matching, two, corrupt in cases:
                name = f'op{op:02x}-sk{int(sk)}-match{int(matching)}-two{int(two)}-crc{int(corrupt)}-r{repeat}'
                folder = args.output / name
                folder.mkdir()
                deleted = (op == 12) == matching
                # Byte-for-byte prior recipe, stripping Fuse's descriptor/clock maps.
                raw = bytearray(prior.raw_track(deleted, two)[3:6253])
                if corrupt:
                    # Locate first data-field CRC using the independently fixed layout.
                    crc_at = 80 + 12 + 3 + 5 + 2 + 22 + 12 + 3 + 1 + 512
                    raw[crc_at + 1] ^= 1
                (folder / 'track.bin').write_bytes(raw)
                command = [str(exe), str(folder / 'track.bin'), str(folder / 'live.bin'), str(0x40 | (0x20 if sk else 0) | op), '2', '1', '255']
                result = subprocess.run(command, capture_output=True, timeout=15, check=False)
                (folder / 'stdout.json').write_bytes(result.stdout)
                (folder / 'stderr.txt').write_bytes(result.stderr)
                packet['runs'].append({'name': name, 'opcode': op, 'skip': sk, 'matching': matching, 'two_sectors': two, 'bad_crc': corrupt, 'repeat': repeat, 'command': command, 'host_timeout_seconds': 15, 'exit_code': result.returncode})
        packet['artifacts'] = {str(p.relative_to(args.output)): sha(p) for p in sorted(args.output.rglob('*')) if p.is_file()}
        (args.output / 'manifest.json').write_text(json.dumps(packet, indent=2) + '\n')
    print(f'Retained {len(packet["runs"])} fresh process observations in {args.output}; not qualified.')


if __name__ == '__main__':
    main()
