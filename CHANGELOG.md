# Changelog

All notable changes to Emu198x will be documented in this file.

Format loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
not strictly. Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
## [0.18.0] - 2026-09-01


### Added

- Read Atari ATR disk images
- Give POKEY's serial port a wire
- Put a disk drive on the SIO bus
- *(atari)* --disk loads an ATR into D1: on the 800XL


### Fixed

- *(atari)* --no-basic takes effect with the real XL OS

## [0.17.0] - 2026-09-01


### Added

- *(mtx)* Parse .mtx tape images and .RUN programs
- *(atari)* Honour GRACTL and VDELAY in the render path
- *(atari)* Latch the triggers when GRACTL asks
- *(atari)* Give overlapping players their third colour
- *(atari)* Implement ANTIC fine scrolling — HSCROL and VSCROL now move the
  playfield, verified against real firmware
- *(atari)* Steal the DMA cycles ANTIC actually takes, at the fetch positions
  the hardware uses rather than a count spread across the line


### Fixed

- *(atari)* Draw ANTIC modes 8, 9 and A across the full playfield width —
  GRAPHICS 3, 4 and 5 were confined to the left quarter or half of the screen
- *(atari)* Let the 800XL type shifted characters; KBCODE's Shift and Control
  bits were swapped, so every `(`, `"` or `?` reached the OS as a command
- *(atari)* Stop a full-line ANTIC DMA budget underflowing the stall window
- *(atari)* Let ANTIC fetch through the machine's live memory, so a display
  list or character set changed mid-frame is visible on that frame
- *(atari)* Read AUDCTL's bits from the end the register starts at; six of the
  eight were transposed
- *(atari)* Clock POKEY's serial output from the timer that drives it, in place
  of two hand-tuned delays

## [0.16.0] - 2026-09-01


### Added

- *(aquarius)* Give the machine its voice
- *(pet)* Give the machine its voice
- *(pet)* Let the PET load a program instead of only typing one
- *(jupiter-ace)* Load .ace snapshots


### Fixed

- *(vic-20)* Give a $1201 PRG the whole 8K expansion block
- **Breaking** — *(vic-20)* Model each RAM expansion cartridge independently
- *(aquarius)* Move the speaker to the pin the hardware puts it on

## [0.15.0] - 2026-09-01


### Added

- Plug an ESP-AT modem into the C64 user port
- Teach the modem Hayes dialing so C64-era clients can connect
- Emulate the Ultimate Command Interface network target
- *(uci-net)* Answer network commands the way the hardware does

## [0.14.0] - 2026-08-31


### Added

- *(atari-800xl)* Load XEX executables

## [0.13.3] - 2026-08-31


### Fixed

- Wire SVI cartridge bank control

## [0.13.2] - 2026-08-31


### Fixed

- *(spectrum)* Terminate TZX playback at end of file
- *(atari)* Implement GTIA PRIOR schemes

## [0.13.1] - 2026-08-31


### Fixed

- Complete allocation-free SID draining
- Model VIC-20 ESP reconnect state

## [0.13.0] - 2026-08-31


### Added

- Add allocation-free SID buffer drain

## [0.12.2] - 2026-08-31


### Fixed

- *(atari)* Verify audible POKEY output

## [0.12.1] - 2026-08-31


### Fixed

- *(einstein)* Remove fictional NTSC configuration

## [0.12.0] - 2026-08-30


### Added

- Persist Master System cartridge saves
- *(vic-20)* Emulate ESP-AT baud changes

## [0.11.1] - 2026-08-30


### Fixed

- Implement Master System cartridge SRAM

## [0.11.0] - 2026-08-30


### Added

- Add VIC-20 cycle serial adapter
- Bridge VIC-20 ESP-AT to TCP


### Fixed

- Integrate Jupiter Ace beeper samples

## [0.10.0] - 2026-08-30


### Added

- Expose VIC-20 serial user-port pins


### Fixed

