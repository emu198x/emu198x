#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Capture unchanged Arnold FDC/FDI/FDD component observations; never qualify hardware."""
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
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ARCHIVE_SHA = 'bf124873d8e4a71cb0cb9ac92d846e4b0540c531c1581cc64cdb4e63145d58bd'
MEMBERS = ['fdc.c', 'fdc.h', 'fdi.c', 'fdi.h', 'fdd.c', 'fdd.h', 'device.h', 'diskimage/diskimg.h']


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-archive', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if not args.source_archive.is_file():
        parser.error('missing prerequisite: Arnold source archive')
    if sha(args.source_archive.read_bytes()) != ARCHIVE_SHA:
        parser.error('blocked dependency: source archive identity mismatch')
    if shutil.which('xcrun') is None:
        parser.error('missing prerequisite: xcrun/Apple clang')
    compiler = subprocess.check_output(['xcrun', '--find', 'clang'], text=True).strip()
    sdk = subprocess.check_output(['xcrun', '--show-sdk-path'], text=True).strip()
    args.output.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location('prior_sk', HERE.parent / '2026-10-21-read-all-skipped/capture.py')
    assert spec and spec.loader
    prior = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prior)
    packet = {
        'qualified': False, 'status': 'blocked_dependency', 'execution': 'fresh', 'producer': 'source_build',
        'scope': 'Arnold controller, drive interface and drive model; authored disk-image/platform adapter; no CPU/ROM/hardware',
        'reference': {'archive_sha256': ARCHIVE_SHA, 'archive_url': 'https://web.archive.org/web/20170608014028id_/http://cpctech.cpc-live.com/arnoldsrc.zip',
                      'source_commit': None, 'archive_path': str(args.source_archive.resolve()), 'members': {}},
        'sdk': {'path': sdk, 'settings_sha256': sha((Path(sdk) / 'SDKSettings.json').read_bytes())},
        'compiler': {'path': compiler, 'sha256': sha(Path(compiler).read_bytes()), 'version': subprocess.check_output([compiler, '--version'], text=True)},
        'host': {'platform': platform.platform(), 'python': platform.python_version(), 'cwd': str(Path.cwd()),
                 'environment': {k: os.environ.get(k) for k in ['PATH', 'SDKROOT', 'MACOSX_DEPLOYMENT_TARGET', 'ASAN_OPTIONS', 'UBSAN_OPTIONS']}},
        'recipe': {}, 'runs': [],
    }
    for path in sorted([HERE / 'capture.py', HERE / 'probe.c', HERE.parent / '2026-10-21-read-all-skipped/capture.py', *HERE.joinpath('host').rglob('*.h')]):
        packet['recipe'][str(path.relative_to(HERE.parent))] = sha(path.read_bytes())
    with tempfile.TemporaryDirectory(prefix='arnold-sk-') as temp:
        build = Path(temp)
        shutil.copytree(HERE / 'host', build, dirs_exist_ok=True)
        with zipfile.ZipFile(args.source_archive) as archive:
            for name in MEMBERS:
                member = 'src/cpc/' + name
                data = archive.read(member)
                dest = build / name
                dest.parent.mkdir(exist_ok=True)
                dest.write_bytes(data)
                packet['reference']['members'][member] = sha(data)
        exe = build / 'probe'
        command = [compiler, '-isysroot', sdk, '-std=gnu99', '-O1', '-g', '-fsanitize=address,undefined', '-fno-sanitize-recover=all', '-I', str(build), str(HERE / 'probe.c'), *[str(build / n) for n in ['fdc.c', 'fdi.c', 'fdd.c']], '-o', str(exe)]
        result = subprocess.run(command, capture_output=True, timeout=60, check=False)
        (args.output / 'compile.stdout').write_bytes(result.stdout)
        (args.output / 'compile.stderr').write_bytes(result.stderr)
        packet['build'] = {'command': command, 'exit_code': result.returncode, 'host_timeout_seconds': 60}
        result.check_returncode()
        packet['executable_sha256'] = sha(exe.read_bytes())
        cases = [(op, sk, match, two, 1, False) for op in [6, 12] for sk in [False, True] for match in [False, True] for two in [False, True]]
        cases += [(op, True, False, True, 2, False) for op in [6, 12]]
        cases += [(op, True, match, True, 1, True) for op in [6, 12] for match in [False, True]]
        for repeat in [0, 1]:
            for poll in [1, 32]:
                for op, sk, match, two, eot, bad in cases:
                    name = f'op{op:02x}-sk{int(sk)}-match{int(match)}-two{int(two)}-eot{eot}-crc{int(bad)}-poll{poll}-r{repeat}'
                    folder = args.output / name
                    folder.mkdir()
                    image = bytearray(prior.edsk((op == 12) == match, two))
                    if bad:
                        image[284] |= 0x20
                        image[285] |= 0x20
                    (folder / 'input.edsk').write_bytes(image)
                    command = [str(exe), str(folder / 'input.edsk'), str(folder / 'live.edsk'), str(0x40 | (0x20 if sk else 0) | op), str(eot), str(poll)]
                    result = subprocess.run(command, capture_output=True, timeout=15, check=False)
                    (folder / 'stdout.json').write_bytes(result.stdout)
                    (folder / 'stderr.txt').write_bytes(result.stderr)
                    packet['runs'].append({'name': name, 'opcode': op, 'skip': sk, 'matching': match, 'two_sectors': two, 'eot': eot, 'bad_crc_metadata': bad, 'poll_us': poll, 'repeat': repeat, 'command': command, 'host_timeout_seconds': 15, 'exit_code': result.returncode})
        packet['artifacts'] = {str(p.relative_to(args.output)): sha(p.read_bytes()) for p in sorted(args.output.rglob('*')) if p.is_file()}
        (args.output / 'manifest.json').write_text(json.dumps(packet, indent=2) + '\n')
    print(f'Retained {len(packet["runs"])} fresh processes; qualified=false.')


if __name__ == '__main__':
    main()
