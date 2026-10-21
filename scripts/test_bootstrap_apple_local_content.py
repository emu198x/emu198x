"""Tests for bootstrap-apple-local-content.py that need no collections."""

import importlib.util
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

SCRIPT = Path(__file__).with_name("bootstrap-apple-local-content.py")
spec = importlib.util.spec_from_file_location("bootstrap", SCRIPT)
bootstrap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bootstrap)


class CropTests(unittest.TestCase):
    def test_an_inlay_keeps_the_part_right_of_its_fraction(self):
        self.assertEqual(bootstrap.crop_rectangle({"left": 0.535}, 531, 388), (284, 0, 247, 388))

    def test_a_pixel_rectangle_is_taken_as_given(self):
        cover = {"crop": [664, 0, 580, 900]}
        self.assertEqual(bootstrap.crop_rectangle(cover, 2915, 900), (664, 0, 580, 900))

    def test_a_front_on_its_own_is_kept_whole(self):
        self.assertEqual(bootstrap.crop_rectangle({}, 200, 320), (0, 0, 200, 320))


class TapeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)

    def zip_with(self, *names):
        path = self.directory / "game.zip"
        with zipfile.ZipFile(path, "w") as archive:
            for name in names:
                archive.writestr(name, b"tape")
        return path

    def test_the_one_tape_is_named_after_the_game(self):
        name = bootstrap.extract_tape(self.zip_with("Game (1989)(Code Masters).TZX"), self.directory / "Game")
        self.assertEqual(name, "Game.tzx")
        self.assertEqual((self.directory / "Game.tzx").read_bytes(), b"tape")

    def test_a_zip_with_two_tapes_is_refused(self):
        with self.assertRaises(bootstrap.BootstrapError):
            bootstrap.extract_tape(self.zip_with("a.tzx", "b.tap"), self.directory / "Game")


class FirmwareTests(unittest.TestCase):
    def test_a_modified_rom_is_refused(self):
        with tempfile.TemporaryDirectory() as roms, tempfile.TemporaryDirectory() as out:
            rom = Path(roms) / "sinclair-zx-spectrum-48k/48.rom"
            rom.parent.mkdir()
            rom.write_bytes(b"\0" * 16384)
            with self.assertRaises(bootstrap.BootstrapError):
                bootstrap.copy_firmware({"spectrum_48k"}, Path(roms), Path(out))
            self.assertFalse((Path(out) / "48.rom").exists())

    def test_a_missing_rom_names_the_machine(self):
        with tempfile.TemporaryDirectory() as roms, tempfile.TemporaryDirectory() as out:
            with self.assertRaisesRegex(bootstrap.BootstrapError, "spectrum_128k"):
                bootstrap.copy_firmware({"spectrum_128k"}, Path(roms), Path(out))


class CoverTests(unittest.TestCase):
    def test_a_changed_download_is_refused(self):
        response = mock.Mock()
        response.read.return_value = b"not the pinned image"
        with tempfile.TemporaryDirectory() as scratch, mock.patch.object(bootstrap, "urlopen", return_value=response):
            with self.assertRaises(bootstrap.BootstrapError):
                bootstrap.fetch("https://example.invalid/cover.jpg", "0" * 64, Path(scratch))


class ManifestTests(unittest.TestCase):
    def test_every_version_names_the_tape(self):
        game = {"versions": [{"name": "128K", "machine": "spectrum_128k"}, {"name": "48K", "machine": "spectrum_48k"}]}
        manifest = bootstrap.manifest(game, "Game.tzx")
        self.assertEqual([v["file"] for v in manifest["versions"]], ["Game.tzx", "Game.tzx"])
        self.assertEqual(manifest["versions"][0]["machine"], "spectrum_128k")


class RecipeTests(unittest.TestCase):
    """The committed recipes, checked against what the script and the app
    accept, so a slip shows here rather than on a television."""

    # SpectrumControls.keys in apple/Emu198x/Shared/SpectrumControls.swift.
    KEY_CODES = (
        {"Space", "Enter", "ShiftLeft", "AltLeft"}
        | {f"Digit{d}" for d in "1234567890"}
        | {f"Key{c}" for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ"}
    )
    STICKS = {"kempston", "sinclair", "cursor", "qaop"}

    def test_every_recipe_names_what_the_script_and_app_accept(self):
        recipes = sorted((SCRIPT.parent.parent / "apple/content").glob("*.json"))
        self.assertTrue(recipes, "no recipes found")
        for recipe in recipes:
            games = bootstrap.json.loads(recipe.read_text())["games"]
            self.assertEqual(len({g["name"] for g in games}), len(games), f"{recipe.name}: duplicate names")
            for game in games:
                where = f"{recipe.name}: {game['name']}"
                self.assertTrue(game["tape"].endswith(".zip"), where)
                cover = game["cover"]
                self.assertTrue("wos" in cover or ("url" in cover and len(cover.get("sha256", "")) == 64), where)
                self.assertTrue(game["versions"], where)
                for version in game["versions"]:
                    self.assertIn(version["machine"], bootstrap.FIRMWARE, where)
                    controls = version.get("controls", {})
                    self.assertIn(controls.get("stick", "kempston"), self.STICKS, where)
                    for code in controls.get("selectKeys", []) + list(controls.get("keys", {}).values()):
                        self.assertIn(code, self.KEY_CODES, where)


if __name__ == "__main__":
    unittest.main()