- *(aquarius)* Require character ROM firmware
- *(master-system)* Parse cartridge headers
- *(svi-328)* Add cassette media path
- *(atari-7800)* Synthesize TIA audio

## [0.9.0] - 2026-08-30


### Added

- *(oric)* Load TAP cassettes through VIA

## [0.8.2] - 2026-08-30


### Fixed

- Align 128K floating-bus reads with hardware
- Model 6845 zero sync widths by variant
- Implement SMS memory control port
- Expose SMS region through TH readback
- Render Oric serial text attributes

## [0.8.1] - 2026-08-30


### Fixed

- Identify unsupported Amiga IPF disks
- Expose Einstein disk media through runtime
- Detect MSX MegaROM mappers on media load
- Model the standard MSX M1 wait state
- Preserve distinct ZX80 and ZX81 picture origins
- Select the GTIA palette by television standard
- Correct Atari GTIA and ANTIC edge cases
- Correct POKEY linked-channel dividers
- Implement MARIA Kangaroo transparency mode
- Correct Jupiter Ace expansion map

## [0.8.0] - 2026-08-30


### Added

- VIC-20 programs now load through the standard media slot, selecting the
  canonical 3 KiB, unexpanded, or 8 KiB RAM configuration from the PRG load
  address before starting BASIC.
- VIC-20 generic CRT and raw BLK5 cartridges now map through the standard
  cartridge slot, persist across resets and snapshots, and cold-start through
  the real KERNAL probe. Static multi-block CRT images are supported; hardware
  with bank-switching I/O remains explicitly rejected.

### Fixed

- PET 80-column models now clock the CRTC at its real 2 MHz character rate.
- The 6845 cursor now follows its programmed raster range and blink mode.
- Acorn Electron sound now uses the ULA's correct pitch divider and source mux.
- VIC-20 display fetches now honour the programmable screen, colour-RAM, and
  character-memory bases across the VIC-I's 14-bit bus; the existing
  register-driven geometry and screen origin therefore work with relocated
  displays as well as the KERNAL defaults.

## [0.7.1] - 2026-08-29


### Fixed

- *(sm83)* Make ISA disassembly upgrades reviewable
- *(pokey)* Correct distortion polynomial gates
- Expose the VIC-20 live raster register
- Route SVI PSG audio to the host
- Correct the Oric AY VIA decode

## [0.7.0] - 2026-08-27


### Added

- *(atari-5200)* Load headered dumps and Bounty Bob's bank switching
- *(spectrum)* Trace I/O ports on the Spectrum and its clones
- *(spectrum)* --frames, --screenshot and --audio-capture
- *(bbc,electron)* Type the shifted legends, so `*` and `"` work
- *(bbc,electron,dragon)* Let a script start and stop the tape
- *(keyboard)* Type the shifted legends on the Dragon, VIC-20, MSX and SVI-328
- *(einstein)* Type the shifted legends
- *(pet)* Let the PET type a subtraction
- *(mtx)* Let the MTX type its shifted legends
- *(aquarius)* Let the Aquarius type its shifted legends, and fix its boot test
- *(zx80, zx81)* Let both Sinclair machines type their shifted legends
- *(spectrum)* Let the Spectrum type its Symbol Shift legends
- Publish six chip cores as emu198x-* crates


### Fixed

