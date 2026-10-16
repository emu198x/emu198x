"""Exercise pinned Windows tools: maintained Meteor Storm tape to a running title."""

import argparse
import hashlib
import json
import platform
import re
import struct
import subprocess
import sys
import time
import urllib.request
import zipfile
from pathlib import Path

PINS = Path(__file__).with_name('windows-developer-journey-inputs.json')


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def firmware(path: Path, expected: str) -> bytes:
    data = path.read_bytes()
    if len(data) != 16384 or digest(data) != expected:
        raise ValueError('Genuine 48K firmware identity differs; no runtime qualification')
    return data



def stage_sources(samples: Path, work: Path, pins: dict) -> tuple[Path, list[dict]]:
    """Use exact pinned blobs; permit only checkout CRLF conversion as transport."""
    head = subprocess.run(['git', '-C', str(samples), 'rev-parse', 'HEAD'], capture_output=True, timeout=10, check=True).stdout.decode().strip()
    if head != pins['samples_revision']:
        raise ValueError('Samples checkout is not the pinned revision')
    root = work / 'sources'
    records = []
    for name, expected in pins['files'].items():
        relative = Path(name)
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError('Invalid pinned input path')
        blob = subprocess.run(['git', '-C', str(samples), 'show', f'{head}:{name}'], capture_output=True, timeout=10, check=True).stdout
        checked_out = (samples / relative).read_bytes()
        if digest(blob) != expected:
            raise ValueError(f'Pinned Git blob differs: {name}')
        identical = checked_out == blob
        if not identical and (relative.suffix == '.scr' or checked_out.replace(b'\r\n', b'\n') != blob):
            raise ValueError(f'Sample worktree contains a real change: {name}')
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(blob)
        records.append({'path': name, 'blob_sha256': expected, 'checkout_sha256': digest(checked_out),
                        'checkout_crlf_count': checked_out.count(b'\r\n'),
                        'materialised_sha256': digest(target.read_bytes()),
                        'checkout_conversion': 'identical' if identical else 'CRLF'})
    return root, records


def tape_blocks(data: bytes) -> list[bytes]:
    blocks = []
    offset = 0
    while offset < len(data):
        if offset + 2 > len(data):
            raise ValueError('Truncated TAP length')
        length = struct.unpack_from('<H', data, offset)[0]
        block = data[offset + 2:offset + 2 + length]
        if length < 2 or len(block) != length:
            raise ValueError('Truncated TAP block')
        checksum = 0
        for byte in block:
            checksum ^= byte
        if checksum:
            raise ValueError('TAP checksum differs')
        blocks.append(block)
        offset += length + 2
    return blocks


def verify_package(data: bytes, code: bytes, screen: bytes) -> None:
    blocks = tape_blocks(data)
    if len(blocks) != 6:
        raise ValueError('Expected BASIC, SCREEN$ and CODE header/data pairs')
    for index, (kind, name, address) in enumerate([(0, b'meteor', 10), (3, b'screen', 16384), (3, b'storm', 32768)]):
        header, body = blocks[index * 2:index * 2 + 2]
        if len(header) != 19 or header[0] != 0 or header[1] != kind or header[2:12].rstrip() != name:
            raise ValueError('Unexpected tape header identity')
        if body[0] != 255 or len(body) - 2 != int.from_bytes(header[12:14], 'little'):
            raise ValueError('Header/data length differs')
        if int.from_bytes(header[14:16], 'little') != address:
            raise ValueError('Wrong BASIC autorun or CODE load address')
    if blocks[3][1:-1] != screen or blocks[5][1:-1] != code:
        raise ValueError('Packaged SCREEN$/CODE differs from authored or assembled input')


def title_addresses() -> list[int]:
    # Character row 3, centred 12-character title. Each glyph scanline is in
    # a separate Spectrum bitmap plane; match pixels, not just occupied cells.
    return [0x4000 + ((24 & 0x38) << 2) + (line << 8) + 10 for line in range(8)]


