"""Exercise pinned native tools: maintained Meteor Storm tape to a running title."""

import argparse
import copy
import hashlib
import json
import platform
import re
import struct
import subprocess
import sys
import tarfile
import time
import urllib.request
import zipfile
from pathlib import Path

PINS = Path(__file__).with_name('windows-developer-journey-inputs.json')
FIXTURES = Path(__file__).with_name('developer-journey-fixtures')


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


# Source-stop invariants reused from umbrella audit 5d9c4927,
# docs/audits/2026-10-16-spectrum-source-debugging/verify.py.
def check_source_debug(root: Path, runtime: dict) -> None:
    """Cross-check actual source, tape, running bytes and source stop."""
    source = (root / 'first-program.asm').read_text(encoding='utf-8').splitlines()
    payload = bytes.fromhex('3e02d3fe18fe')
    tape = (root / 'first-program.tap').read_bytes()
    blocks = []
    position = 0
    while position < len(tape):
        size = int.from_bytes(tape[position:position + 2], 'little')
        block = tape[position + 2:position + 2 + size]
        if not size or len(block) != size:
            raise ValueError('truncated tape')
        blocks.append(block)
        position += size + 2
    if position != len(tape) or blocks[-1][1:-1] != payload:
        raise ValueError('unexpected tape payload')
    rows = runtime['observations']
    memory = next(row for row in rows if row['kind'] == 'memory_read')
    if memory['addr'] != 32768 or bytes(memory['bytes']) != payload:
        raise ValueError('loaded memory differs from tape')
    instructions = next(row['instructions'] for row in rows if row['kind'] == 'disasm')
    expected = [('ld a,2', 32768), ('out ($FE),a', 32770), ('jr hold', 32772)]
    if len(instructions) != len(expected):
        raise ValueError('incomplete disassembly')
    for instruction, (text, address) in zip(instructions, expected, strict=True):
        line = source.index(' ' + text) + 1
        if instruction['addr'] != address or instruction['source'] != {'file': 'first-program.asm', 'line': line}:
            raise ValueError('source/disassembly mapping differs')
        if bytes(instruction['raw']) != payload[address - 32768:address - 32768 + 2]:
            raise ValueError('disassembly bytes differ from tape')
    symbols = {r['name']: r['addr'] for r in rows if r['kind'] == 'debug_symbol'}
    if symbols != {'start': 32768, 'hold': 32772}:
        raise ValueError('symbol addresses differ from program')
    stops = [r for r in rows if r['kind'] == 'run_until_line']
    loop_line = source.index(' jr hold') + 1
    for stop in stops[:2]:
        if not stop['reached'] or stop['pc'] != symbols['hold'] or stop['addr'] != symbols['hold']:
            raise ValueError('source stop did not reach hold')
        if stop['stopped_at'] != {'file': 'first-program.asm', 'line': loop_line}:
            raise ValueError('wrong stopped source location')
    if len(stops) != 4 or stops[0]['steps'] < 1:
        raise ValueError('missing nontrivial source-stop evidence')
    if stops[2]['addr'] is not None or stops[2]['reached'] or stops[2]['steps'] or stops[2]['ticks']:
        raise ValueError('comment-line request moved the machine')
    if stops[3]['reached'] or stops[3]['addr'] != symbols['start'] or stops[3]['steps'] != 16:
        raise ValueError('bounded miss was not reported')
    if stops[3]['stopped_at'] != stops[0]['stopped_at']:
        raise ValueError('bounded miss lost the actual source location')
    cpu = next(r['registers'] for r in rows if r['kind'] == 'query_cpu')
    if int(cpu['pc'].removeprefix('$'), 16) != symbols['hold']:
        raise ValueError('CPU PC differs from source stop')
    step = next(r for r in rows if r['kind'] == 'step')
    if step['instructions'] != 1 or step['pc_trace'] != [symbols['hold']] or step['pc'] != symbols['hold']:
        raise ValueError('loop step differs from program')



