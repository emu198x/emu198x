"""Verify the retained package, independent extraction and cold-boot outcome."""

import copy
import hashlib
import json
from pathlib import Path


def hex_value(value: str) -> int:
    return int(value.removeprefix('$'), 16)


def runtime_check(root: Path, runtime: dict) -> None:
    if runtime['exit_code'] != 0:
        raise ValueError('emulator did not exit cleanly')
    disk_hash = hashlib.sha256((root / 'demo.adf').read_bytes()).hexdigest()
    if runtime['disk_sha256'] != disk_hash:
        raise ValueError('runtime did not use the retained disk')
    colours = runtime['colours']
    if colours['filtered_total'] != len(colours['entries']) or colours['returned'] != colours['filtered_total']:
        raise ValueError('colour-write log is incomplete')
    program = [entry for entry in colours['entries'] if hex_value(entry['pc']) < 0x80000]
    if [hex_value(entry['value']) for entry in program] != [0x35, 0x50]:
        raise ValueError('intended program did not start and finish')
    if program[0]['cck'] >= program[1]['cck']:
        raise ValueError('completion precedes start')
    pc = hex_value(runtime['cpu']['registers']['pc'])
    if not 0 < pc < 0x80000:
        raise ValueError('CPU did not reach the program in chip RAM')
    memory = runtime['memory_near_pc']
    # MOVE.W #$0050,$0180(A5), followed by BRA.S back to itself.
    signature = bytes.fromhex('3b7c0050018060fe')
    if memory['addr'] != pc - 6 or bytes(memory['bytes'][:8]) != signature:
        raise ValueError('actual PC/memory differs from completion and hold loop')
    if (root / 'demo').read_bytes().count(signature) != 1:
        raise ValueError('completion signature absent or ambiguous in packaged executable')


def verify(root: Path) -> None:
    manifest = json.loads((root / 'files.json').read_text())
    if not manifest:
        raise ValueError('empty evidence manifest')
    for name, expected in manifest.items():
        if hashlib.sha256((root / name).read_bytes()).hexdigest() != expected:
            raise ValueError(f'evidence hash mismatch: {name}')
    source = json.loads((root / 'source.json').read_text())
    for name, expected in source['files'].items():
        if hashlib.sha256((root / name).read_bytes()).hexdigest() != expected:
            raise ValueError(f'sample input differs from pinned source: {name}')
    for name, expected in source['rebuild_sha256'].items():
        if hashlib.sha256((root / name).read_bytes()).hexdigest() != expected:
            raise ValueError(f'independent rebuild differs: {name}')
    if (root / 'demo').read_bytes() != (root / 'extracted-demo').read_bytes():
        raise ValueError('packaged executable differs from assembled input')
    if (root / 'extracted-startup-sequence').read_bytes() != b'demo\n':
        raise ValueError('startup sequence does not launch the packaged demo')
    adf = json.loads((root / 'adf-verify.stdout').read_text())
    if adf.get('result') != 'ok' or adf.get('filesystem') != 'ofs':
        raise ValueError('ADF integrity did not pass')
    commands = json.loads((root / 'commands.json').read_text())
    outcomes = {row['label']: row['exit_code'] for row in commands}
    for label in ['make', 'rebuild', 'adf-verify', 'adf-info', 'xdf-help', 'xdf-list', 'xdf-extract', 'xdf-startup']:
        if outcomes.get(label) != 0:
            raise ValueError(f'missing successful command: {label}')
    if outcomes.get('truncated-verify') != 1 or 'corrupt ADF' not in (root / 'truncated-verify.stderr').read_text():
        raise ValueError('truncated disk was not rejected')
    if json.loads((root / 'runner-command.json').read_text())['exit_code'] != 0:
        raise ValueError('runtime runner did not succeed')
    runtime = json.loads((root / 'runtime.json').read_text())
    runtime_check(root, runtime)
    # Check the raw RPC transcript rather than trusting only the derived report.
    transcript = [json.loads(line) for line in (root / 'rpc.jsonl').read_text().splitlines()]
    requests = [r['request'] for r in transcript if 'request' in r]
    responses = {r['response']['id']: r['response'] for r in transcript if 'response' in r and 'id' in r['response']}
    frames = []
    observations = {}
    for request in requests:
        response = responses.get(request['id'])
        if response is None or 'error' in response:
            raise ValueError('RPC request lacks successful response')
        if request['method'] == 'tools/call':
            result = response['result']
            if result.get('isError'):
                raise ValueError('RPC tool failed')
            name = request['params']['name']
            if name == 'run_frames':
                frames.append(request['params']['arguments']['frames'])
            else:
                observations[name] = json.loads(result['content'][0]['text'])
    if frames != [400] * 5:
        raise ValueError('cold boot did not run the declared frames')
    for name, field in [('query_cpu', 'cpu'), ('chipset_write_log', 'colours'), ('memory_read', 'memory_near_pc')]:
        if observations.get(name) != runtime[field]:
            raise ValueError('derived observation differs from raw RPC response')
    mutated = copy.deepcopy(runtime)
    mutated['colours']['entries'] = [entry for entry in mutated['colours']['entries'] if hex_value(entry['value']) != 0x50]
    mutated['colours']['filtered_total'] -= 1
    mutated['colours']['returned'] -= 1
    try:
        runtime_check(root, mutated)
    except ValueError as error:
        print(f'Missing-completion negative control rejected: {error}')
    else:
        raise ValueError('missing completion incorrectly accepted')
    print(f'{len(manifest)} evidence hashes, package/extraction/rebuild and cold-boot outcome verified')


if __name__ == '__main__':
    verify(Path(__file__).parent)
