"""Repeat the retained Amiga package/cold-boot fixture on native Intel macOS."""

import argparse
import copy
import hashlib
import importlib.util
import json
import platform
import shutil
import subprocess
import sys
import tarfile
import urllib.request
from pathlib import Path


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ValueError('Missing retained verifier')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixtures', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--firmware', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != 'darwin' or platform.machine() != 'x86_64':
        raise ValueError('Requires native Intel macOS')
    if hashlib.sha256(args.firmware.read_bytes()).hexdigest() != 'ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53':
        raise ValueError('Kickstart identity differs')
    args.output.mkdir()
    report = {'qualified': False, 'host': {'system': platform.system(), 'release': platform.release(), 'machine': platform.machine()}, 'commands': []}

    def retain() -> None:
        (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

    def run(label: str, argv: list[str], timeout: int = 60, expected: int = 0) -> None:
        result = subprocess.run(argv, cwd=args.output, capture_output=True, timeout=timeout, check=False)
        for stream in ['stdout', 'stderr']:
            (args.output / f'{label}.{stream}').write_bytes(getattr(result, stream))
        report['commands'].append({'label': label, 'argv': argv, 'exit_code': result.returncode, 'timeout_seconds': timeout})
        retain()
        if result.returncode != expected:
            raise ValueError(f'{label} exited {result.returncode}')

    retain()
    source = json.loads((args.fixtures / 'source.json').read_text())
    for name, sha in source['files'].items():
        data = (args.fixtures / name).read_bytes()
        if hashlib.sha256(data).hexdigest() != sha:
            raise ValueError('Retained fixture differs: ' + name)
        (args.output / name).write_bytes(data)
    url = 'https://github.com/emu198x/emu198x/releases/download/v0.35.2/emu198x-amiga-x86_64-apple-darwin.tar.gz'
    with urllib.request.urlopen(url, timeout=30) as response:
        data = response.read(128 * 1024 * 1024 + 1)
    if len(data) > 128 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != '9e763aeea0f9f95ee4964a442b3071443530e34966f42a4a42a0bc728f42380e':
        raise ValueError('Published Amiga archive differs')
    archive = args.work / 'amiga.tar.gz'
    archive.write_bytes(data)
    with tarfile.open(archive) as packed:
        members = [m for m in packed.getmembers() if m.isfile() and Path(m.name).name == 'emu198x-amiga']
        if len(members) != 1 or members[0].size > 128 * 1024 * 1024:
            raise ValueError('Expected one bounded emulator')
        stream = packed.extractfile(members[0])
        if stream is None:
            raise ValueError('Missing emulator')
        executable = stream.read()
    journey = load('journey', Path(__file__).with_name('windows-developer-journey.py'))
    journey.executable_identity(executable, 'x86_64-apple-darwin')
    binary = args.work / 'tools/emu198x-amiga'
    binary.write_bytes(executable)
    binary.chmod(0o755)
    report['emulator'] = {'release': 'v0.35.2', 'url': url, 'archive_sha256': hashlib.sha256(data).hexdigest(), 'executable_sha256': hashlib.sha256(executable).hexdigest()}
    tools = args.work / 'tools'
    run('make', ['make', '-B', f'ASM={tools / "asm198x"}', f'BUILD={tools / "build198x"}'])
    run('verify', [str(tools / 'build198x'), 'adf', 'verify', 'demo.adf', '--format', 'json'])
    for name, sha in source['rebuild_sha256'].items():
        if hashlib.sha256((args.output / name).read_bytes()).hexdigest() != sha:
            raise ValueError('Canonical macOS/Linux product differs: ' + name)
    (args.output / 'truncated.adf').write_bytes((args.output / 'demo.adf').read_bytes()[:4096])
    run('truncated', [str(tools / 'build198x'), 'adf', 'verify', 'truncated.adf', '--format', 'json'], expected=1)
    run('runtime-runner', [sys.executable, str(args.fixtures / 'run.py'), '--emulator', str(binary), '--kickstart', str(args.firmware), '--disk', str(args.output / 'demo.adf'), '--output', str(args.output)], timeout=260)
    checker = load('retained_amiga', args.fixtures / 'verify.py')
    runtime = json.loads((args.output / 'runtime.json').read_text())
    checker.runtime_check(args.output, runtime)
    bad = copy.deepcopy(runtime)
    bad['colours']['entries'] = [e for e in bad['colours']['entries'] if checker.hex_value(e['value']) != 0x50]
    bad['colours']['filtered_total'] -= 1
    bad['colours']['returned'] -= 1
    try:
        checker.runtime_check(args.output, bad)
    except ValueError:
        pass
    else:
        raise ValueError('Missing completion incorrectly qualified')
    # Keep provenance beside the outputs; copied helper code remains owned by the umbrella.
    shutil.copyfile(args.fixtures / 'source.json', args.output / 'source.json')
    report.update(qualified=True, canonical_products_match=True, missing_completion_rejected=True)
    retain()
    print('Intel macOS Amiga build, cold boot and completion agree with retained products')


if __name__ == '__main__':
    main()
