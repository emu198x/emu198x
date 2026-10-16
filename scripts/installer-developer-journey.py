"""Audit public installation routes on disposable native hosted runners."""

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

PINS = Path(__file__).with_name('installer-journey-inputs.json')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', choices=['macos-intel', 'macos-arm', 'windows'], required=True)
    parser.add_argument('--samples-root', type=Path, required=True)
    parser.add_argument('--firmware', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    config = json.loads(PINS.read_text())
    pins = config['hosts'][args.host]
    expected = 'Windows' if args.host == 'windows' else 'Darwin'
    machine = 'arm64' if args.host == 'macos-arm' else 'x86_64'
    if platform.system() != expected or platform.machine().lower().replace('amd64', 'x86_64') != machine:
        raise ValueError('Not the declared native host')
    args.output.mkdir()
    args.work.mkdir()
    report = {'qualified': False, 'host': {'system': platform.system(), 'machine': platform.machine(), 'release': platform.release()}, 'commands': [], 'installed': {}, 'cleanup': {}, 'limits': []}

    def retain() -> None:
        (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')

    def run(label: str, argv: list[str], timeout: int = 180, allow_failure: bool = False) -> subprocess.CompletedProcess:
        result = subprocess.run(argv, capture_output=True, timeout=timeout, check=False)
        for stream in ['stdout', 'stderr']:
            (args.output / f'{label}.{stream}').write_bytes(getattr(result, stream))
        report['commands'].append({'label': label, 'argv': argv, 'exit_code': result.returncode, 'timeout_seconds': timeout})
        retain()
        if result.returncode and not allow_failure:
            raise ValueError(f'{label} exited {result.returncode}')
        return result

    def download(url: str, sha: str, destination: Path) -> None:
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read(128 * 1024 * 1024 + 1)
        if len(data) > 128 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != sha:
            raise ValueError('Public installer/archive identity differs')
        destination.write_bytes(data)

    def ps(label: str, code: str) -> str:
        return run(label, ['pwsh', '-NoProfile', '-NonInteractive', '-Command', code]).stdout.decode('utf-8-sig').strip()

    retain()
    names = [t['name'] for t in pins['tools']]
    baseline = {name: shutil.which(name) for name in names}
    report['baseline'] = baseline
    retain()
    if any(baseline.values()):
        raise ValueError('A family executable was already installed; this is not a clean family install')
    installed = {}
    brew_installed = []
    receipt_dirs = []
    previous_user_path = None
    user_path_captured = False
    try:
        if args.host.startswith('macos'):
            report['limits'].append('Hosted macOS image with preinstalled Homebrew and developer prerequisites; Homebrew bootstrap and consumer desktop setup are untested')
            os.environ['HOMEBREW_NO_AUTO_UPDATE'] = '1'
            os.environ['HOMEBREW_NO_INSTALL_CLEANUP'] = '1'
            run('brew-version', ['brew', '--version'])
            before = run('brew-before', ['brew', 'list', '--formula', '--full-name']).stdout.decode().splitlines()
            if any(row.split('/')[-1] in names for row in before):
                raise ValueError('Family formula already exists, including unlinked installations')
            for name in names:
                tap = config['taps'][name]
                formula = tap['formula']
                org_tap = formula.rsplit('/', 1)[0]
                run(name + '-tap', ['brew', 'tap', org_tap])
                repo = Path(run(name + '-tap-path', ['brew', '--repo', org_tap]).stdout.decode().strip())
                actual_revision = run(name + '-tap-revision', ['git', '-C', str(repo), 'rev-parse', 'HEAD']).stdout.decode().strip()
                formula_bytes = (repo / f'Formula/{name}.rb').read_bytes()
                actual_sha = hashlib.sha256(formula_bytes).hexdigest()
                (args.output / f'{name}.rb').write_bytes(formula_bytes)
                if actual_sha != tap['formula_sha256']:
                    raise ValueError('Public formula changed after selection; record new pins before qualification')
                result = run(name + '-install', ['brew', 'install', formula], timeout=600, allow_failure=True)
                if result.returncode and b'brew trust' in result.stdout + result.stderr:
                    run(name + '-trust', ['brew', 'trust', formula])
                    result = run(name + '-install-after-trust', ['brew', 'install', formula], timeout=600)
                if result.returncode:
                    raise ValueError(name + ' installer failed')
                brew_installed.append(formula)
                path = shutil.which(name)
                if path is None:
                    raise ValueError('Installed tool is absent from PATH: ' + name)
                installed[name] = path
                report['installed'][name] = {'route': 'Homebrew', 'path': path, 'sha256': hashlib.sha256(Path(path).read_bytes()).hexdigest(), 'formula_revision': actual_revision, 'formula_sha256': actual_sha}
                run(name + '-version', [path, '--version'])
                run(name + '-info', ['brew', 'info', '--json=v2', formula])
        else:
            report['limits'] += ['Disposable Windows Server image with developer prerequisites; Windows client GUI and prerequisite-free setup are untested', 'Emu has an advertised ZIP route, no vendor installer; ZIP execution is not installer qualification', 'No removal command is documented by the selected PowerShell installers; cleanup is explicit file/receipt removal and user PATH restoration']
            previous_user_path = json.loads(ps('user-path-before', "ConvertTo-Json -Compress -InputObject ([Environment]::GetEnvironmentVariable('Path','User'))"))
            user_path_captured = True
            for name in ['asm198x', 'build198x']:
                item = config['installers'][name]
                script = args.work / (name + '-installer.ps1')
                receipt_dir = Path(os.environ.get('XDG_CONFIG_HOME', os.environ['LOCALAPPDATA'])) / name
                if receipt_dir.exists():
                    raise ValueError('Installer receipt directory already exists: ' + name)
                receipt_dirs.append(receipt_dir)
                download(item['url'], item['sha256'], script)
                run(name + '-install', ['pwsh', '-NoProfile', '-NonInteractive', '-File', str(script)])
                receipt = json.loads((receipt_dir / (name + '-receipt.json')).read_text(encoding='utf-8-sig'))
                (args.output / (name + '-receipt.json')).write_text(json.dumps(receipt, indent=2) + '\n')
                user_path = json.loads(ps(name + '-user-path-after', "ConvertTo-Json -Compress -InputObject ([Environment]::GetEnvironmentVariable('Path','User'))"))
                machine_path = json.loads(ps(name + '-machine-path', "ConvertTo-Json -Compress -InputObject ([Environment]::GetEnvironmentVariable('Path','Machine'))"))
                os.environ['PATH'] = (machine_path or '') + os.pathsep + (user_path or '')
                path = shutil.which(name)
                if path is None:
                    raise ValueError('Fresh machine/user PATH does not resolve ' + name)
                installed[name] = path
                report['installed'][name] = {'route': 'public PowerShell installer', 'path': path, 'sha256': hashlib.sha256(Path(path).read_bytes()).hexdigest(), 'installer': item, 'receipt_version': receipt['version'], 'user_path_after': user_path}
                run(name + '-version', [path, '--version'])
            tool = next(t for t in pins['tools'] if t['name'] == 'emu198x-spectrum')
            archive = args.work / tool['archive']
            download(tool['url'], tool['sha256'], archive)
            with zipfile.ZipFile(archive) as zipped:
                members = [m for m in zipped.infolist() if Path(m.filename).name == 'emu198x-spectrum.exe']
                if len(members) != 1 or members[0].file_size > 128 * 1024 * 1024:
                    raise ValueError('Missing bounded emulator in advertised ZIP')
                binary = args.work / 'emu198x-spectrum.exe'
                binary.write_bytes(zipped.read(members[0]))
            installed['emu198x-spectrum'] = str(binary)
            report['installed']['emu198x-spectrum'] = {'route': 'advertised ZIP, not an installer', 'path': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), **tool}
            run('emu198x-spectrum-version', [str(binary), '--version'])
        retain()
        selected = args.output / 'selected-inputs.json'
        selected.write_text(json.dumps(pins, indent=2) + '\n')
        mapping = args.output / 'installed-tools.json'
        mapping.write_text(json.dumps(installed, indent=2) + '\n')
        run('installed-journey', [sys.executable, str(Path(__file__).with_name('windows-developer-journey.py')), '--pins', str(selected), '--installed-tools', str(mapping), '--samples-root', str(args.samples_root), '--firmware', str(args.firmware), '--work', str(args.work / 'journey'), '--output', str(args.output / 'journey')], timeout=600)
        report['qualified'] = True
    finally:
        for formula in reversed(brew_installed):
            run(formula.split('/')[-1] + '-uninstall', ['brew', 'uninstall', formula])
            name = formula.split('/')[-1]
            if shutil.which(name) is not None:
                raise ValueError('Executable remains after uninstall: ' + name)
            report['cleanup'][name] = 'Homebrew uninstall and PATH absence verified'
        if args.host == 'windows':
            for name, path in installed.items():
                Path(path).unlink(missing_ok=True)
                if Path(path).exists():
                    raise ValueError('Installed file cleanup failed')
                report['cleanup'][name] = 'File removed; no documented vendor uninstaller'
            for folder in receipt_dirs:
                if folder.exists():
                    shutil.rmtree(folder)
            if user_path_captured:
                literal = '$null' if previous_user_path is None else "'" + previous_user_path.replace("'", "''") + "'"
                ps('restore-user-path', "[Environment]::SetEnvironmentVariable('Path'," + literal + ",'User')")
                actual = json.loads(ps('verify-restored-path', "ConvertTo-Json -Compress -InputObject ([Environment]::GetEnvironmentVariable('Path','User'))"))
                if actual != previous_user_path:
                    raise ValueError('User PATH restoration failed')
                report['cleanup']['user_path_restored'] = True
        retain()


if __name__ == '__main__':
    main()
