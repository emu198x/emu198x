# Local content for the Apple TV app

An Apple TV has no file picker, so the tvOS app plays what is built into
it. Everything in this folder except this README is git-ignored and is
copied into local builds of the tvOS target.

- `48.rom`: the Sinclair 48K ROM, unmodified (SHA-1
  `5ea7c2b824672e914525d1d5c419d71b84a426a2`). The app refuses any other
  image. Copy it from `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
- `128-0.rom` and `128-1.rom`: the Sinclair 128K ROMs, for 128K versions,
  unmodified (the English images MAME's `spec128` driver lists). Copy
  them from `~/.emu198x/roms/sinclair-zx-spectrum-128k/`.
- Programs: `.tap` and `.tzx` tapes (autoloaded: the runtime types
  `LOAD ""` and presses play), `.sna` and `.z80` snapshots, and `.state`
  files saved with Save State in the Mac app.
- Box art (optional): `<name>.jpg` or `<name>.png` beside a program or
  game is its cover in the library. A cassette inlay's front panel, about
  5:8, fits best.
- Versions (optional): `<Game>.game.json` makes one library entry of
  several ways to play a game. Each version names its file, its machine
  (`spectrum_48k` or `spectrum_128k`) and its controls, as below; the
  first is the one selecting the game plays, and a long press offers the
  rest:
  `{"versions": [{"name": "128K", "file": "Fantasy World Dizzy.tzx",
  "machine": "spectrum_128k", "controls": {"stick": "kempston"}},
  {"name": "48K", "file": "Fantasy World Dizzy.tzx",
  "machine": "spectrum_48k", "controls": {"stick": "kempston"}}]}`.
  A program no game lists has an entry of its own, on the 48K.
- Controls (optional): `<name>.controls.json` beside a program sets how a
  controller plays it. What the player changes under Pause, Controls is
  kept as changes on top of it, so a later fix to the file still applies
  to everything they left alone, and Reset to the Game's Settings drops
  their changes.
  Any field may be left out, for example
  `{"stick": "kempston", "selectKeys": ["KeyK"]}` for Dizzy. `stick` is
  `kempston`, `sinclair`, `cursor` or `qaop`; `selectKeys` are the keys
  the game asks for, in order, to choose a joystick and start (Crystal
  Kingdom Dizzy takes `["Digit3", "Digit1"]`), pressed on the player's
  first move once the tape has stopped; `keys` maps `b`, `x`, `y`,
  `leftShoulder` and `rightShoulder` to keys. Keys are browser key codes
  (`KeyK`, `Digit1`, `Space`, `Enter`).

Builds that contain these files are for your own devices. Do not commit
them or distribute such a build: `knowledge/decisions/test-rom-policy.md`
allows the 48K ROM only in the published browser builds, and game
software stays its authors' property.
