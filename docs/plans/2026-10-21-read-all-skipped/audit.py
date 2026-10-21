#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Read-only audit of captured reference-model observations, not silicon truth."""
from __future__ import annotations

import copy
import hashlib
import io
import json
import tarfile
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def check(record: dict, files: dict[str, bytes]) -> dict:
    if record.get('qualified') is not False or record.get('origin') != 'fresh_execution':
        raise ValueError('reference research cannot qualify a configuration')
    if record.get('kind') != 'all-skipped-direct-reference-controller-research-not-rom-loading':
        raise ValueError('wrong study kind')
    if record.get('capture_completed') is not True:
        raise ValueError('incomplete capture')
    if record['fuse_source']['patched'] is not False:
        raise ValueError('reference controller was patched')
    if record['fuse_source']['commit'] != '0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2':
        raise ValueError('wrong executed Fuse source')
    if record['mame_executable']['source_commit'] != 'unknown':
        raise ValueError('unattested installed MAME source identity')
    if record['mame_inspected_source']['executed_source_identity'] is not False:
        raise ValueError('inspected MAME source is not a producer attestation')
    if record['fuse_executable']['sha256'] not in (HERE / 'README.md').read_text():
        raise ValueError('stale executable description')
    for name, expected in record['authored_sources'].items():
        if hashlib.sha256((HERE / name).read_bytes()).hexdigest() != expected:
            raise ValueError('authored capture source changed: ' + name)
    expected_keys = {(engine, repeat, opcode | (0x20 if sk else 0), two, sk)
                     for engine in ['fuse', 'mame'] for repeat in [1, 2]
                     for opcode in [0x46, 0x4C] for two in [False, True]
                     for sk in [False, True]}
    seen = set()
    commands = {row['name']: row for row in record['commands']}
    if len(commands) != len(record['commands']):
        raise ValueError('duplicate command receipt')
    payload = bytes(((33 + (i & 255)) & 255) ^ (i >> 8) for i in range(512))
    for row in record['observations']:
        key = (row['engine'], row['repeat'], row['opcode'], row['two_sectors'], row['sk'])
        if key not in expected_keys or key in seen or row['n'] != 2 or row['dtl'] != 255 or row['no_tc'] is not True:
            raise ValueError('missing, duplicate or wrong reference case')
        seen.add(key)
        count = 512
        actual = row['observed']
        raw_name = row['name'] + ('.stdout' if row['engine'] == 'fuse' else '.json')
        if raw_name not in files or json.loads(files[raw_name]) != actual:
            raise ValueError('raw observation disagrees with summary: ' + raw_name)
        if row['payload_count'] != count or bytes(actual['data']) != payload[:count]:
            raise ValueError('wrong observed model payload')
        expected_result = ([64, 128, 64, 0, 0, 1, 2] if row['engine'] == 'mame' and row['sk']
                           else [0, 0, 64, 0, 0, 1, 2])
        if actual['result'] != expected_result:
            raise ValueError('wrong observed model result')
        if actual['idle_msr'] != 128 or actual['recovery_msr'] != 128:
            raise ValueError('result did not drain and recover')
        if row['engine'] == 'fuse' and actual['media_unchanged'] is not True:
            raise ValueError('live reference track changed')
        if hashlib.sha256(files[row['media']]).hexdigest() != row['media_sha256']:
            raise ValueError('reference source media changed')
        command = commands[row['name']]
        if command['exit'] != 0 or not 0 < command['host_timeout_seconds'] <= 45:
            raise ValueError('bad process receipt')
        if row['engine'] == 'fuse':
            expected = [str(value) for value in [row['opcode'] | (0x20 if row['sk'] else 0), 0, 0, 0, 1, 2, 1, 42, 255]]
            if command['argv'][-9:] != expected or Path(command['argv'][-10]).name != row['media']:
                raise ValueError('wrong executed Fuse command')
        else:
            environment = command['environment']
            if environment.get('PROBE_DTL') != str(row['dtl']) or environment.get('PROBE_OPCODE') != str(row['opcode'] | (0x20 if row['sk'] else 0)):
                raise ValueError('wrong executed MAME command')
            if '-cfg_directory' not in command['argv'] or command['argv'][-2:] != ['-seconds_to_run', '10']:
                raise ValueError('wrong MAME host bounds or private settings')
        if files[row['name'] + '.stdout'] == b'' and row['engine'] == 'fuse':
            raise ValueError('empty source-controller output')
    if seen != expected_keys:
        raise ValueError('missing reference case')
    return {'schema_version': 1, 'status': 'pass', 'qualified': False,
            'scope': 'captured independent reference-model observations only',
            'final_status_oracle': 'unresolved', 'observations': len(seen),
            'fuse_processes': 16, 'mame_processes': 16,
            'observed_payload_bytes_every_case': 512,
            'genuine_all_skipped_transfer_observed': False}