- *(atari)* Pulse ANTIC's NMI so a DLI stops eating the VBI
- *(atari)* Report the television standard from GTIA's PAL register
- *(shell)* Name the argument an MCP tool does not take
- *(shell)* MCP mode loads the media named on the command line
- *(amiga-adf)* Say which container it is, not that the size is wrong
- *(nes)* One scripted frame is one recorded frame
- *(dragon)* Play the tape the file holds, not a reconstruction of it
- *(dragon)* Stop the tape smoke reporting success on a scan it declined
- *(electron)* Sample the IRQ line every cycle, not once a scanline
- *(shell)* Refuse a character the machine cannot type, do not count it
- *(bbc)* Decode the video ULA the way the hardware does
- *(shell)* Place the layout helper before the test module
- *(bbc,electron,oric)* Step instructions through the same hardware as a frame
- *(pet)* Give the PET the rest of its keyboard, and stop cursor-right typing ]
- *(oric)* Let the Oric type its shifted legends, and tell # from backslash
- *(ace)* Stop the Jupiter Ace typing the wrong letters
- *(zx81)* Stop dropping every keystroke after the first
- *(m5)* Name the M5's keycaps after what they type, and reach the shifted ones
- *(atom)* Stop dropping every second keystroke
- *(spectrum)* Make the Super HALT Invaders test reach a HALT
- *(build)* Record the test-skip dev-dependency in Cargo.lock
- *(build)* Give the Einstein dev-dependency a version
- *(build)* Give the last four path dependencies a version
- *(status)* Identify shipping machines by their binary, not by prefix
- *(docs)* Make relative doc references resolve, and keep them resolving
- *(docs)* Answer "does this link resolve" from git, not from the disk it ran on
- *(6845)* Wrap the horizontal counter instead of overflowing it


### Chore

- **Breaking** — *(atari-5200)* Drop the PAL profile — the 5200 shipped NTSC only. The `atari-5200-pal` machine profile is removed, and `--region pal` is rejected. `--region ntsc` keeps working.

## [0.6.0] - 2026-08-25


### Added

- *(amiga)* Decode the extended ROM window, and boot AROS with it
- Generate the ZX80 picture with the CPU, not a borrowed ULA
- Give the ZX80's synthetic firmware a display routine of its own
- Offer the ZX80's 16 KB RAM pack as a profile
- Parse ZX80 .o images and wire the cassette line to the bus
- Load ZX80 tapes through the ROM's own cassette loader
- Free-run the ZX80's line clock and lock it to the sync
- *(zx80)* Load a cassette from the headless runner
- *(ui)* Derive pixel aspect from the raster instead of the crop
- *(shell)* Name the active line counts, and check them against the VIC-II
- *(ui)* State what the display was, not just how fast the pixels came
- *(shell)* Put the display on the machine so the audit can see it
- *(game-boy)* A synthetic cartridge carrying the Emu198x plate
- *(nes)* A synthetic cartridge, and the plate carries a real colour
- *(atari-5200)* A synthetic cartridge, and a BIOS that hands over to it
- *(atari-7800)* A synthetic cartridge, and one zone per scanline
- *(atari-2600)* A synthetic cartridge drawn by chasing the beam
- *(shell)* Put the framebuffer extent on the query surface
- *(dragon)* Take the shared --script session surface
- *(zx81)* Generate the display from the bus, not from the display file
- *(zx80)* Put "was a picture generated" on the query surface
- *(zx81)* Add the 60 Hz board as a selectable variant
- *(zx81)* Parse .p/.p81 tape images
- *(zx81)* Load .p images off the cassette line
- *(zx81)* Fit each board its own RAM, and make the TS1000 real
- *(tape)* Put tape position on the query surface
- *(zx81)* WRX hi-res, and fix the tape test that SLOW mode invalidated
- *(zx80)* Give the board a television-standard strap
- *(zx81)* Give the machine its voice, which is the display
- *(sega-vdp)* Magnify sprites when R1 bit 0 is set
- *(sega-vdp)* Apply the 315-5124's address-bus mask bits
- *(sega-vdp)* The 315-5124's magnification quirk, and Mode 4's status fill
- *(sega-master-system)* Ship the early machine as its own model
- *(sega-vdp)* Mode 4's 224 and 240-line displays
- *(sega-vdp)* Give the H counter a value, and port $3F a way to latch it
- *(sms)* The Light Phaser
- *(shell)* Absolute aim input, and the Light Phaser on a mouse
- *(shell)* Say so when a run paints nothing


### Fixed

