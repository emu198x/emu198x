"""Verify a published Spectrum archive and producer receipt for v0.35.2."""

import argparse
import copy
import hashlib
import importlib.util
import io
import json
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path


def gh(*args: str) -> dict | list:
    return json.loads(subprocess.check_output(['gh', 'api', *args], timeout=60))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--producer-run', required=True)
    parser.add_argument('--target', choices=['x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu'], default='x86_64-pc-windows-msvc')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    repo = 'emu198x/emu198x'
    tag = 'v0.35.2'
    package = 'emu198x-spectrum'
    target = args.target
    source = subprocess.check_output(['git', 'rev-parse', tag + '^{commit}'], text=True, timeout=10).strip()
    if source != '42e28e284d2b3120e850aea66bfcf097b6119afa':
        raise ValueError('Tag differs from the reviewed release merge')
    lock = subprocess.check_output(['git', 'show', source + ':Cargo.lock'], timeout=10)
    validator = Path('scripts/source_lock.py')
    released_validator = subprocess.check_output(['git', 'show', source + ':scripts/source_lock.py'], timeout=10)
    if validator.read_bytes() != released_validator:
        raise ValueError('Local validator differs from the released producer validator')
    spec = importlib.util.spec_from_file_location('release_source_lock', validator)
    if spec is None or spec.loader is None:
        raise ValueError('Missing producer validator')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    run = gh(f'repos/{repo}/actions/runs/{args.producer_run}')
    if run['head_sha'] != source or run['event'] != 'push' or run['conclusion'] != 'success':
        raise ValueError('The actual tag producer has not completed successfully')
    jobs = gh(f'repos/{repo}/actions/runs/{args.producer_run}/jobs?per_page=100')['jobs']
    required = {f'custom-qualify-packages / Verify source lock ({t})' for t in module.TARGETS}
    required |= {f'custom-qualify-packages / linux-install / Install and exercise ({t})' for t in module.TARGETS if 'linux' in t}
    selected = [j for j in jobs if j['name'] in required]
    if {j['name'] for j in selected} != required or any(j['conclusion'] != 'success' for j in selected):
        raise ValueError('Native source/installation producer gates did not all pass')
    release = gh(f'repos/{repo}/releases/tags/{tag}')
    if release['draft'] or release['prerelease'] or release['tag_name'] != tag:
        raise ValueError('Expected the actual stable publication')
    catalog = []
    for page in range(1, 11):
        rows = gh(f'repos/{repo}/releases/{release["id"]}/assets?per_page=100&page={page}')
        if not isinstance(rows, list):
            raise TypeError('Invalid asset page')
        catalog.extend(rows)
        if len(rows) < 100:
            break
    else:
        raise ValueError('Release asset inventory exceeded its bound')
    release['assets'] = catalog
    if any(j['completed_at'] > release['published_at'] for j in selected):
        raise ValueError('Publication preceded its mandatory native gates')
    archive = f'{package}-{target}' + ('.zip' if target.endswith('msvc') else '.tar.gz')
    names = [archive, archive + '.sha256', f'{package}-{target}-source-lock.json']
    assets = []
    for name in names:
        rows = [a for a in release['assets'] if a['name'] == name]
        if len(rows) != 1:
            raise ValueError('Missing or duplicate published asset')
        assets.append(rows[0])
    with tempfile.TemporaryDirectory(prefix='spectrum-public-release-') as folder:
        root = Path(folder)
        for name in names:
            subprocess.run(['gh', 'release', 'download', tag, '-R', repo, '--pattern', name, '--dir', str(root)], check=True, timeout=90)
        raw = (root / archive).read_bytes()
        archive_sha = hashlib.sha256(raw).hexdigest()
        checksum = (root / (archive + '.sha256')).read_text().split()
        if len(checksum) != 2 or checksum[0] != archive_sha or checksum[1].lstrip('*') != archive:
            raise ValueError('Published archive/checksum differs')
        if assets[0].get('digest') != 'sha256:' + archive_sha:
            raise ValueError('GitHub asset digest differs')
        if target.endswith('msvc'):
            with zipfile.ZipFile(root / archive) as zipped:
                if zipped.namelist().count(package + '.exe') != 1 or zipped.getinfo(package + '.exe').file_size > 128 * 1024 * 1024:
                    raise ValueError('Expected one bounded executable')
                executable = zipped.read(package + '.exe')
        else:
            with tarfile.open(root / archive) as packed:
                members = [m for m in packed.getmembers() if m.name == f'{package}-{target}/{package}' and m.isfile()]
                if len(members) != 1 or members[0].size > 128 * 1024 * 1024:
                    raise ValueError('Expected one bounded executable')
                stream = packed.extractfile(members[0])
                if stream is None:
                    raise ValueError('Missing executable stream')
                executable = stream.read()
        if module.binary_target(executable) != target:
            raise ValueError('Wrong executable architecture')
        artifact = {'package': package, 'version': '0.35.2', 'target': target, 'archive': archive,
                    'archive_sha256': archive_sha, 'executable_sha256': hashlib.sha256(executable).hexdigest()}
        receipt = json.loads((root / names[2]).read_text())
        identity = {'commit': source, 'cargo_lock_sha256': hashlib.sha256(lock).hexdigest()}
        env = {'GITHUB_ACTIONS': 'true', 'GITHUB_RUN_ID': str(run['id']), 'GITHUB_RUN_ATTEMPT': str(run['run_attempt'])}
        module.verify_receipt(receipt, artifact, identity, env)
        artifacts = gh(f'repos/{repo}/actions/runs/{args.producer_run}/artifacts?per_page=100')
        if artifacts['total_count'] > 100:
            raise ValueError('Producer artifact inventory exceeds this bounded lookup')
        matches = [a for a in artifacts['artifacts'] if a['name'] == f'producer-qualification-{target}' and not a['expired']]
        if len(matches) != 1:
            raise ValueError('Missing actual native producer qualification artifact')
        producer_artifact = matches[0]
        zipped_bytes = subprocess.check_output(['gh', 'api', '--allow-escape-sequences', f'repos/{repo}/actions/artifacts/{producer_artifact["id"]}/zip'], timeout=60)
        with zipfile.ZipFile(io.BytesIO(zipped_bytes)) as zipped:
            if zipped.namelist() != ['producer-verification.json'] or zipped.getinfo('producer-verification.json').file_size > 16 * 1024 * 1024:
                raise ValueError('Unexpected native producer report')
            producer_report = json.loads(zipped.read('producer-verification.json'))
        if producer_report.get('verified') is not True or producer_report['source'] != identity or producer_report['target'] != target:
            raise ValueError('Actual native qualification source/target differs')
        native_receipts = [r for r in producer_report['receipts'] if r['package'] == package and r['target'] == target]
        if native_receipts != [receipt]:
            raise ValueError('Public receipt differs from the actual native producer report')
        for key in ['source_commit', 'committed_lock_sha256', 'archive_sha256', 'executable_sha256']:
            bad = copy.deepcopy(receipt)
            bad[key] = '0' * len(bad[key])
            try:
                module.verify_receipt(bad, artifact, identity, env)
            except ValueError:
                continue
            raise ValueError('Changed receipt incorrectly passed: ' + key)
        output = {'verified': True, 'tag': tag, 'source': identity, 'artifact': artifact, 'receipt': receipt,
                  'producer_run': {k: run[k] for k in ['id', 'head_sha', 'run_attempt', 'conclusion', 'html_url']},
                  'producer_jobs': jobs, 'producer_qualification_report': producer_report,
                  'producer_artifact': {k: producer_artifact[k] for k in ['id', 'name', 'size_in_bytes', 'digest']},
                  'producer_artifact_download_sha256': hashlib.sha256(zipped_bytes).hexdigest(),
                  'release': {k: release[k] for k in ['id', 'tag_name', 'published_at', 'html_url']},
                  'assets': [{k: a[k] for k in ['id', 'name', 'size', 'digest', 'browser_download_url']} for a in assets],
                  'producer_validator_sha256': hashlib.sha256(released_validator).hexdigest(),
                  'changed_receipt_controls_rejected': 4, 'compiled_dependency_graph': 'unknown', 'signature': 'not_applicable_unsigned'}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(output, indent=2) + '\n')
        print('Published archive/executable/source/producer identities agree; four changed receipt controls reject')


if __name__ == '__main__':
    main()
