#!/usr/bin/env python3
"""Install this build's generated Homebrew formulae on a disposable CI runner."""
import functools
import http.server
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def main() -> None:
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise SystemExit('Run only on disposable GitHub Actions runners; this creates a Homebrew tap.')
    manifest = json.loads(Path(sys.argv[1]).read_text())
    target = sys.argv[2]
    if target != f'{os.uname().machine}-unknown-linux-gnu':
        raise SystemExit('Package target must match the native Linux runner')
    formulae = [a for r in manifest['releases'] for a in r['artifacts'] if a.endswith('.rb')]
    if not formulae or len(formulae) != len(set(formulae)):
        raise SystemExit('Expected a nonempty, unique set of formulae')
    dist = Path('target/distrib').resolve()
    for name in formulae:
        if Path(name).name != name or not (dist / name).is_file():
            raise SystemExit(f'Missing generated formula: {name}')
    run('brew', 'tap-new', '198x/package-test')
    tap = Path(run('brew', '--repository', '198x/package-test', capture_output=True).stdout.strip()) / 'Formula'
    with tempfile.TemporaryDirectory(prefix='linux-packages-') as tmp:
        directory = Path(tmp)
        handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(directory))
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        base = f'http://127.0.0.1:{server.server_port}'
        try:
            for name in formulae:
                text = (dist / name).read_text()
                archives = re.findall(r'url "(https://[^"\n]+/([^/"\n]+))"', text)
                selected = [file for _, file in archives if target in file]
                if len(selected) != 1:
                    raise SystemExit(f'{name}: expected one archive for {target}, got {selected}')
                archive = selected[0]
                shutil.copyfile(dist / archive, directory / archive)
                # Change only the download location; retain CPU/OS selection,
                # dependencies, checksums and install code from cargo-dist.
                for url, file in archives:
                    text = text.replace(url, f'{base}/{file}')
                (tap / name).write_text(text)
                formula = f'198x/package-test/{Path(name).stem}'
                # Prove Homebrew rejects damaged bytes before trusting a pass.
                original = (directory / archive).read_bytes()
                (directory / archive).write_bytes(b'invalid release archive')
                failed = subprocess.run(['brew', 'fetch', '--force', formula], text=True, capture_output=True)
                if failed.returncode == 0 or 'SHA256 mismatch' not in failed.stdout + failed.stderr:
                    raise SystemExit(f'Checksum rejection was not exercised:\n{failed.stdout}\n{failed.stderr}')
                (directory / archive).write_bytes(original)
                run('brew', 'fetch', '--force', formula)
                run('brew', 'install', formula)
                prefix = run('brew', '--prefix', formula, capture_output=True).stdout.strip()
                run('bash', 'scripts/smoke-package.sh', Path(name).stem, f'{prefix}/bin', str(directory))
                run('brew', 'uninstall', formula)
        finally:
            server.shutdown()


if __name__ == '__main__':
    main()