def load(path: Path = HERE / 'runs.tar.gz') -> tuple[dict, dict[str, bytes]]:
    with tarfile.open(path) as archive:
        members = archive.getmembers()
        if len(members) > 200 or len({m.name for m in members}) != len(members) or any(not m.isfile() or m.size > 1024 * 1024
                                     or Path(m.name).is_absolute() or '..' in Path(m.name).parts
                                     for m in members):
            raise ValueError('unsafe or unbounded research archive')
        files = {m.name: archive.extractfile(m).read() for m in members}
    hashes = json.loads(files['hashes.json'])
    if set(hashes) != set(files) - {'hashes.json'}:
        raise ValueError('missing or unlisted artifact')
    for name, expected in hashes.items():
        if hashlib.sha256(files[name]).hexdigest() != expected:
            raise ValueError('archive hash mismatch: ' + name)
    return json.loads(files['campaign.json']), files


def controls(record: dict, files: dict[str, bytes]) -> list[str]:
    rejected = []
    for label, mutation in [
        ('false-qualification', lambda r: r.update(qualified=True)),
        ('missing-case', lambda r: r['observations'].pop()),
        ('duplicate-case', lambda r: r['observations'].append(copy.deepcopy(r['observations'][0]))),
        ('wrong-zero-payload', lambda r: r['observations'][0]['observed'].update(data=[])),
        ('wrong-256-payload', lambda r: r['observations'][0]['observed'].update(data=list(range(256)))),
        ('invented-all-skipped-status', lambda r: r['observations'][0]['observed'].update(result=[64, 4, 0, 0, 0, 1, 2])),
        ('invented-mame-source', lambda r: r['mame_executable'].update(source_commit='attested')),
        ('wrong-sk-command', lambda r: next(c for c in r['commands'] if c['name'] == 'mame-1-66-sectors1')['environment'].update(PROBE_OPCODE='70')),
        ('stale-executable-description', lambda r: r['fuse_executable'].update(sha256='0' * 64)),
        ('zero-processes', lambda r: r.update(observations=[])),
    ]:
        candidate = copy.deepcopy(record)
        mutation(candidate)
        try:
            check(candidate, files)
        except ValueError:
            rejected.append(label)
        else:
            raise ValueError('false evidence accepted: ' + label)
    # Rehash the altered raw artifact, write a real archive, then traverse the
    # ordinary loader. Integrity checks alone must not accept semantic disagreement.
    for engine in ['fuse', 'mame']:
        candidate_files = files.copy()
        row = next(row for row in record['observations'] if row['engine'] == engine)
        name = row['name'] + ('.stdout' if engine == 'fuse' else '.json')
        raw = json.loads(candidate_files[name])
        raw['data'] = []
        candidate_files[name] = json.dumps(raw).encode()
        hashes = json.loads(candidate_files['hashes.json'])
        hashes[name] = hashlib.sha256(candidate_files[name]).hexdigest()
        candidate_files['hashes.json'] = json.dumps(hashes).encode()
        with tempfile.TemporaryDirectory(prefix='198x-rehashed-raw-audit-') as temporary:
            archive_path = Path(temporary) / 'altered.tar.gz'
            with tarfile.open(archive_path, 'w:gz') as archive:
                for artifact, data in candidate_files.items():
                    info = tarfile.TarInfo(artifact)
                    info.size = len(data)
                    archive.addfile(info, io.BytesIO(data))
            candidate_record, decoded = load(archive_path)
            try:
                check(candidate_record, decoded)
            except ValueError as error:
                if 'raw observation disagrees with summary' not in str(error):
                    raise ValueError('semantic control failed for the wrong reason') from error
                rejected.append(engine + '-rehashed-raw-summary-disagreement')
            else:
                raise ValueError('altered/rehashed raw output accepted: ' + engine)
    return rejected


if __name__ == '__main__':
    record, files = load()
    result = check(record, files)
    result['false_evidence_controls_rejected'] = controls(record, files)
    print(json.dumps(result, indent=2))
