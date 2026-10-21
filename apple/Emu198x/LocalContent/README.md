# Local content for the Apple TV app

An Apple TV has no file picker, so the tvOS app plays what is built into
it. Everything in this folder except this README is git-ignored and is
copied into local builds of the tvOS target.

`scripts/bootstrap-apple-local-content.py` fills it from a recipe in
`apple/content` and your own TOSEC set, World of Spectrum archive and
`~/.emu198x/roms`. To add things by hand:

- `48.rom`: the Sinclair 48K ROM, unmodified (SHA-1
  `5ea7c2b824672e914525d1d5c419d71b84a426a2`). The app refuses any other
  image. Copy it from `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
- `128-0.rom` and `128-1.rom`: the Sinclair 128K ROMs, for 128K versions,
  unmodified (the English images MAME's `spec128` driver lists). Copy
  them from `~/.emu198x/roms/sinclair-zx-spectrum-128k/`.
- `basic.rom`, `kernal.rom` and `chargen.rom`: the C64 ROMs, for C64
  versions, unmodified (901226-01, 901227-03 and 901225-01, as MAME's
  `c64` driver lists them). Copy them from `~/.emu198x/roms/commodore-c64/`.
- `kick13.rom` and `kick31a1200.rom`: Kickstart 1.3 for the A500 and 3.1
  for the A1200, unmodified (315093-02 as MAME's `amiga` driver lists it,
  and 40.68 as WinUAE's ROM list does). Copy them from
  `~/.emu198x/roms/commodore-amiga/`.
- Programs: `.tap` and `.tzx` tapes (autoloaded: the runtime types
  `LOAD ""` and presses play), `.sna` and `.z80` snapshots, and `.state`
  files saved with Save State in the Mac app.
- Cartridges: `.nes` for the NES (NTSC), `.sms` for the Master System
  (PAL) and `.gg` for the Game Gear. Rename a TOSEC `.bin` to match. They
  need no ROM. The controller is the console's own: the bottom face
  button is NES B or Sega button 1, the right one NES A or button 2,
  Options (View) is Start (Pause on the Master System) and the left
  shoulder is the NES's Select.
- Amiga disks: `.adf`, on an A500 unless a version says `amiga_1200`.
  Every disk a version lists is in a drive before the Amiga starts, the
  first in DF0 and the rest in DF1 onwards, so a two-disk game finds its
  second disk without a swap.
- Box art (optional): `<name>.jpg` or `<name>.png` beside a program or
  game is its cover in the library. A cassette inlay's front panel, about
  5:8, fits best.
- Versions (optional): `<Game>.game.json` makes one library entry of
  several ways to play a game. Each version names its file, its machine
  (`spectrum_48k`, `spectrum_128k`, `c64`, `amiga_500`, `amiga_1200`,
  `nes`, `master_system` or `game_gear`), any further disks (`"disks":
  ["Game 2.adf"]`) and, on a computer, its controls, as below; the
  first is the one selecting the game plays, and a long press offers the
  rest:
  `{"versions": [{"name": "128K", "file": "Fantasy World Dizzy.tzx",
  "machine": "spectrum_128k", "controls": {"stick": "kempston"}},
  {"name": "48K", "file": "Fantasy World Dizzy.tzx",
  "machine": "spectrum_48k", "controls": {"stick": "kempston"}}]}`.
  A program no game lists has an entry of its own, on the 48K or, for a
  cartridge, its console. A C64 tape (`.tap`, autoloaded with
  SHIFT+RUN/STOP) needs a version naming `c64`, since a loose `.tap` is
  taken for a Spectrum tape.
- Controls (optional): `<name>.controls.json` beside a program sets how a
  controller plays it. What the player changes under Pause, Controls is
  kept as changes on top of it, so a later fix to the file still applies
  to everything they left alone, and Reset to the Game's Settings drops
  their changes.
  Any field may be left out, for example
  `{"stick": "kempston", "selectKeys": ["KeyK"]}` for Dizzy. `stick` is
  `kempston`, `sinclair`, `cursor` or `qaop` on the Spectrum, `port2` or
  `port1` on the C64, and `port2` on the Amiga; `selectKeys` are the keys
  the game asks for, in order, to choose a joystick and start (Crystal
  Kingdom Dizzy takes `["Digit3", "Digit1"]`), pressed on the player's
  first move once the tape has stopped; `keys` maps `b`, `x`, `y`,
  `leftShoulder` and `rightShoulder` to keys. Keys are browser key codes
  (`KeyK`, `Digit1`, `Space`, `Enter`); on the Amiga, `MouseLeft` and
  `MouseRight` are the mouse buttons, and B is the left one unless a
  game says otherwise.

Builds that contain these files are for your own devices. Do not commit
them or distribute such a build: `knowledge/decisions/test-rom-policy.md`
allows the 48K ROM only in the published browser builds, and game
software stays its authors' property.
