#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Verify retained raw zxsp observations; success does not qualify hardware."""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
PIN = '27d2c30d86d3f4877e24dd3dd97d7ebd51bab7c6'
SOURCE = {
    'Source/Uni/Items/Fdc/Fdc765.cpp': '18380f64ad2d094f8f5f743e3244403176fecbc2e4490014e3bebcc1a5b20616',
    'Source/Uni/Items/Fdc/Fdc765.h': '542f7aa156a980a2dc8788ad190caac4386a49e7b5b030635e74410b986782a4',
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_archive() -> dict[str, bytes]:
    with tarfile.open(HERE / 'runs.tar.gz', 'r:gz') as archive:
        files = {}
        for member in archive.getmembers():
            require(member.isfile() and member.name not in files, 'invalid/duplicate archive member')
            require(not Path(member.name).is_absolute() and '..' not in Path(member.name).parts, 'unsafe member')
            require(member.size < 2_000_000, 'oversize member')
            stream = archive.extractfile(member)
            require(stream is not None, 'missing member stream')
            files[member.name] = stream.read()
        return files


def audit(files: dict[str, bytes]) -> dict:
    m = json.loads(files['manifest.json'])
    require(m['qualified'] is False and m['status'] == 'blocked_dependency', 'qualification scope')
    require(m['execution'] == 'fresh' and m['producer'] == 'source_build', 'producer mode')
    require(m['reference']['commit'] == PIN and m['reference']['files'] == SOURCE, 'reference identity')
    require(m['build']['exit_code'] == 0 and '-fno-sanitize-recover=all' in m['build']['command'], 'build identity')
    require(len(m['executable_sha256']) == 64, 'missing executable identity')
    for name, expected in m['recipe'].items():
        require(digest((HERE.parent / name).read_bytes()) == expected, 'recipe identity')
    require(set(m['artifacts']) == set(files) - {'manifest.json'}, 'artifact set')
    for name, expected in m['artifacts'].items():
        require(digest(files[name]) == expected, 'artifact digest')
    spec = importlib.util.spec_from_file_location('prior_sk', HERE.parent / '2026-10-21-read-all-skipped/capture.py')
    require(spec is not None and spec.loader is not None, 'missing prior recipe')
    prior = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prior)
    expected_cases = {(op, sk, match, two, False, rep) for op in [6, 12] for sk in [False, True] for match in [False, True] for two in [False, True] for rep in [0, 1]}
    expected_cases |= {(op, True, False, True, True, rep) for op in [6, 12] for rep in [0, 1]}
    cases = [(r['opcode'], r['skip'], r['matching'], r['two_sectors'], r['bad_crc'], r['repeat']) for r in m['runs']]
    require(len(cases) == 36 and set(cases) == expected_cases, 'case matrix')
    observations = []
    for r in m['runs']:
        op, sk, match, two, bad, rep = r['opcode'], r['skip'], r['matching'], r['two_sectors'], r['bad_crc'], r['repeat']
        name = r['name']
        require(name == f'op{op:02x}-sk{int(sk)}-match{int(match)}-two{int(two)}-crc{int(bad)}-r{rep}', 'case identity')
        require(r['exit_code'] == 0 and r['host_timeout_seconds'] == 15, 'process result')
        require(files[name + '/stderr.txt'] == b'', 'unexpected diagnostic')
        track = bytearray(prior.raw_track((op == 12) == match, two)[3:6253])
        if bad:
            track[653] ^= 1
        require(files[name + '/track.bin'] == track, 'track recipe')
        require(files[name + '/live.bin'] == track, 'live media changed')
        d = json.loads(files[name + '/stdout.json'])
        require(d['tick_seconds'] == 0.000001 and 0 < d['total_ticks'] < 8000000, 'emulated bounds')
        require(d['drive_reads'] >= 512 and d['drive_writes'] == 0, 'drive activity')
        phases = d['phases']
        require([p['name'] for p in phases] == ['initial_sis', 'specify', 'read', 'recovery_sds'], 'phase framing')
        command = [0x40 | (0x20 if sk else 0) | op, 0, 0, 0, 1, 2, 1, 42, 255]
        require([p['writes'] for p in phases] == [[8], [3, 223, 3], command, [4, 0]], 'raw command bytes')
        require(r['command'][3:] == [str(command[0]), '2', '1', '255'], 'process command')
        last_tick = 0
        for p in phases:
            require(p['idle_msr'] == 128, 'idle recovery')
            for tick, msr, value in p['reads']:
                require(last_tick < tick <= d['total_ticks'] and msr in [208, 240] and 0 <= value <= 255, 'raw read framing')
                last_tick = tick
        require([x[2] for x in phases[0]['reads']] == [192, 0], 'initial READY acknowledgment')
        require(not phases[1]['reads'], 'SPECIFY result')
        require(len(phases[3]['reads']) == 1 and phases[3]['reads'][0][1:] == [208, 48], 'SDS recovery')
        reads = phases[2]['reads']
        data = bytes(v for _, msr, v in reads if msr == 240)
        result = [v for _, msr, v in reads if msr == 208]
        require([msr for _, msr, _ in reads] == [240] * len(data) + [208] * 7, 'result framing')
        require(data == (prior.payload(1) if match else b''), 'raw payload')
        expected = [64, 32, 32, 0, 0, 1, 2] if bad else [64, 0, 64, 0, 0, 1, 2] if not match and not sk else [0, 128, 0, 1, 0, 1, 2]
        require(result == expected, 'raw result')
        observations.append({'name': name, 'payload_bytes': len(data), 'result': ' '.join(f'{b:02X}' for b in result), 'skip_off_payload_control': 'fail' if not sk and not match else 'not_applicable'})
    require(sum(r['skip_off_payload_control'] == 'fail' for r in observations) == 8, 'control count')
    return {'status': 'blocked_dependency', 'qualified': False, 'fresh_processes': len(observations), 'failed_skip_off_controls': 8, 'observations': observations}


