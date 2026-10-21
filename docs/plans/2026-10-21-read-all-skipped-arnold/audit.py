#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Audit raw captured Arnold observations. Exit zero is not a hardware pass."""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import tarfile
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ARCHIVE_SHA = 'bf124873d8e4a71cb0cb9ac92d846e4b0540c531c1581cc64cdb4e63145d58bd'
SOURCE = {
    'src/cpc/fdc.c': '3cd2e948ce23c3285247f3f64c5114b2c16d571173b6af6f952144569ffbec30',
    'src/cpc/fdc.h': '1fa127ac49942efe07f9622443d8a995461a723d7ab3a807c20412caba44aad3',
    'src/cpc/fdi.c': '6492d6e892ef542c5777de3f4bae6433bfe1aef5044cf30cf29dc3c87ccf6174',
    'src/cpc/fdi.h': 'e8d0eb07deadc83573c0de35427cbc39d96689a15b6618e6699e33d4b8faa973',
    'src/cpc/fdd.c': '366bd5f0fbfae22b928aa32b5499531a6cfa0e9668aae20267f123daf68a130a',
    'src/cpc/fdd.h': 'b12e5254f943a5716f7067d1ec8e918159449a9c798a37700637b0f08e771d13',
    'src/cpc/device.h': '1e73d19fa3a34d805d980bb29fcde367c9b50af2976daa62cb03007b2427cd2f',
    'src/cpc/diskimage/diskimg.h': '80d0b7b8daa3496823963087308f10206d0b2812daf14b956095dcbe260da6b4',
}


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise ValueError(reason)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load() -> dict[str, bytes]:
    files = {}
    with tarfile.open(HERE / 'runs.tar.gz') as archive:
        for member in archive.getmembers():
            require(member.isfile() and member.name not in files and member.size < 2_000_000, 'archive member')
            require(not Path(member.name).is_absolute() and '..' not in Path(member.name).parts, 'unsafe member')
            stream = archive.extractfile(member)
            require(stream is not None, 'missing archive stream')
            files[member.name] = stream.read()
    return files


