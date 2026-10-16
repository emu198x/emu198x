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

    def test_native_header_identity_rejects_the_other_host_and_architecture(self) -> None:
        pe = bytearray(512)
        pe[:2] = b'MZ'
        struct.pack_into('<I', pe, 0x3C, 0x80)
        pe[0x80:0x84] = b'PE\0\0'
        struct.pack_into('<H', pe, 0x84, 0x8664)
        struct.pack_into('<H', pe, 0x80 + 24, 0x20B)
        struct.pack_into('<Q', pe, 0x80 + 24 + 72, 1048576)
        self.assertEqual(journey.executable_identity(bytes(pe), 'x86_64-pc-windows-msvc'), 1048576)
        elf = bytearray(64)
        elf[:6] = b'\x7fELF\x02\x01'
        struct.pack_into('<H', elf, 18, 62)
        self.assertIsNone(journey.executable_identity(bytes(elf), 'x86_64-unknown-linux-gnu'))
        for image, target in [(pe, 'x86_64-unknown-linux-gnu'), (elf, 'x86_64-pc-windows-msvc')]:
            with self.assertRaises(ValueError):
                journey.executable_identity(bytes(image), target)
        struct.pack_into('<H', elf, 18, 183)
        with self.assertRaises(ValueError):
            journey.executable_identity(bytes(elf), 'x86_64-unknown-linux-gnu')

    def test_macho_rejects_the_other_architecture_and_other_formats(self) -> None:
        image = bytes.fromhex('cffaedfe07000001') + bytes(24)
        self.assertIsNone(journey.executable_identity(image, 'x86_64-apple-darwin'))
        for target in ['aarch64-apple-darwin', 'x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu']:
            with self.assertRaises(ValueError):
                journey.executable_identity(image, target)
        arm = bytes.fromhex('cffaedfe0c000001') + bytes(24)
        self.assertIsNone(journey.executable_identity(arm, 'aarch64-apple-darwin'))
        with self.assertRaises(ValueError):
            journey.executable_identity(arm, 'x86_64-apple-darwin')

    def test_retained_source_stop_requires_agreement_and_rejects_wrong_line(self) -> None:
        root = journey.FIXTURES
        record = json.loads((root / 'reference-runtime.json').read_text())
        journey.check_source_debug(root, record)
        record['observations'] = [dict(r) for r in record['observations']]
        stop = next(r for r in record['observations'] if r['kind'] == 'run_until_line')
        stop['stopped_at'] = {'file': 'first-program.asm', 'line': 7}
        with self.assertRaisesRegex(ValueError, 'wrong stopped source location'):
            journey.check_source_debug(root, record)

    def test_native_manifests_share_fixture_and_release_set_but_bind_their_own_hosts(self) -> None:
        windows = json.loads(journey.PINS.read_text())
        linux = json.loads(journey.PINS.with_name('linux-developer-journey-inputs.json').read_text())
        for key in ['samples_revision', 'sample_path', 'files', 'firmware_sha256', 'frames', 'source_debug_fixture_sha256', 'source_debug_session_sha256']:
            self.assertEqual(windows[key], linux[key])
        self.assertEqual(windows['target'], 'x86_64-pc-windows-msvc')
        self.assertEqual(linux['target'], 'x86_64-unknown-linux-gnu')
        for a, b in zip(windows['tools'], linux['tools'], strict=True):
            self.assertEqual((a['name'], a['release']), (b['name'], b['release']))
            for tool, pins in [(a, windows), (b, linux)]:
                self.assertIn(pins['target'], tool['archive'])
                self.assertRegex(tool['sha256'], r'^[a-f0-9]{64}$')
                self.assertTrue(tool['url'].endswith('/' + tool['archive']))

    def test_workflow_is_opt_in_and_keeps_the_same_sample_pin(self) -> None:
        workflow = (Path(__file__).parents[1] / '.github/workflows/test-linux-packages.yml').read_text()
        pins = json.loads(journey.PINS.read_text())
        self.assertIn(pins['samples_revision'], workflow)
        self.assertIn('if: ${{ !inputs.windows_startup_regression && !inputs.windows_developer_journey && !inputs.linux_developer_journey && !inputs.macos_developer_journey && !inputs.installer_journey }}', workflow)
        self.assertIn("github.event_name == 'workflow_dispatch' && inputs.windows_developer_journey", workflow)
        self.assertIn('ROM_B64: ${{ secrets.EMU198X_SPECTRUM_48K_ROM_B64 }}', workflow)
        self.assertIn('name: Remove private Windows-journey firmware\n        if: always()', workflow)


if __name__ == '__main__':
    unittest.main()