- Measure the ZX80 beam position instead of a zeroed clock
- Report ZX80 paint geometry per frame, not since boot
- Record every frame a machine emits, and don't mux silence away
- *(zx80)* Let a cassette sit in the deck without playing
- *(zx80, zx81)* Stop showing PAL pixels square
- *(ui)* Re-derive pixel aspect when the machine changes under it
- *(spectrum)* Stop showing every variant's pixels square
- *(shell)* Calibrate the NTSC active line against four published ratios
- *(tms9918)* Stop showing seven machines' pixels square
- *(sms, nes, c64, 2600)* Derive pixel aspect; state it for the handhelds
- *(atari, vdg, vic-20)* Derive pixel aspect from the chip clocks
- *(bbc, electron, cpc, oric, ace, aquarius)* Derive pixel aspect
- *(amiga)* Derive pixel aspect; leave the PET where it belongs
- *(zx80, zx81)* Show the 288 lines a set shows, not 240
- *(amiga)* State the display through the variant enum
- *(dragon)* State the rate the overscan framebuffer actually fills at
- *(atari)* Size the framebuffer to the region's field, not to PAL's
- *(vdp)* Give every PAL machine the 288 lines its set shows
- *(jupiter-ace)* Take the 288 lines a PAL set shows
- *(acorn-atom)* Hold the 288 lines a PAL set shows, not the VDG's 243
- *(2600, cpc)* Take the whole field; classify the BBC and Electron
- *(vic-20)* Four pixels a cycle, not eight
- *(aquarius, oric)* Draw the border one has, classify the one the other blanks
- *(aquarius)* Draw the 25 rows the hardware scans, not 24 from the wrong end
- *(video)* Derive the horizontal border too, and classify every core
- *(ui)* Open windows at the size the machine will report
- *(sega-vdp)* Clock the PAL VDP from the Master System's own master
- *(video)* Place each picture where its chip scans it, not in the middle
- *(gtia)* Composite the window, not the normal playfield
- *(vic-i)* Draw the display where the registers put it
- *(sms)* Clock a PAL Master System's Z80 from its own master
- *(vic-i)* Draw multicolour cells instead of colouring them wrong
- *(einstein)* The ROM window is the lower 32K, not the low 8K
- *(c64)* Preserve far-edge VIC-II C-data state
- *(z80)* Collapse held Spectrum I/O strobes
- *(z80)* Run one native frame per request
- *(amiga)* Pace UI at one field per frame
- *(amiga)* Freeze same-CCK DMA ownership
- *(amiga)* Keep NTSC floppy at 300 RPM
- *(z80)* Pin the MTX and Einstein frame budget to the real frame
- *(shell)* Preserve exact native frame counts
- *(jupiter-ace)* Run 208 T-states per line, not 207
- *(zx81)* Report the television strap on port bit 6
- *(zx81)* Budget the shortest frame, not the field backstop
- *(zx81)* Report the active line count for the board's region
- *(zx81)* Form the pattern address as the ULA does
- *(zx80, zx81)* Place the picture from the ROM's pad, not a nominal frame
- *(zx81, z80)* Let the machine reach SLOW mode at all
- *(zx81)* Size the window from the board strap, not a constant
- *(tms9918)* The fifth-sprite flag needs the frame flag clear
- *(tms9918)* Coincidence covers sprites off the edges of the screen
- *(sega-vdp)* Honour both of R0's scroll and column-mask bits
- *(sega-vdp)* Latch the vertical scroll once a frame
- *(sega-vdp)* The line counter is checked on the first line of vblank
- *(sega-vdp)* Drive CRAM through the chip's measured output levels
- *(sega-vdp)* A sprite Y near the top of the range hangs off the screen
- *(sms)* The light gun sees the border, not just the picture
- *(atari-5200)* Pick the 16 KB cartridge layout from evidence, not size

## [0.5.0] - 2026-08-19


### Added