def main() -> None:
    files = load_archive()
    result = audit(files)
    rejected = []
    edits = ['payload', 'phantom_payload', 'status', 'recovery', 'reference', 'matrix', 'zero_assertions', 'media']
    for edit in edits:
        changed = copy.deepcopy(files)
        m = json.loads(changed['manifest.json'])
        r = next(r for r in m['runs'] if r['matching']) if edit == 'payload' else m['runs'][0]
        raw_name = r['name'] + '/stdout.json'
        raw = json.loads(changed[raw_name])
        if edit == 'payload':
            raw['phases'][2]['reads'][0][2] ^= 1
        elif edit == 'phantom_payload':
            event = raw['phases'][2]['reads'][0]
            raw['phases'][2]['reads'].insert(0, [event[0] - 1, 240, 33])
        elif edit == 'status':
            raw['phases'][2]['reads'][0][2] ^= 1
        elif edit == 'recovery':
            raw['phases'][3]['reads'] = []
        elif edit == 'reference':
            m['reference']['commit'] = '0' * 40
        elif edit == 'matrix':
            m['runs'] = m['runs'][:-1]
        elif edit == 'zero_assertions':
            raw['phases'][2]['reads'] = []
        elif edit == 'media':
            changed[r['name'] + '/live.bin'] = b'wrong media'
        changed[raw_name] = (json.dumps(raw) + '\n').encode()
        m['artifacts'] = {n: digest(b) for n, b in changed.items() if n != 'manifest.json'}
        changed['manifest.json'] = json.dumps(m).encode()
        try:
            audit(changed)
        except (ValueError, IndexError) as error:
            rejected.append({'control': edit, 'rejected': str(error)})
        else:
            raise ValueError(f'false evidence accepted: {edit}')
    result['negative_controls'] = rejected
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