def executable_identity(executable: bytes, target: str) -> int | None:
    if target == 'x86_64-pc-windows-msvc':
        if not executable.startswith(b'MZ'):
            raise ValueError('Expected Windows PE executable')
        pe = struct.unpack_from('<I', executable, 0x3C)[0]
        if executable[pe:pe + 4] != b'PE\0\0' or struct.unpack_from('<H', executable, pe + 4)[0] != 0x8664:
            raise ValueError('Expected native x86-64 PE image')
        if struct.unpack_from('<H', executable, pe + 24)[0] != 0x20B:
            raise ValueError('Expected PE32+ optional header')
        return struct.unpack_from('<Q', executable, pe + 24 + 72)[0]
    if target == 'x86_64-unknown-linux-gnu':
        if executable[:6] != b'\x7fELF\x02\x01' or struct.unpack_from('<H', executable, 18)[0] != 62:
            raise ValueError('Expected native x86-64 little-endian ELF image')
        return None
    if target in {'x86_64-apple-darwin', 'aarch64-apple-darwin'}:
        cpu = 0x01000007 if target.startswith('x86_64') else 0x0100000C
        if executable[:4] != b'\xcf\xfa\xed\xfe' or struct.unpack_from('<I', executable, 4)[0] != cpu:
            raise ValueError('Expected native 64-bit Mach-O image')
        return None
    raise ValueError('Unsupported native target')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pins', type=Path, default=PINS)
    parser.add_argument('--installed-tools', type=Path, help='JSON map of actual installed executable paths; every entry must match public bytes')
    parser.add_argument('--samples-root', type=Path, required=True)
    parser.add_argument('--firmware', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    if any(args.output.iterdir()):
        raise ValueError('Evidence directory must be fresh')
    pins = json.loads(args.pins.read_text(encoding='utf-8'))
    report = {'qualified': False, 'status': 'running', 'host': {'system': platform.system(), 'release': platform.release(), 'machine': platform.machine(), 'python': platform.python_version()}, 'samples_revision': pins['samples_revision'], 'tools': [], 'commands': []}
    output = args.output / 'report.json'

    def retain() -> None:
        output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')

    def run(label: str, argv: list[str], cwd: Path, timeout: int = 60, expected_exit: int = 0) -> str:
        result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, encoding='utf-8', timeout=timeout, check=False)
        (args.output / f'{label}.stdout').write_text(result.stdout, encoding='utf-8')
        (args.output / f'{label}.stderr').write_text(result.stderr, encoding='utf-8')
        report['commands'].append({'label': label, 'argv': argv, 'exit_code': result.returncode, 'timeout_seconds': timeout})
        retain()
        if result.returncode != expected_exit:
            raise RuntimeError(f'{label} exited {result.returncode}; see retained output')
        return result.stdout

    retain()
    try:
        target = pins.get('target', 'x86_64-pc-windows-msvc')
        expected_os = {'x86_64-pc-windows-msvc': 'win32', 'x86_64-unknown-linux-gnu': 'linux', 'x86_64-apple-darwin': 'darwin', 'aarch64-apple-darwin': 'darwin'}.get(target)
        machines = {'arm64', 'aarch64'} if target.startswith('aarch64') else {'amd64', 'x86_64'}
        if expected_os is None or sys.platform != expected_os or platform.machine().lower() not in machines:
            raise ValueError('Journey requires the declared native host')
        report['target'] = target
        rom = firmware(args.firmware, pins['firmware_sha256'])
        report['firmware_sha256'] = digest(rom)
        staged, report['source_materialisation'] = stage_sources(args.samples_root, args.work, pins)
        report['source_hashes'] = pins['files']
        installed = json.loads(args.installed_tools.read_text(encoding='utf-8')) if args.installed_tools else None
        if installed is not None and set(installed) != {tool['name'] for tool in pins['tools']}:
            raise ValueError('Installed tool map must contain exactly the pinned tools')
        report['installation_mode'] = 'installed-executables' if installed is not None else 'archive-extraction'
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
                raise ValueError('Published native archive checksum differs')
            filename = tool['name'] + ('.exe' if expected_os == 'win32' else '')
            if expected_os == 'win32':
                with zipfile.ZipFile(archive) as zipped:
                    entries = [name for name in zipped.namelist() if Path(name).name == filename]
                    if len(entries) != 1 or zipped.getinfo(entries[0]).file_size > 128 * 1024 * 1024:
                        raise ValueError('Archive does not identify one bounded executable')
                    executable = zipped.read(entries[0])
            else:
                with tarfile.open(archive) as packed:
                    entries = [m for m in packed.getmembers() if m.isfile() and Path(m.name).name == filename]
                    if len(entries) != 1 or entries[0].size > 128 * 1024 * 1024:
                        raise ValueError('Archive does not identify one bounded executable')
                    stream = packed.extractfile(entries[0])
                    if stream is None:
                        raise ValueError('Missing executable stream')
                    executable = stream.read()
            stack_reserve = executable_identity(executable, target)
            binary = tools / filename
            binary.write_bytes(executable)
            if expected_os != 'win32':
                binary.chmod(0o755)
            if installed is not None:
                binary = Path(installed[tool['name']])
                if binary.read_bytes() != executable:
                    raise ValueError('Installed executable differs from the pinned public archive: ' + tool['name'])
            binaries[tool['name']] = str(binary)
            report['tools'].append({**tool, 'executable_sha256': digest(executable), 'pe_stack_reserve_bytes': stack_reserve})
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
        startup = args.output / 'startup-session.json'
        startup.write_text(json.dumps([{'action': 'query_cpu'}]), encoding='utf-8')
        run('startup', [binaries['emu198x-spectrum'], '--machine', 'spectrum_48k', '--rom', str(args.firmware), '--script', str(startup)], args.work, timeout=30)


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
        debug = args.output / 'source-debug'
        debug.mkdir()
        original = (FIXTURES / 'first-program.asm').read_bytes().replace(b'\r\n', b'\n')
        if digest(original) != pins['source_debug_fixture_sha256']:
            raise ValueError('First lesson fixture differs from retained source')
        (debug / 'first-program.asm').write_bytes(original)
        session_bytes = (FIXTURES / 'source-session.json').read_bytes().replace(b'\r\n', b'\n')
        if digest(session_bytes) != pins['source_debug_session_sha256']:
            raise ValueError('Source-debug session differs from retained task')
        session = debug / 'session.json'
        session.write_bytes(session_bytes)
        run('debug-asm', [asm, '--dialect', 'pasmo', '--cpu', 'z80', '--tapbas', '--debug', '--message-format=json', 'first-program.asm', '-o', 'first-program.tap'], debug)
        observed = json.loads(run('debug-runtime', [binaries['emu198x-spectrum'], '--machine', 'spectrum_48k', '--rom', str(args.firmware), '--script', str(session)], debug, timeout=120))
        check_source_debug(debug, observed)
        bad = copy.deepcopy(observed)
        next(o for o in bad['observations'] if o['kind'] == 'run_until_line')['stopped_at']['line'] = 7
        try:
            check_source_debug(debug, bad)
        except ValueError:
            pass
        else:
            raise ValueError('Wrong source location incorrectly passed')
        missing_debug = debug / 'missing-sidecar.json'
        missing_debug.write_text(json.dumps([{'action': 'run_until_line', 'file': 'first-program.asm', 'line': 9, 'max_steps': 16}]), encoding='utf-8')
        run('missing-sidecar', [binaries['emu198x-spectrum'], '--machine', 'spectrum_48k', '--rom', str(args.firmware), '--script', str(missing_debug)], debug, expected_exit=1)
        error = (args.output / 'missing-sidecar.stderr').read_text(encoding='utf-8')
        if 'needs debug info' not in error or 'load_debug_info' not in error:
            raise ValueError('Missing sidecar failed for an unexpected reason')
        report['source_debug'] = {'verified': True, 'fixture_sha256': digest(original), 'missing_sidecar_rejected': True, 'wrong_location_rejected': True, 'source_line': 9, 'pc': 32772}
        report.update(qualified=True, status='passed', checks=['six-block package payloads match inputs', 'entry bytes loaded through ROM tape path', 'PC and phase show title execution', 'rendered title matches ROM glyphs', 'real missing-CODE tape rejected'])
        retain()
        print(f'{target} Build/Asm/Emu handoff and source debugging qualified; real controls rejected')
    except Exception as error:
        report.update(status='failed', error=str(error))
        retain()
        raise


if __name__ == '__main__':
    main()