def audit(files: dict[str, bytes]) -> dict:
    m = json.loads(files['manifest.json'])
    require(m['qualified'] is False and m['status'] == 'blocked_dependency', 'qualification scope')
    require(m['execution'] == 'fresh' and m['producer'] == 'source_build', 'execution mode')
    require(m['reference']['archive_sha256'] == ARCHIVE_SHA and m['reference']['members'] == SOURCE and m['reference']['source_commit'] is None, 'reference identity')
    require(m['build']['exit_code'] == 0 and '-fno-sanitize-recover=all' in m['build']['command'] and '-isysroot' in m['build']['command'], 'build identity')
    require(len(m['executable_sha256']) == len(m['compiler']['sha256']) == len(m['sdk']['settings_sha256']) == 64, 'tool identity')
    for name, expected in m['recipe'].items():
        require(sha((HERE.parent / name).read_bytes()) == expected, 'recipe identity')
    require(set(m['artifacts']) == set(files) - {'manifest.json'}, 'artifact set')
    for name, expected in m['artifacts'].items():
        require(sha(files[name]) == expected, 'artifact digest')
    spec = importlib.util.spec_from_file_location('prior_sk', HERE.parent / '2026-10-21-read-all-skipped/capture.py')
    require(spec is not None and spec.loader is not None, 'prior recipe')
    prior = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prior)
    cases = {(op, sk, match, two, 1, False) for op in [6, 12] for sk in [False, True] for match in [False, True] for two in [False, True]}
    cases |= {(op, True, False, True, 2, False) for op in [6, 12]}
    cases |= {(op, True, match, True, 1, True) for op in [6, 12] for match in [False, True]}
    expected_cases = {(*case, poll, repeat) for case in cases for poll in [1, 32] for repeat in [0, 1]}
    seen, counts, repeats = [], Counter(), {}
    for r in m['runs']:
        op, sk, match, two, eot, bad, poll, repeat = (r[k] for k in ['opcode', 'skip', 'matching', 'two_sectors', 'eot', 'bad_crc_metadata', 'poll_us', 'repeat'])
        seen.append((op, sk, match, two, eot, bad, poll, repeat))
        name = f'op{op:02x}-sk{int(sk)}-match{int(match)}-two{int(two)}-eot{eot}-crc{int(bad)}-poll{poll}-r{repeat}'
        require(r['name'] == name, 'case identity')
        require(r['exit_code'] == 0 and r['host_timeout_seconds'] == 15, 'process result')
        require(files[name + '/stderr.txt'] == b'', 'runtime diagnostic')
        media = bytearray(prior.edsk((op == 12) == match, two))
        if bad:
            media[284] |= 0x20
            media[285] |= 0x20
        require(files[name + '/input.edsk'] == media, 'disk recipe')
        require(files[name + '/live.edsk'] == media, 'source/live media changed')
        d = json.loads(files[name + '/stdout.json'])
        require(d['tick_seconds'] == 0.000001 and d['poll_us'] == poll and 0 < d['total_ticks'] < 8000000, 'emulated bounds')
        require(d['disk_ids'] > 0 and d['disk_reads'] > 0 and d['disk_bytes'] == d['disk_reads'] * 512 and d['disk_writes'] == 0, 'disk activity')
        p = d['phases']
        require([x['name'] for x in p] == ['specify', 'read', 'recovery_sds'], 'phase framing')
        command = [0x40 | (0x20 if sk else 0) | op, 0, 0, 0, 1, 2, eot, 42, 255]
        require([x['writes'] for x in p] == [[3, 223, 3], command, [4, 0]], 'raw command')
        require(r['command'][3:] == [str(command[0]), str(eot), str(poll)], 'process command')
        last_tick = 0
        for phase in p:
            require(phase['idle_msr'] == 128, 'idle framing')
            for tick, msr, value in phase['reads']:
                require(last_tick < tick <= d['total_ticks'] and tick % poll == 0 and msr in [208, 240] and 0 <= value <= 255, 'raw read framing')
                last_tick = tick
        require(not p[0]['reads'], 'SPECIFY framing')
        require(len(p[2]['reads']) == 1 and p[2]['reads'][0][1:] == [208, 48], 'SDS recovery')
        data = bytes(v for _, msr, v in p[1]['reads'] if msr == 240)
        result = [v for _, msr, v in p[1]['reads'] if msr == 208]
        require([msr for _, msr, _ in p[1]['reads']] == [240] * len(data) + [208] * 7, 'result framing')
        payload = prior.payload(2) if eot == 2 else b'' if sk and not match else prior.payload(1)
        require(data == payload, 'raw payload')
        expected = [64, 32, 32, 0, 0, 1, 2] if bad and match else [0, 0, 64, 0, 0, 1, 2] if not sk and not match else [64, 128, 0 if match else 64, 0, 0, eot, 2]
        require(result == expected, 'raw result')
        key = (op, sk, match, two, eot, bad)
        observation = (data, tuple(result))
        require(key not in repeats or repeats[key] == observation, 'repeat/poll disagreement')
        repeats[key] = observation
        label = ('CRC metadata, matching' if bad and match else 'CRC metadata, skipped' if bad else 'skip then R2' if eot == 2 else 'matching' if match else 'SK-on mismatch' if sk else 'SK-off mismatch')
        counts[(label, len(data), tuple(result))] += 1
    require(len(seen) == 88 and set(seen) == expected_cases, 'case matrix')
    return {'status': 'blocked_dependency', 'qualified': False, 'scope': 'captured component model observations only', 'fresh_processes': len(seen), 'skip_off_payload_controls_passed': 16, 'genuine_all_skipped_clean_runs': 16, 'repeat_and_poll_agreement': True, 'observations': [{'case': label, 'runs': n, 'payload_bytes': size, 'result': ' '.join(f'{b:02X}' for b in result)} for (label, size, result), n in sorted(counts.items())]}


def main() -> None:
    files = load()
    result = audit(files)
    rejected = []
    for edit in ['payload', 'phantom_payload', 'result', 'recovery', 'media', 'reference', 'matrix', 'false_qualification', 'empty_execution']:
        changed = copy.deepcopy(files)
        m = json.loads(changed['manifest.json'])
        r = next(r for r in m['runs'] if not r['matching'] and r['skip']) if edit == 'phantom_payload' else m['runs'][0]
        name = r['name'] + '/stdout.json'
        raw = json.loads(changed[name])
        if edit == 'payload':
            raw['phases'][1]['reads'][0][2] ^= 1
        elif edit == 'phantom_payload':
            first = raw['phases'][1]['reads'][0]
            raw['phases'][1]['reads'].insert(0, [first[0] - r['poll_us'], 240, 33])
        elif edit == 'result':
            raw['phases'][1]['reads'][-7][2] ^= 1
        elif edit == 'recovery':
            raw['phases'][2]['reads'] = []
        elif edit == 'media':
            changed[r['name'] + '/live.edsk'] = b'changed media'
        elif edit == 'reference':
            m['reference']['members']['src/cpc/fdc.c'] = '0' * 64
        elif edit == 'matrix':
            m['runs'] = m['runs'][:-1]
        elif edit == 'false_qualification':
            m['qualified'] = True
        elif edit == 'empty_execution':
            raw['phases'][1]['reads'] = []
        changed[name] = json.dumps(raw).encode()
        m['artifacts'] = {n: sha(b) for n, b in changed.items() if n != 'manifest.json'}
        changed['manifest.json'] = json.dumps(m).encode()
        try:
            audit(changed)
        except ValueError as error:
            rejected.append({'control': edit, 'rejected': str(error)})
        else:
            raise ValueError(f'false evidence accepted: {edit}')
    result['negative_controls'] = rejected
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
