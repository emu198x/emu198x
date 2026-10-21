# Apple TV app: every known version of a game

Steve, 2026-10-21: support the 128K versions of the Dizzy games, and in
time every known version. The library keeps one cover per game; selecting
it starts the default version, and a long press lists the others.

Follows step 5 of `2026-10-20-apple-native-host-spike.md`, where the app
plays only the 48K.

## What is known

From TOSEC's Spectrum set (`~/Reference-Index/data-index.txt`), the
Codemasters Dizzy games come as:
- **48K-128K tapes**: one tape for both machines. On a 128K the loader
  reads extra blocks, the AY music, that the 48K skips. Every game has
  one except Dizzy (1987), whose only tape predates the 128K.
- **"(128K)" tapes**: TZX for Fantasy World, Spellbound and Fast Food;
  TAP for Treasure Island, Magicland, Prince of the Yolkfolk and Crystal
  Kingdom. These look like other dumps of the same releases, not other
  versions: Fast Food's differs from its 48K-128K tape in 11 bytes, all in
  the first 40 (TZX block headers), and Fantasy World's in 146. They are
  left out unless their data turns out to differ.
- **Compilations**: The Dizzy Collection (1991, three tape sides) and
  Dizzy's Excellent Adventures (1991).
- **Snapshots** (Z80, 48K and 128K): copies, not releases. Left out.

No Codemasters +3 disk version is in TOSEC. The TR-DOS Treasure Island
is a Russian conversion.

The runtime already plays all of these on a 128K: `autoload_basic_tape`
takes the 128K loader menu (checked headlessly with Treasure Island and
Fantasy World, which reach their titles with the tape stopped and sound
playing). What is missing is in the app: it builds only the 48K
and reads only `48.rom`.

## Design

- A game is described by `<Game>.game.json` beside its cover in
  `LocalContent`. Each version names its file, its machine and its
  controls; the first is the default:

  ```json
  {"versions": [
    {"name": "128K", "file": "Treasure Island Dizzy.tzx",
     "machine": "spectrum_128k",
     "controls": {"stick": "kempston", "selectKeys": ["KeyK"]}},
    {"name": "48K", "file": "Treasure Island Dizzy.tzx",
     "machine": "spectrum_48k",
     "controls": {"stick": "kempston", "selectKeys": ["KeyK"]}}
  ]}
  ```

  A program no manifest names still shows on its own, as now, on the
  48K with its `<name>.controls.json`.
- ROMs sit beside `48.rom` as `128-0.rom` and `128-1.rom`, the names the
  desktop hosts use. Each is accepted only as the published image: the
  48K by `test-rom-policy.md`, the 128K by MAME's `spec128` driver
  (English BIOS: `zx128_0.rom` SHA-1
  `4f4b11ec22326280bdb96e3baf9db4b4cb1d02c5`, `zx128_1.rom`
  `80080644289ed93d71a1103992a154cc9802b2fa`). A version whose ROMs are
  missing says so when chosen.
- What the player changes in Controls is saved per game and version.

## Tasks

1. `Shared/SpectrumFirmware.swift`: firmware per machine (ids, file
   names, SHA-1s); load a machine's set from a folder or the bundle.
   Verify: the Mac app still boots the 48K from `~/.emu198x/roms`.
2. `Shared/SpectrumSession.swift`: start a given machine with its
   firmware; `play` takes a version. Verify: Mac and TV builds.
3. `tvOS/LocalLibrary.swift`: read manifests into games with versions;
   loose programs become one-version games. Verify: the library lists the
   twelve games once each.
4. `tvOS/Emu198xSpectrumTVApp.swift`: select starts the default version;
   a long press lists the versions. Verify in the tvOS simulator.
5. Content (local, not committed): manifests for the twelve, with 128K
   versions and the 128K-only tapes, after checking each version's select
   keys headlessly on its machine.
6. Verify end to end: one 128K version with Steve's controller in the Mac
   app.

## Findings

Each of the twelve was autoloaded headlessly on the 128K with the
Kempston plugged in from power-on, then given its select keys and driven
right:
- Eleven reach their games. On the 128K the whole tape loads, music
  included, so the tape stops when the game is in and the select keys no
  longer wait (Kwik Snax waited about 4 minutes on the 48K).
- Crystal Kingdom Dizzy takes 3, 1, 1 on the 128K against 3, 1 on the
  48K, so each version carries its own controls.
- Dizzy (1987) plays its tape through and then stays on its loading
  screen (#1924), so it has only a 48K version for now.

In the tvOS simulator the library shows one cover per game with its
versions beneath ("128K · 48K"), and Treasure Island Dizzy's default
version boots the 128K, loads through the 128K's Tape Loader and reaches
its title screen. The long press that lists versions has not been tried
there: the simulator's remote is driven by hand.

Later: the two compilations (a menu per tape, games on more than one
side), then the +2 and +3.
