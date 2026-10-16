"""Controls for the Windows journey's independent media/runtime observations."""

import importlib.util
import json
import struct
import tempfile
import unittest
from pathlib import Path
from subprocess import CompletedProcess
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('journey', Path(__file__).with_name('windows-developer-journey.py'))
assert SPEC is not None and SPEC.loader is not None
journey = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(journey)


def frame(payload: bytes) -> bytes:
    checksum = 0
    for value in payload:
        checksum ^= value
    block = payload + bytes([checksum])
    return struct.pack('<H', len(block)) + block


def pair(kind: int, name: bytes, address: int, payload: bytes) -> bytes:
    header = bytes([0, kind]) + name.ljust(10) + struct.pack('<HHH', len(payload), address, 0)
    return frame(header) + frame(b'\xff' + payload)


class JourneyControls(unittest.TestCase):
    def test_package_recovers_independent_inputs_and_rejects_missing_or_changed_code(self) -> None:
        code, screen = b'entry' * 20, b'screen' * 1152
        tape = pair(0, b'meteor', 10, b'loader') + pair(3, b'screen', 16384, screen) + pair(3, b'storm', 32768, code)
        journey.verify_package(tape, code, screen)
        with self.assertRaisesRegex(ValueError, 'header/data pairs'):
            journey.verify_package(pair(0, b'meteor', 10, b'loader'), code, screen)
        with self.assertRaisesRegex(ValueError, 'differs'):
            journey.verify_package(tape, b'wrong code', screen)
        changed = bytearray(tape)
        changed[-1] ^= 1
        with self.assertRaisesRegex(ValueError, 'checksum'):
            journey.verify_package(bytes(changed), code, screen)
        with self.assertRaisesRegex(ValueError, 'Truncated'):
            journey.tape_blocks(tape[:-1])

    def test_checksum_correct_wrong_load_address_is_rejected(self) -> None:
        code, screen = b'program', b'pixels'
        tape = pair(0, b'meteor', 10, b'loader') + pair(3, b'screen', 16384, screen) + pair(3, b'storm', 49152, code)
        with self.assertRaisesRegex(ValueError, 'load address'):
            journey.verify_package(tape, code, screen)

    def test_runtime_needs_loaded_code_cpu_state_and_actual_title_pixels(self) -> None:
        code = bytes(range(64))
        rom = bytes(i % 251 for i in range(16384))
        phase = 35000
        rows = [{'kind': 'memory_read', 'addr': 32768, 'bytes': list(code)}, {'kind': 'memory_read', 'addr': phase, 'bytes': [0]}]
        # Independently stated row-3 scanline addresses, rather than the helper.
        for line, address in enumerate([0x406A, 0x416A, 0x426A, 0x436A, 0x446A, 0x456A, 0x466A, 0x476A]):
            rows.append({'kind': 'memory_read', 'addr': address, 'bytes': [rom[0x3C00 + character * 8 + line] for character in b'METEOR STORM']})
        rows.append({'kind': 'query_cpu', 'registers': {'pc': '$8010'}})
        journey.verify_boot({'observations': rows}, code, phase, rom)
        for index, replacement, reason in [(0, [0] * 64, 'entry bytes'), (1, [1], 'title state'), (2, [0] * 12, 'Rendered title')]:
            changed = json.loads(json.dumps(rows))
            changed[index]['bytes'] = replacement
            with self.assertRaisesRegex(ValueError, reason):
                journey.verify_boot({'observations': changed}, code, phase, rom)
        rows[-1]['registers']['pc'] = '$0556'
        with self.assertRaisesRegex(ValueError, 'CPU/title state'):
            journey.verify_boot({'observations': rows}, code, phase, rom)

    def test_wrong_or_missing_firmware_cannot_qualify(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / '48.rom'
            with self.assertRaises(FileNotFoundError):
                journey.firmware(path, '0' * 64)
            path.write_bytes(bytes(16384))
            with self.assertRaisesRegex(ValueError, 'identity differs'):
                journey.firmware(path, '0' * 64)

    def test_windows_checkout_conversion_preserves_exact_blob_inputs_and_rejects_real_edits(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            samples = root / 'samples'
            samples.mkdir()
            blob = b'org 32768\n ld a,2\n'
            (samples / 'demo.asm').write_bytes(blob.replace(b'\n', b'\r\n'))
            revision = 'a' * 40
            pins = {'samples_revision': revision, 'files': {'demo.asm': journey.digest(blob)}}
            replies = [CompletedProcess([], 0, (revision + '\n').encode()), CompletedProcess([], 0, blob)]
            with patch.object(journey.subprocess, 'run', side_effect=replies):
                staged, records = journey.stage_sources(samples, root / 'work', pins)
            self.assertEqual((staged / 'demo.asm').read_bytes(), blob)
            self.assertEqual(records[0]['checkout_conversion'], 'CRLF')
            self.assertEqual(records[0]['materialised_sha256'], pins['files']['demo.asm'])
            (samples / 'demo.asm').write_bytes(b'org 32768\r\n ld a,4\r\n')
            with patch.object(journey.subprocess, 'run', side_effect=replies), self.assertRaisesRegex(ValueError, 'real change'):
                journey.stage_sources(samples, root / 'changed', pins)

    def test_workflow_is_opt_in_and_keeps_the_same_sample_pin(self) -> None:
        workflow = (Path(__file__).parents[1] / '.github/workflows/test-linux-packages.yml').read_text()
        pins = json.loads(journey.PINS.read_text())
        self.assertIn(pins['samples_revision'], workflow)
        self.assertIn('if: ${{ !inputs.windows_developer_journey }}', workflow)
        self.assertIn("github.event_name == 'workflow_dispatch' && inputs.windows_developer_journey", workflow)
        self.assertIn('ROM_B64: ${{ secrets.EMU198X_SPECTRUM_48K_ROM_B64 }}', workflow)
        self.assertIn('name: Remove private Windows-journey firmware\n        if: always()', workflow)


if __name__ == '__main__':
    unittest.main()
