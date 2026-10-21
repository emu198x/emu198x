#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Fresh bounded reference research, never ROM-loader qualification."""
from __future__ import annotations

import binascii
import hashlib
import json
import os
import platform
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
FAMILY = Path('/Users/stevehill/Projects/198x')
FUSE = FAMILY / 'emulators/zx-spectrum/fuse-emulator-fuse'
MAME_SOURCE = FAMILY / 'emulators/multi-system/mame'
PAYLOAD = bytes((17 * i + 3) & 255 for i in range(128))


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def raw_track(deleted: bool, bad_crc: bool) -> bytes:
    """Same gap/clock/CRC recipe as prior standalone Fuse studies."""
    raw = bytearray(3 + 6250 + 3 * 782)
    raw[:3] = bytes([6250 & 255, 6250 >> 8, 0])
    pos = 0

    def put(value: int, missing_clock: bool = False) -> None:
        nonlocal pos
        assert pos < 6250
        raw[3 + pos] = value
        if missing_clock:
            raw[3 + 6250 + pos // 8] |= 1 << (pos % 8)
        pos += 1

    def gap(count: int, value: int) -> None:
        for _ in range(count):
            put(value)

    def field(data: bytes, corrupt: bool = False) -> None:
        for _ in range(3):
            put(0xA1, True)
        for value in data:
            put(value)
        crc = binascii.crc_hqx(b'\xa1\xa1\xa1' + data, 0xFFFF)
        put(crc >> 8)
        put((crc & 255) ^ int(corrupt))

    gap(80, 0x4E)
    gap(12, 0)
    field(bytes([0xFE, 0, 0, 1, 0]))
    gap(22, 0x4E)
    gap(12, 0)
    field(bytes([0xF8 if deleted else 0xFB]) + PAYLOAD, bad_crc)
    gap(6250 - pos, 0x4E)
    return bytes(raw)


def edsk(deleted: bool, bad_crc: bool) -> bytes:
    data = bytearray(768)
    signature = b'EXTENDED CPC DSK File\r\nDisk-Info\r\n'
    data[:len(signature)] = signature
    data[0x22:0x30] = b'198x DTL zero '
    data[0x30:0x32] = bytes([1, 1])
    data[0x34] = 2
    data[256:268] = b'Track-Info\r\n'
    data[256 + 0x13] = 2
    data[256 + 0x14:256 + 0x18] = bytes([0, 1, 0x2A, 0xE5])
    data[280:288] = bytes([0, 0, 1, 0, 0x20 if bad_crc else 0,
                            (0x40 if deleted else 0) | (0x20 if bad_crc else 0), 128, 0])
    data[512:640] = PAYLOAD
    return bytes(data)


def main() -> None:
    out = HERE / 'runs'
    out.mkdir(exist_ok=False)
    record: dict = {'schema_version': 1, 'qualified': False, 'origin': 'fresh_execution',
                    'kind': 'direct-reference-controller-research-not-rom-loading',
                    'host': platform.platform(), 'python': platform.python_version(),
                    'commands': [], 'observations': []}

    def save() -> None:
        (out / 'campaign.json').write_text(json.dumps(record, indent=2) + '\n')

    def run(name: str, argv: list[str], timeout: int = 15,
            extra_env: dict[str, str] | None = None) -> subprocess.CompletedProcess[bytes]:
        environment = {'LC_ALL': 'C', 'TZ': 'UTC', **(extra_env or {})}
        started = time.monotonic()
        result = subprocess.run(argv, cwd=HERE, env={**os.environ, **environment},
                                capture_output=True, timeout=timeout, check=False)
        (out / f'{name}.stdout').write_bytes(result.stdout)
        (out / f'{name}.stderr').write_bytes(result.stderr)
        record['commands'].append({'name': name, 'argv': argv, 'cwd': str(HERE),
                                   'environment': environment, 'host_timeout_seconds': timeout,
                                   'exit': result.returncode,
                                   'elapsed_seconds': round(time.monotonic() - started, 3)})
        save()
        result.check_returncode()
        return result

    commit = subprocess.check_output(['git', '-C', str(FUSE), 'rev-parse', 'HEAD'], text=True).strip()
    assert commit == '0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2'
    assert not subprocess.check_output(['git', '-C', str(FUSE), 'status', '--porcelain',
                                        '--untracked-files=no'], text=True).strip()
    paths = ['event.h', 'bitmap.h', *[f'peripherals/disk/{p}' for p in
             ['upd_fdc.c', 'upd_fdc.h', 'fdd.c', 'fdd.h', 'disk.h', 'crc.c', 'crc.h']]]
    record['fuse_source'] = {'commit': commit, 'patched': False,
                             'files': {p: sha(FUSE / p) for p in paths}}
    record['authored_sources'] = {str(p.relative_to(HERE)): sha(p)
                                  for p in [HERE / 'capture.py', HERE / 'fuse_probe.c',
                                            HERE / 'mame-probe.lua', HERE / 'host.c',
                                            *sorted((HERE / 'host').glob('*'))] if p.is_file()}
    clang_path = shutil.which('clang')
    assert clang_path, 'missing prerequisite: clang'
    clang = Path(clang_path).resolve()
    record['compiler'] = {'path': str(clang), 'sha256': sha(clang),
                           'version': run('clang-version', [str(clang), '--version']).stdout.decode()}
    binary = HERE / 'fuse-stock'
    command = [str(clang), '-std=c11', '-O1', '-g', '-fsanitize=address,undefined',
               '-I' + str(HERE / 'host'), '-I' + str(FUSE),
               '-I' + str(FUSE / 'peripherals/disk'), '-include', str(HERE / 'host/config.h'),
               str(HERE / 'host.c'), str(HERE / 'fuse_probe.c'),
               *[str(FUSE / ('peripherals/disk/' + p)) for p in ['upd_fdc.c', 'fdd.c', 'crc.c']],
               '-o', str(binary)]
    run('fuse-build', command, 60)
    record['fuse_executable'] = {'path': str(binary), 'sha256': sha(binary), 'delivery': 'source_build'}
    mame_path = shutil.which('mame')
    assert mame_path, 'missing prerequisite: mame'
    mame = Path(mame_path).resolve()
    record['mame_executable'] = {'path': str(mame), 'sha256': sha(mame),
                                'delivery': 'installed_binary', 'source_commit': 'unknown',
                                'version': run('mame-version', [str(mame), '-help']).stdout.decode().splitlines()[0]}
    record['mame_inspected_source'] = {
        'commit': subprocess.check_output(['git', '-C', str(MAME_SOURCE), 'rev-parse', 'HEAD'], text=True).strip(),
        'executed_source_identity': False,
        'files': {p: sha(MAME_SOURCE / p) for p in ['src/devices/machine/upd765.cpp',
                                                   'src/lib/formats/dsk_dsk.cpp']}}
    firmware = [(Path.home() / f'.emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{i}.rom').read_bytes()
                for i in range(4)]
    assert all(len(page) == 16384 for page in firmware)
    record['firmware_sha256'] = [hashlib.sha256(page).hexdigest() for page in firmware]
    cases = [(opcode, dtl, False) for opcode in [0x46, 0x4C] for dtl in [0, 1, 127, 128, 255]]
    cases += [(opcode, 0, True) for opcode in [0x46, 0x4C]]
    with tempfile.TemporaryDirectory(prefix='198x-dtl-zero-private-') as temporary:
        private = Path(temporary)
        roms = private / 'specpls3'
        roms.mkdir()
        (roms / '40092.ic7').write_bytes(firmware[0] + firmware[1])
        (roms / '40093.ic8').write_bytes(firmware[2] + firmware[3])
        for repeat in [1, 2]:
            for opcode, dtl, bad_crc in cases:
                case = f'{repeat}-{opcode:02x}-dtl{dtl:03}-crc{int(bad_crc)}'
                raw = out / f'{case}.raw'
                disk = out / f'{case}.dsk'
                raw.write_bytes(raw_track(opcode == 0x4C, bad_crc))
                disk.write_bytes(edsk(opcode == 0x4C, bad_crc))
                for engine in ['fuse', 'mame']:
                    name = f'{engine}-{case}'
                    media = raw if engine == 'fuse' else disk
                    digest = sha(media)
                    if engine == 'fuse':
                        argv = [str(binary), str(raw), *[str(b) for b in [opcode, 0, 0, 0, 1, 0, 1, 0x2A, dtl]]]
                        process = run(name, argv)
                        assert not process.stderr, 'reference/sanitizer diagnostic'
                        observed = json.loads(process.stdout)
                    else:
                        observation = private / 'observation.json'
                        observation.unlink(missing_ok=True)
                        argv = [str(mame), 'specpls3', '-bios', 'en', '-rompath', str(private),
                                '-flop1', str(disk), '-video', 'none', '-sound', 'none', '-nothrottle',
                                '-skip_gameinfo', '-noreadconfig', '-nowriteconfig', '-autoboot_delay', '0',
                                '-autoboot_script', str(HERE / 'mame-probe.lua'), '-seconds_to_run', '10']
                        run(name, argv, 45, {'SDL_VIDEODRIVER': 'dummy', 'SDL_AUDIODRIVER': 'dummy',
                                             'PROBE_OUTPUT': str(observation), 'PROBE_DTL': str(dtl),
                                             'PROBE_OPCODE': str(opcode)})
                        observed = json.loads(observation.read_text())
                    assert sha(media) == digest, 'source media changed'
                    assert len(observed['result']) == 7 and observed['idle_msr'] == 128
                    assert observed['recovery_msr'] == 128
                    record['observations'].append({'name': name, 'engine': engine, 'repeat': repeat,
                                                    'opcode': opcode, 'n': 0, 'dtl': dtl, 'bad_crc': bad_crc,
                                                    'media': media.name, 'media_sha256': digest,
                                                    'payload_count': len(observed['data']),
                                                    'payload_is_fixture_prefix': bytes(observed['data']) == PAYLOAD[:len(observed['data'])],
                                                    'observed': observed})
                    save()
                    print(name, len(observed['data']), observed['result'], flush=True)
    record['capture_completed'] = True
    save()
    (out / 'hashes.json').write_text(json.dumps({p.name: sha(p) for p in sorted(out.iterdir())
                                                if p.is_file()}, indent=2) + '\n')


if __name__ == '__main__':
    main()