def verify_boot(result: dict, code: bytes, phase: int, rom: bytes) -> None:
    observations = result['observations']
    reads = {row['addr']: bytes(row['bytes']) for row in observations if row['kind'] == 'memory_read'}
    if len(reads) != 10 or reads.get(32768) != code[:64] or len(code) < 64:
        raise ValueError('Assembled entry bytes were not loaded')
    cpu = next(row['registers'] for row in observations if row['kind'] == 'query_cpu')
    pc = int(str(cpu['pc']).removeprefix('$'), 16)
    if not 32768 <= pc < 32768 + len(code) or reads.get(phase) != b'\0':
        raise ValueError('CPU/title state does not show the running program')
    for scan, address in enumerate(title_addresses()):
        expected = bytes(rom[0x3C00 + char * 8 + scan] for char in b'METEOR STORM')
        if reads.get(address) != expected:
            raise ValueError('Rendered title differs from METEOR STORM')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--samples-root', type=Path, required=True)
    parser.add_argument('--firmware', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    if any(args.output.iterdir()):
        raise ValueError('Evidence directory must be fresh')
    pins = json.loads(PINS.read_text(encoding='utf-8'))
    report = {'qualified': False, 'status': 'running', 'host': {'system': platform.system(), 'release': platform.release(), 'machine': platform.machine(), 'python': platform.python_version()}, 'samples_revision': pins['samples_revision'], 'tools': [], 'commands': []}
    output = args.output / 'report.json'

    def retain() -> None:
        output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')

    def run(label: str, argv: list[str], cwd: Path, timeout: int = 60) -> str:
        result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, encoding='utf-8', timeout=timeout, check=False)
        (args.output / f'{label}.stdout').write_text(result.stdout, encoding='utf-8')
        (args.output / f'{label}.stderr').write_text(result.stderr, encoding='utf-8')
        report['commands'].append({'label': label, 'argv': argv, 'exit_code': result.returncode, 'timeout_seconds': timeout})
        retain()
        if result.returncode:
            raise RuntimeError(f'{label} exited {result.returncode}; see retained output')
        return result.stdout

    retain()
    try:
        if sys.platform != 'win32' or platform.machine().lower() not in {'amd64', 'x86_64'}:
            raise ValueError('This qualification requires a native x86-64 Windows runner')
        rom = firmware(args.firmware, pins['firmware_sha256'])
        report['firmware_sha256'] = digest(rom)
        staged, report['source_materialisation'] = stage_sources(args.samples_root, args.work, pins)
        report['source_hashes'] = pins['files']
        tools = args.work / 'tools'
        tools.mkdir(parents=True, exist_ok=True)
        binaries = {}
        for tool in pins['tools']:
            archive = args.work / tool['archive']
            started = time.monotonic()
            with urllib.request.urlopen(tool['url'], timeout=30) as response, archive.open('wb') as destination:
                size = 0
                while chunk := response.read(1024 * 1024):
                    size += len(chunk)
                    if size > 128 * 1024 * 1024 or time.monotonic() - started > 60:
                        raise ValueError('Archive download exceeds bounded size/time')
                    destination.write(chunk)
            if digest(archive.read_bytes()) != tool['sha256']:
                raise ValueError('Published Windows archive checksum differs')
            filename = tool['name'] + '.exe'
            with zipfile.ZipFile(archive) as zipped:
                entries = [name for name in zipped.namelist() if Path(name).name == filename]
                if len(entries) != 1 or zipped.getinfo(entries[0]).file_size > 128 * 1024 * 1024:
                    raise ValueError('Archive does not identify one bounded executable')
                executable = zipped.read(entries[0])
            if not executable.startswith(b'MZ'):
                raise ValueError('Expected Windows PE executable')
            binary = tools / filename
            binary.write_bytes(executable)
            binaries[tool['name']] = str(binary)
            report['tools'].append({**tool, 'executable_sha256': digest(executable)})
            run(tool['name'] + '-help', [str(binary), '--help'], args.work)
        source = staged / pins['sample_path'] / 'checkpoints/loading-screen'
        build = args.work / 'build'
        build.mkdir()
        asm = binaries['asm198x']
        run('basic', [binaries['build198x'], 'basic', str(source / 'loader.bas'), '--machine', 'sinclair-zx-spectrum', '-o', str(build / 'meteor.tap'), '--name', 'meteor'], args.work)
        for name, filename in [('screen', 'screen.asm'), ('storm', 'meteor-storm.asm')]:
            run(name, [asm, '--dialect', 'pasmo', '--cpu', 'z80', '--tap', str(source / filename), '-o', name], build)
        run('raw-code', [asm, '--dialect', 'pasmo', '--cpu', 'z80', '--sym=storm.sym', str(source / 'meteor-storm.asm'), '-o', 'program.bin'], build)
        symbols = {m[1]: int(m[2], 16) for line in (build / 'storm.sym').read_text(encoding='utf-8').splitlines() if (m := re.fullmatch(r'(\w+) = \$([0-9a-fA-F]+)', line))}
        code = (build / 'program.bin').read_bytes()
        data = b''.join((build / name).read_bytes() for name in ['meteor.tap', 'screen', 'storm'])
        verify_package(data, code, (source / 'meteor-storm.scr').read_bytes())
        tape = args.output / 'meteor-storm.tap'
        tape.write_bytes(data)
        (args.output / 'program.bin').write_bytes(code)
        (args.output / 'symbols.json').write_text(json.dumps(symbols, indent=2) + '\n', encoding='utf-8')
        report['package'] = {'sha256': digest(data), 'code_sha256': digest(code), 'blocks': 6}

        def boot(label: str, medium: Path, capture: bool) -> dict:
            steps = [{'action': 'load_media', 'slot': 'tape-1', 'kind': 'tape', 'path': str(medium)}, {'action': 'autoload_tape', 'slot': 'tape-1', 'max_boot_frames': 500}, {'action': 'run_frames', 'frames': pins['frames']}, {'action': 'memory_read', 'addr': 32768, 'len': 64}, {'action': 'memory_read', 'addr': symbols['phase'], 'len': 1}]
            steps += [{'action': 'memory_read', 'addr': address, 'len': 12} for address in title_addresses()]
            steps.append({'action': 'query_cpu'})
            if capture:
                steps.append({'action': 'save_screenshot', 'path': str(args.output / 'title.png')})
            session = args.output / f'{label}-session.json'
            session.write_text(json.dumps(steps, indent=2) + '\n', encoding='utf-8')
            text = run(label, [binaries['emu198x-spectrum'], '--machine', 'spectrum_48k', '--rom', str(args.firmware), '--script', str(session)], args.work, timeout=180)
            return json.loads(text)

        verify_boot(boot('positive', tape, True), code, symbols['phase'], rom)
        truncated = args.output / 'missing-code.tap'
        truncated.write_bytes((build / 'meteor.tap').read_bytes() + (build / 'screen').read_bytes())
        try:
            verify_boot(boot('missing-code', truncated, False), code, symbols['phase'], rom)
        except ValueError as error:
            report['missing_code_rejected'] = str(error)
        else:
            raise ValueError('Tape missing CODE incorrectly qualified')
        report.update(qualified=True, status='passed', checks=['six-block package payloads match inputs', 'entry bytes loaded through ROM tape path', 'PC and phase show title execution', 'rendered title matches ROM glyphs', 'real missing-CODE tape rejected'])
        retain()
        print('Windows Build/Asm/Emu handoff qualified; real missing-CODE control rejected')
    except Exception as error:
        report.update(status='failed', error=str(error))
        retain()
        raise


if __name__ == '__main__':
    main()