- *(status)* Add the system registry that joins the repo's four vocabularies
- *(status)* Collect per-machine test evidence from what actually ran
- *(sega)* Ship the Game Gear from its own crate
- *(status)* Make evidence collection the test job, not a second pass
- *(status)* Restore the status canon as generated pages checked for drift
- *(sega)* Prove the Master System and Game Gear boot, without a commercial ROM
- Prove the remaining five firmware-free machines boot
- *(spectrum)* Evidence the Spectrum's boot on every pull request
- *(spectrum)* Evidence the whole Sinclair/Amstrad line's boot in CI
- Prove the six TMS9918 machines start, without their firmware
- *(msx)* Boot real firmware, using a BIOS nobody needs permission for
- *(c64)* Boot real firmware, using ROMs nobody needs permission for
- *(800xl)* Boot real firmware, using an OS nobody needs permission for
- Prove the VIC-20, Atari 5200 and Amiga start, without their firmware
- Prove the Acorn Atom and Oric Atmos start, without their firmware
- Prove the Aquarius and Jupiter Ace start, without their firmware
- Prove the CPC and BBC Micro start, without their firmware
- Prove the Dragon 32 starts, without its firmware
- Prove the Electron and PET start, without their firmware
- Prove the ZX80 and ZX81 start, closing the set at 30 of 30


### Fixed

- *(sega-vdp)* Show the Game Gear's LCD, not the Master System's television
- *(spectrum)* Resolve the provisioned ROM, and stop the goldens passing on nothing
- Record machine-msx's test-skip dev-dep in the lockfile
- Record the 86 skips that were still passing in silence
- Catch the guard forms the checker walked past
- *(zx8x)* Take character bitmaps from I, not from ROM $0000
- Record the ZX80/ZX81 test-skip dev-deps in the lockfile


### Refactor

- **Breaking** — Retire support_tier. `MachineProfile::support_tier` and the `SupportTier` enum are removed, and the `session.profile.support_tier` query path no longer resolves. Scripts and MCP clients reading that path will need to drop it; nothing replaces it, because a tier nobody maintained is not information worth preserving. A tier could earn its way back with a stated meaning per rung, a mechanism that moves it, and a consumer that reads it — it had none of the three.

## [0.4.0] - 2026-08-18


### Added

- Cross-check the Debug198x banked model against real 128K paging
- Make an unreadable space shape reportable, not just survivable


### Fixed

- Decline an ambiguous source file rather than guessing which one

## [0.3.0] - 2026-08-18


### Added

- *(cpc)* Load games from tape
- *(cpc)* Add the CPC runtime, so the machine can be driven
- *(cpc)* Add the CPC frontend, so the machine is runnable
- *(nes)* Expose OAM and per-scanline sprite counts, so dropout is measurable
- *(spectrum)* Autoload tape on the 128K family, not just the 48K
- *(cpc)* Read the screen back as text
- *(cpc)* Add a 6128 model with the PAL's banked RAM
- *(cpc)* Model the Gate Array's /WAIT stretching
- Read Debug198x sidecars for symbolised debugging


### Fixed

- *(denise)* Repair the dual-playfield priority test, which never armed a playfield
- Make the Atari 800XL MCP tests run and give memory_read its advertised default
- *(spectrum)* Let a caller pin which ROMs the Spectrum boots
- *(nes)* Stop the debug PPU trapping when sprite size changes mid-line
- *(dragon)* Make a missing golden fail instead of passing quietly
- *(video)* Encode captures at a constant quantiser, not CRF
- *(audio)* Emit whole frames, so a quiet machine still has an audio track
- *(cpc)* Fit the 464's HD6845S, which reads its start address back
- *(z80)* Restore WZ = PC + 1 on the INIR/INDR repeat path
- *(spectrum)* Move the 48K floating-bus read origin to 14335
- **Breaking** — Widen the I/O trace port to the full 16-bit address bus. `emu198x_shell::IoEvent::port` is now `u16` rather than `u8`. Every consumer is in this workspace; machines with their own `u8` event type need no change, as the conversion widens.
- Correct the Debug198x banked-paging model


### Performance

- *(debug)* Stop re-rendering the framebuffer once per stepped instruction

## [0.2.3] - 2026-08-15


### Added

