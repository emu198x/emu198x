# Apple TV app: Dizzy on other systems

Steve, 2026-10-21: add the Dizzy games' other systems to the Apple TV
collection. He chose one app, renamed Emu198x, in which a system is one
more kind of version: Treasure Island Dizzy offers the Spectrum 128K and
48K, the Commodore 64, the Amstrad CPC and the Amiga under one cover. It
builds on `2026-10-21-apple-tv-game-versions.md`.


## What is known

From `~/Reference-Index/data-index.txt` (TOSEC, Codemasters and Camerica
releases, leaving out alternate dumps; where only a cracked copy exists,
the table says so):

| System | Dizzy releases | Format | Core | State |
|---|---|---|---|---|
| NES | Fantastic Adventures of Dizzy; Dizzy the Adventurer, each plain and Aladdin Deck Enhancer | iNES | `runtime-nintendo-nes` | cartridge, no firmware |
| Game Gear | Fantastic Dizzy; The Excellent Dizzy Collection | ROM | `runtime-sega-game-gear` | cartridge, no firmware |
| Master System | Fantastic Dizzy | ROM | `runtime-sega-master-system` | cartridge, no firmware |
| Commodore 64 | Dizzy 1 to 6, Bubble, Down the Rapids, Kwik Snax, Panic, Crystal Kingdom | TAP, also D64 and PRG | `runtime-commodore-c64` | tape and disk autoload exist in the runtime; firmware in `~/.emu198x/roms` |
| Amstrad CPC | Dizzy 1 to 7, Bubble, Down the Rapids, Fast Food; Crystal Kingdom, Bubble, Down the Rapids and Fast Food only as cracked copies | DSK only | `runtime-amstrad-cpc` | blocked: the core is a CPC 464 with a tape slot and no disk drive (#499) |
| Amiga | Dizzy 2 to 7, Bubble, Panic, Fast Food, Fantastic Dizzy, two compilations | IPF (SPS); ADF as cracks, and two originals | `runtime-commodore-amiga` | ADFs now, as below; IPF originals later (#105, #1284). Kickstarts 1.3 and 3.1 are in `~/.emu198x/roms` |
| Atari ST, Mega Drive, SNES, Enterprise | several | | none | out of reach |

Amiga (Steve, 2026-10-21): the original disks were copy-protected, so
the only images of most of them that an ADF can hold are scene cracks;
the uncracked originals are SPS's IPF images. Until the core reads IPF,
the collection uses the cleanest crack of each, says so in the version
name ("Amiga (SR crack)"), and swaps in the IPF original later. Cleanest
means no trainer, bad dump or alternate where there is a choice, and
never a copy flagged as carrying a virus (some Dizzy's Excellent
Adventures disks are, `[v Saddam 1]`):

| Game | ADF |
|---|---|
| Treasure Island Dizzy | the 1989 original, uncracked |
| Fantastic Dizzy | the 1994 AGA original, uncracked, two disks (A1200) |
| Fantasy World Dizzy | `[cr SR]` |
| Magicland Dizzy | `[cr TRSI]` |
| Fast Food | `[cr SR]` |
| Kwik Snax | `[cr FLT][t ATX]`, the only one, with a trainer |
| Dizzy Panic | `[cr ATX]` |
| Bubble Dizzy | v1.01 `[cr SR]` |
| Prince of the Yolkfolk | `[cr Flashtro]` |
| Crystal Kingdom Dizzy | `[cr FLT]`, two disks |
| The Dizzy Collection | `[cr SR]`, two disks |
| Dizzy's Excellent Adventures | `[cr Flashtro]`, two disks |

Spellbound Dizzy has no ADF of its own; which games each compilation
holds is to be checked. Easter Eggstravaganza, an Amiga Action coverdisk
game, exists only as a hacked copy and is left out.

Spectrum disks: no Codemasters disk release of a Dizzy game is known.
TOSEC's Spectrum disk images of them are TR-DOS conversions by scene
groups, all flagged as hacks, so they are left out.

## Findings (step 1, 2026-10-21)

Headless, on the desktop binaries:

- NES: both Camerica cartridges reach their titles and play; Start on
  input port 1 starts them.
- Master System and Game Gear: Codemasters cartridges drew garbage until
  the Codemasters mapper landed (#204, #214). Fantastic Dizzy now plays
  on both; on the Game Gear it starts with button 1 after a long intro.
  The Game Gear Excellent Dizzy Collection is a Master System-mode
  cartridge, shown cropped by the Game Gear core (#1932), so the app runs
  it on the Master System.
- C64: all twelve tapes autoload and reach their titles; the joystick in
  port 2 starts most, Spellbound wants Space. Through Fleet every C64 tape
  read as blank, because Fleet mounted it writable, which the C64 takes as
  a blank SAVE tape (#1953). A C64 loader stops the motor for up to 12
  seconds mid-load, so the app counts a C64 tape as loading until it has
  played to its end.
- Amiga: crack intros wait for the left mouse button; Bubble, Fantasy
  World and Magicland then play. Fantastic Dizzy (AGA) and Crystal
  Kingdom need their second disk, and Fantastic Dizzy reads both, so the
  machine takes up to four drives (Steve, 2026-10-21; its own plan,
  `2026-10-21-amiga-four-drives.md`, #1956). Through the app's path, with
  every disk in before boot: Fantastic Dizzy (A1200) reaches its title
  from two disks, Crystal Kingdom its title, Bubble its title, and
  Fantasy World, Magicland and Fast Food play. Prince of the Yolkfolk,
  Dizzy Panic, Kwik Snax and both compilations stay black or blank after
  the intro, and the uncracked Treasure Island draws its play screen torn:
  Amiga core faults, filed separately.

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
   Each console's controller is its own pad, by button position; Options
   (View) is Start because Menu pauses.
4. C64: tape autoload through `Fleet` (asked for first), joystick presets
   for port 2, keys per game. Verify as for the Spectrum.
5. The bootstrap recipe gains the new versions and their covers.

6. Amiga: the ADFs above on the machine each needs (A500 with Kickstart
   1.3; the A1200 for Fantastic Dizzy's AGA version), with a way to
   change disks for the two-disk games.

Later, when the cores can: CPC disks (#499), and the Amiga IPF originals
(#105, #1284) in place of the cracks.
