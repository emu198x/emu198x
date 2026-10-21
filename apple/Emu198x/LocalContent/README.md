# Local content for the Apple TV app

An Apple TV has no file picker, so the tvOS app plays what is built into
it. Everything in this folder except this README is git-ignored and is
copied into local builds of the tvOS target.

- `48.rom`: the Sinclair 48K ROM, unmodified (SHA-1
  `5ea7c2b824672e914525d1d5c419d71b84a426a2`). The app refuses any other
  image. Copy it from `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
- Programs: `.tap` and `.tzx` tapes (autoloaded: the runtime types
  `LOAD ""` and presses play), `.sna` and `.z80` snapshots, and `.state`
  files saved with Save State in the Mac app.
- Controls (optional): `<name>.controls.json` beside a program sets how a
  controller plays it until the player changes it under Pause, Controls.
  Any field may be left out, for example
  `{"stick": "kempston", "selectKey": "KeyK"}` for Dizzy. `stick` is
  `kempston`, `sinclair`, `cursor` or `qaop`; `selectKey` is the key the
  game asks for to choose a joystick, pressed on the player's first move
  once the tape has stopped; `keys` maps `b`, `x`, `y`, `leftShoulder`
  and `rightShoulder` to keys. Keys are browser key codes (`KeyK`,
  `Digit1`, `Space`, `Enter`).

Builds that contain these files are for your own devices. Do not commit
them or distribute such a build: `knowledge/decisions/test-rom-policy.md`
allows the 48K ROM only in the published browser builds, and game
software stays its authors' property.