- *(cpc)* Generate interrupts in the Gate Array from the CRTC's HSync
- *(cpc)* Boot the CPC464 firmware to its own blue-and-yellow screen
- *(cpc)* Render the display at the dot clock
- *(cpc)* Let the CPC be typed at


### Fixed

- *(6845)* Start VSync at the beginning of row R7, not its end
- *(bbc)* Point the tape test at the UEF that was there all along
- *(uef)* Default the tape test to the UEF we already vendor
- *(cpc)* Report VSync on PPI port B, where programs look for it
- *(c64)* Stop type_string dropping characters, and wire load_basic_program

## [0.2.2] - 2026-08-14


### Added

- *(spectrum)* Read .szx snapshots
- *(spectrum)* Expose the CPU on the query surface
- *(cpc)* Add the Amstrad Gate Array's video modes and palette
- *(release)* Generate the changelog from commits, not from packages


### Fixed

- *(z80)* Hold the M1 opcode strobes to the rising edge of T3
- *(z80)* Make the M1 refresh strobe a full clock wide
- *(z80)* Hold /RFSH to the start of the next machine cycle
- *(z80)* Hold the memory read strobes to the end of T3
- *(z80)* Present each M-cycle's address on its own T1 rise
- *(z80)* Hold the memory write strobes to the end of T3
- *(z80)* Hold the I/O strobes from T2 fall to the end of the cycle
- *(z80)* Give the not-taken displacement cycle a read's pins
- *(z80)* Stop driving IR during internal cycles
- *(ula)* Arm the contention gate on the edge that drops /MREQ
- *(spectrum)* Derive the floating-bus sample instant from the I/O M-cycle
- *(ula)* Phase-lock the contention window to the ULA's fetch group
- *(ula)* Open the contention window at the fetch cycle, not the fetch
- *(shell)* Refuse a snapshot extension we do not write, and wait for the BASIC prompt
- *(spectrum)* Charge +2A contention from a measured mask
- *(spectrum)* Charge each port class the lookups FUSE charges it
- *(z80)* Sample /INT at the instruction boundary, not a half-cycle early
- *(sega)* Tick the Z80 twice per T-state, and feed /INT before the tick
- *(msx,coleco,svi)* Tick the Z80 twice per T-state, and feed /INT before it
- *(sord-m5,mtx)* Tick the Z80 twice per T-state on the CTC-vectored machines
- *(einstein)* Tick the Z80 twice per T-state, making the 4 MHz claim true
- *(zx80,zx81)* Tick the Z80 twice per T-state against a T-state ULA
- *(aquarius)* Tick the Z80 twice per T-state
- *(release)* Let every crate's work reach the suite changelog
- *(release)* Process every crate so its commits can reach the changelog
- *(release)* Write the suite changelog from the workspace, not from one machine

The per-system binaries stay at 0.x for now; library crates published to
crates.io may hit their own 1.0 on their own schedules.

## [Unreleased]

## [0.2.1](https://github.com/emu198x/emu198x/compare/v0.2.0...v0.2.1) - 2026-08-11

### Added

- *(spectrum)* boot any variant headlessly with --machine

### Other

- make the release able to ship binaries again
- Correct Amiga DMA ownership and media persistence
- inherit the Emu198x suite version
- declare the last two silent guards, both in src test modules
- give every fixture guard a voice, across the workspace
- Ship higher-CPU Amiga profiles

## [0.1.0] — 2026-05-23

Initial public release. Six per-system native verifier shells, each shipping
as its own binary for macOS (arm64 + x86_64), Linux x86_64, and Windows
x86_64.

### What works

- **Sinclair ZX Spectrum 48K** — real Z80 + ULA-driven machine loop;
  TAP/TZX loading with autoload and cycle-faithful tape turbo; live beeper +
  tape audio; real-software regressions including Manic Miner and Jet Set
  Willy. Other Spectrum variants (16K, 128K, +2, +2A/B, +3) exist as crates
  and are in active work.
