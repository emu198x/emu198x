# Apple TV app: Dizzy on other systems

Steve, 2026-10-21: add the Dizzy games' other systems to the Apple TV
collection. He chose one app, renamed Emu198x, in which a system is one
more kind of version: Treasure Island Dizzy offers the Spectrum 128K and
48K, the Commodore 64, the Amstrad CPC and the Amiga under one cover. It
builds on `2026-10-21-apple-tv-game-versions.md`.

Waiting on disk space: Steve, 2026-10-21, asked to hold the builds until
there is room (about 15 GB for the other cores and Xcode).

## What is known

From `~/Reference-Index/data-index.txt` (TOSEC, Codemasters and Camerica
releases, no alternates or cracks):

| System | Dizzy releases | Format | Core | State |
|---|---|---|---|---|
| NES | Fantastic Adventures of Dizzy; Dizzy the Adventurer, each plain and Aladdin Deck Enhancer | iNES | `runtime-nintendo-nes` | cartridge, no firmware |
| Game Gear | Fantastic Dizzy; The Excellent Dizzy Collection | ROM | `runtime-sega-game-gear` | cartridge, no firmware |
| Master System | Fantastic Dizzy | ROM | `runtime-sega-master-system` | cartridge, no firmware |
| Commodore 64 | Dizzy 1 to 6, Bubble, Down the Rapids, Kwik Snax, Panic, Crystal Kingdom | TAP, also D64 and PRG | `runtime-commodore-c64` | tape and disk autoload exist in the runtime; firmware in `~/.emu198x/roms` |
| Amstrad CPC | Dizzy 1 to 7, Bubble, Down the Rapids, Fast Food; Crystal Kingdom, Bubble, Down the Rapids and Fast Food only as cracked copies | DSK only | `runtime-amstrad-cpc` | blocked: the core is a CPC 464 with a tape slot and no disk drive (#499) |
| Amiga | Dizzy 2 to 7, Bubble, Panic, Fast Food, Fantastic Dizzy, two compilations | IPF (SPS); ADF for two | `runtime-commodore-amiga` | blocked: IPF is not supported (#105, #1284); the two ADFs could run |
| Atari ST, Mega Drive, SNES, Enterprise | several | | none | out of reach |

Spectrum disks: no Codemasters disk release of a Dizzy game is known.
TOSEC's Spectrum disk images of them are TR-DOS conversions by scene
groups, all flagged as hacks, so they are left out.

## Design

- A version gains a system: `"machine"` becomes a family and variant
  (`sinclair-zx-spectrum` / `spectrum_128k`, `nintendo-nes` / its variant),
  so `<Game>.game.json` can list versions across systems. Existing
  Spectrum manifests keep working.
- Per family, the app needs: its firmware (none for the cartridge
  systems), how a program starts (insert a cartridge; type and run a tape
  load), and controller presets (a console's pad maps straight to its
  controller one; a computer's joystick needs presets like the
  Spectrum's).
- The app is renamed Emu198x, with the bundle id and product name to
  match; the library is one shelf of games.
- Starting a tape on anything but the Spectrum needs `Fleet` to expose
  that family's autoload, as it does the Spectrum's. That is a change to
  `emu198x-fleet-web`, so it is asked for when the C64 step reaches it.

## Steps

1. Feasibility, headless, before any app work: each release above on its
   desktop binary, to its title and into the game with a controller.
   Records what each needs (keys, mapper, region).
2. The app learns families: `SpectrumModel` becomes a machine with family,
   variant, firmware and start method; the session builds whichever it is
   given; the xcframework is built with those families' features. Verify:
   the Spectrum set plays as before.
3. Cartridge systems: NES, Game Gear and Master System versions, with
   their covers. Verify in the simulator and with Steve's controller.
4. C64: tape autoload through `Fleet` (asked for first), joystick presets
   for port 2, keys per game. Verify as for the Spectrum.
5. The bootstrap recipe gains the new versions and their covers.

Later, when the cores can: CPC disks (#499) and Amiga IPF (#105, #1284).
