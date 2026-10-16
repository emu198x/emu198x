"""Cold-boot the packaged demo and retain bounded read-only MCP observations."""

import argparse
import hashlib
import json
import os
import selectors
import subprocess
import time
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--emulator', type=Path, required=True)
    parser.add_argument('--kickstart', type=Path, required=True)
    parser.add_argument('--disk', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    argv = [str(args.emulator), '--mcp', '--model', 'a500', '--kickstart', str(args.kickstart), '--disk', str(args.disk)]
    started = time.monotonic()
    with (args.output / 'runtime.stderr').open('wb') as stderr, (args.output / 'rpc.jsonl').open('w') as transcript:
        process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)
        if process.stdin is None or process.stdout is None:
            raise RuntimeError('MCP pipes missing')
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        buffered = b''
        sequence = 0

        def rpc(method: str, params: dict) -> dict:
            nonlocal sequence, buffered
            sequence += 1
            request = {'jsonrpc': '2.0', 'id': sequence, 'method': method, 'params': params}
            transcript.write(json.dumps({'request': request}) + '\n')
            transcript.flush()
            process.stdin.write(json.dumps(request).encode() + b'\n')
            process.stdin.flush()
            deadline = min(started + 240, time.monotonic() + 60)
            while True:
                while b'\n' not in buffered:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not selector.select(remaining):
                        raise TimeoutError(f'MCP timeout: {method} {params}')
                    chunk = os.read(process.stdout.fileno(), 65536)
                    if not chunk:
                        raise RuntimeError('emulator exited before reply')
                    buffered += chunk
                line, buffered = buffered.split(b'\n', 1)
                response = json.loads(line)
                transcript.write(json.dumps({'response': response}) + '\n')
                transcript.flush()
                if response.get('id') == sequence:
                    if 'error' in response:
                        raise RuntimeError(response['error'])
                    return response['result']

        def call(name: str, **arguments: object) -> dict:
            result = rpc('tools/call', {'name': name, 'arguments': arguments})
            if result.get('isError'):
                raise RuntimeError(result)
            return json.loads(result['content'][0]['text'])

        try:
            rpc('initialize', {'protocolVersion': '2025-06-18', 'capabilities': {}, 'clientInfo': {'name': 'package-reopen-audit', 'version': '1'}})
            for batch in range(5):
                call('run_frames', frames=400)
                print(f'Cold boot: {(batch + 1) * 400} frames', flush=True)
            cpu = call('query_cpu')
            colours = call('chipset_write_log', offset=0x180, limit=8192)
            pc = int(str(cpu['registers']['pc']).lstrip('$'), 16)
            memory = call('memory_read', addr=pc - 6, len=10)
            screenshot = args.output / 'finished.png'
            call('save_screenshot', path=str(screenshot))
            result = {'configuration': 'A500 OCS PAL / Kickstart 1.3 / cold ADF boot', 'argv': argv,
                      'disk_sha256': hashlib.sha256(args.disk.read_bytes()).hexdigest(),
                      'kickstart_sha256': hashlib.sha256(args.kickstart.read_bytes()).hexdigest(),
                      'cpu': cpu, 'colours': colours, 'memory_near_pc': memory}
        finally:
            process.stdin.close()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
            selector.close()
            process.stdout.close()
        result.update(exit_code=process.returncode, elapsed_seconds=round(time.monotonic() - started, 3),
                      total_timeout_seconds=240, per_call_timeout_seconds=60)
        (args.output / 'runtime.json').write_text(json.dumps(result, indent=2) + '\n')
        if process.returncode != 0:
            raise SystemExit(f'emulator exit: {process.returncode}')
        print('Cold-boot observations retained; verify them before claiming success', flush=True)


if __name__ == '__main__':
    main()