- **Commodore 64** — live 6502 / CIA / VIC-II / SID board loop; KERNAL
  boots to `READY.`; TAP-backed datasette with autoload; host-side `.prg`,
  `.bas`, `.d64`, `.t64` import paths; optional live 1541 drive-8 with real
  `D64` media insertion (read-only; write path is post-launch).
- **Nintendo Entertainment System** — live 2A03 / 2C02 / APU machine loop;
  iNES cartridge loading with 14 mappers (NROM, MMC1, UxROM, CNROM, MMC3,
  MMC5, AxROM, Color Dreams, VRC2a, Action 53, BxROM, NINA-001, Sunsoft-4,
  Camerica); `nestest` passes 8991/8991.
- **Commodore Amiga A500 OCS PAL** — live board loop over `motorola-68000`,
  Agnus, Denise, Paula, Gary, dual 8520 CIAs, keyboard, DF0 floppy; live
  Paula audio; Kickstart 1.3 boots; Workbench 1.3 and 2.04 (ECS A500+)
  desktop. A1200 / AGA work is mid-flight and not yet shipping.
- **Nintendo Game Boy** — live DMG-family CPU / PPU / APU machine loop;
  `raw` / `lcd` / `crt` video presenter modes; headless cartridge runner with
  `.sav` battery-RAM sidecars; Blargg and mooneye-style verification gates.
- **Dragon 32** — real BASIC ROM boot over `motorola-6809`, dual MC6821
  PIAs, MC6883 SAM, MC6847 VDG; CAS media, ROM / DGN cartridges, DragonDOS
  VDK disks (read-only), PC-Dragon PAK snapshots; 11/12 application smoke
  matches against patched XRoar reference frames.

### Verification

- Z80 — 100% Tom Harte, ZEXDOC, ZEXALL pass
- 6502 — 100% Tom Harte
- 68000 — 100% Tom Harte (1,000,058 vectors)
- 627/629 NES ROMs survive 300 frames in the local-archive smoke matrix
- Per-system 10-title catalogue infrastructure (`emu198x-catalogue`) covers
  Spectrum, C64, NES, Amiga via TOML manifests

### Modes

Each per-system binary supports three modes:

- `--ui` (default) — native interactive shell with `wgpu` video, `cpal`
  audio, `gilrs` gamepad input, `winit` windowing
- `--script` — headless JSON-driven runner for screenshots, snapshots,
  capture, regression
- `--mcp` — JSON-RPC 2.0 MCP server over stdio, for Claude Code / other
  MCP hosts

### Not in this release

Stated honestly upfront so nothing surprises:

- Spectrum variants beyond 48K are work-in-progress
- Amiga A600 / A1200 / A3000 / A4000 / CDTV / CD32 (AGA chipset is mid-flight)
- Game Boy Color, Super Game Boy, link cable
- Dragon 64, CoCo line, DragonDOS write path, OS-9
- NES Famicom Disk System, Zapper, Game Genie, mapper coverage past 14
- C64 cartridge (CRT) support, REU, mouse / paddles, 1541 write path, C128
- Pentagon / Scorpion / Timex Spectrum variants (crates exist, deferred)
- Any system not in the six above (Atari 2600, BBC Micro, MSX, Master
  System, etc. — these are Wave 2+ per the roadmap)

### Documentation

- [README](README.md) — what the project is, how to build, how to obtain ROMs
  legally, per-system runner examples
- public docs site — system status, MCP integration, capture, scripting, and
  accuracy progress
- [`CONTRIBUTING.md`](CONTRIBUTING.md), [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md),
  [`SECURITY.md`](SECURITY.md)

### Notes

- ROMs are not bundled. The README's "Getting ROMs" section covers each
  platform's legal acquisition path (Cloanto Amiga Forever, Cloanto C64
  Forever, World of Spectrum's Sinclair-permitted set, etc.).
- License is GPL-2.0-or-later workspace-wide.
- Project lives in the 198x family alongside Code Like It's 198x.

[Unreleased]: https://github.com/emu198x/emu198x/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/emu198x/emu198x/releases/tag/v0.1.0
