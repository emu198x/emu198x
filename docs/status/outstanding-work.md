# Outstanding Work — Cross-System Rollup

Status as of 2026-06-01. Companion to
[`current-system-usability.md`](current-system-usability.md). Each section is
the live list of open items per machine, ordered roughly by user impact
within that machine. Items are tagged:

- **L** — relevant to the October Spectrum launch
- **A** — accuracy / correctness debt that doesn't block usability
- **S** — scope expansion (broader software / new machines / new hardware
  paths)

Resolved items are kept here briefly only when they unblock something else
listed below.

## ZX Spectrum — `emu198x-spectrum`

CPU surface in genuinely good shape: Tom Harte 100%, ZEXDOC/ZEXALL all
checkpoints, FUSE 1,351/1,356 with 5 documented disagreements, Patrik Rak
`z80test` 6/6 with zero allowlist. 262/262 runtime tests pass. 11 variants
boot to a working screen.

- **L — Strict PNG comparison for the 5 ULA / contention smokes against
  Spectron references.** The smokes currently compare against self-locked
  goldens; spec'd target is byte-equal against Spectron's
  `tests/Results/<name>_{48,128}.png`. Spectron renders 1224×968 with
  border + scaling, so the comparator needs a downscale-and-crop step
  before equality. See
  [`knowledge/tests/spectrum.md`](../../knowledge/tests/spectrum.md)
  § Outstanding launch-blockers.
- **A — 4 residual FUSE block-I/O AF disagreements** on `INIR`, `OTIR`,
  `CPDR`, `OTDR` (X/Y undocumented flag bits at the final repeat
  iteration). WZ matches, T-states match, memory effects match; just the
  undoc bits. Resolution needs silicon-level evidence; not a launch
  blocker.
- **S — Scorpion ZS-256 screen rendering.** CPU-liveness boot test
  passes; the Service ROM doesn't paint standard screen RAM yet. Three
  concrete bugs in `machine-scorpion-zs256/src/memory.rs` identified
  against FUSE's `machines/scorpion.c`: page-select bit (`$1FFD` bit 0
  vs bit 4), ROM-select logic, and ROM 3 should be Beta Disk overlay not
  bank-selectable. The fixes interact and need to land together —
  research recorded for a future one-session attempt.

## Commodore 64 — `emu198x-c64`

Headless boot to `READY.` verified live 2026-06-01; disk autoload walks an
Impossible Mission D64 end-to-end through the IEC bus and 1541. CPU
oracles: Tom Harte 100%, Dormann functional pass, Lorenz 250/265. 71/71
active runtime tests pass.

- **S — Real-software autoload tests are gated on archive paths.** 8
  D64 autoload tests + 5 TAP autoload tests (Impossible Mission,
  Ghostbusters, Thomas the Tank Engine, Thing on a Spring, Thinker) sit
  `ignored` waiting for the local archive root to land. Tests are
  written; wire them once the archive path is settled.
- **S — `--autoload-disk` only types `LOAD"*",8,1`.** For non-autostart
  binaries the load completes and drops back to `READY.` Adding an
  `--autoload-run` (or an `8,1` → `RUN` extension) would smooth game
  launches to one command.
- **A — 15 Lorenz `cpu` skips need full C64 machine model** (CIA timer
  interaction, 6510 banking, KERNAL tape traps, IRQ delivery,
  cycle-observable `cputiming`, `finish` screen-clear). The 6510 zero-page
  port `cpuport` already flipped to PASS once the three pin classes were
  modelled.
- **A — Drive/tape workflows are flag-heavy.** Discoverability gap, not
  correctness gap. Could be folded into a single `--smart-autoload` that
  picks disk vs tape vs PRG by file extension.

## Nintendo NES — `emu198x-nes`

Tom Harte 100% (with one allowlisted opcode — `$AB` LXA/ATX uses Mesen's
stable model per the NES test-oracle decision); nestest 8991/8991;
155-ROM sweep at **135 PASS / 5 FAIL / 0 TIMEOUT / 15 VISUAL**. APU
length-counter timing and LXA both closed 2026-06-01.

- **A — `blargg_nes_cpu_test5` test 01-implied (2 ROMs).** Both `cpu.nes`
  and `official.nes` now fail uniquely on test 01 after the LXA fix.
  Probe at
  [`crates/machine-nintendo-nes/tests/cpu_test5_probe.rs`](../../crates/machine-nintendo-nes/tests/cpu_test5_probe.rs)
  confirms sub-tests 02-11 all carry `[OK]` markers. Test 01 covers 22
  implied-mode opcodes (ROL/ASL/ROR/LSR A, T(A/X/Y), IN/DE X/Y, the
  seven flag set/clear ops, NOP). A Rust port of blargg's CRC-32
  framework lives at
  [`crates/mos-6502/tests/blargg_01_implied_crc.rs`](../../crates/mos-6502/tests/blargg_01_implied_crc.rs);
  2/20 OFFICIAL_ONLY opcodes match (TXA, TYA), confirming the
  CRC + iteration order are correct. The remaining 18 likely diverge in
  `set_paxyso`'s PLP behaviour or first-iteration CPU state.
- **A — OAMDMA + DMC DMA cycle accounting** (`sprdma_and_dmc_dma` 0/2).
  OAMDMA is fixed 514 cycles in the machine layer; DMC sample DMA steals
  individual CPU cycles but doesn't interleave with an in-progress
  OAMDMA. Need: 513/514 by even/odd alignment + DMC interleave.
- **A — `cpu_timing_test6` protocol** (0/1). Settles at
  `$00F0 = 0x98`; protocol not understood (the `0x98` byte is the TYA
  opcode, which may be a hint but is not confirmed).
- **S — More mapper coverage.** Memory mapping, expansion audio, and
  scanline IRQ are wired for MMC5; broader mapper coverage and
  hardware-test cross-checking remain useful.

## Commodore Amiga — `emu198x-amiga`

Full `--model` matrix reachable from script mode as of `bc23bc8`
(2026-06-01): A1000 / A500 / A500+A501 / A500-Plus / A500-Maxed / A600 /
A1200 / A2000. A1200 + Kickstart 3.1 boots clean through Insert-Workbench
to a clean Workbench 3.1 desktop — no palette or geometry artefacts. CPU
oracles: 68000 100% Tom Harte (1M tests); 68010/68020 100% against Musashi
via `m68k-test-gen`.

- **A — Promote AGA Workbench 3.1 boot to an automated screenshot smoke.**
  The boot was verified manually this session (`--model a1200
  --kickstart kick31a1200.rom --disk workbench31.adf --frames 1800
  --screenshot aga_wb.png`). Locking a golden would catch regressions in
  the FMODE bitplane wide-fetch (`d31e46a`) and 68020 full-format EA
  decode (`369d50b`) paths.
- **A — Gayle for A600 / A1200.** Current Gayle wiring covers what
  Kickstart 3.1 needs to boot. IDE and PCMCIA paths are stub-level;
  broader software (e.g. an A1200 with a hard drive image) will need
  them properly modelled.
- **S — Broader software validation across OCS / ECS / AGA.** Workbench
  1.3 / 2.x / 3.1 desktops verified, but game/application coverage is
  thin. Pick representative titles per chipset and wire as headless
  smokes with screenshot artefacts.
- **S — Long-term scope (recorded, not active).** Apollo Vampire FPGA +
  AC68080, PiStorm, RTG framebuffer expansions — the trait surface was
  designed to accommodate non-Commodore CPUs / chipsets / dual-display
  from day one, but no implementation work is scheduled.

## Nintendo Game Boy — `emu198x-game-boy`

CPU oracle: 49,600 Adam Tennant SM83 single-step tests pass + 92 lib unit
tests. DMG-family verifier window works with `wgpu` `raw`/`lcd`/`crt`,
keyboard/gamepad joypad, scripts, snapshots, `.sav` battery-RAM sidecars.

- **A — Tune `lcd` filter against hardware references.** The LCD preset
  is wired but not calibrated against side-by-side photos. Game Boy is
  the obvious case for taking the LCD preset seriously.
- **S — Broader real-game smoke coverage.** Boot through known-good
  titles and lock screenshots so regressions get caught.

## Dragon 32 — `emu198x-dragon` (not in October launch)

Native verifier window, real Dragon 32 BASIC ROM boot, mono audio pinned
to XRoar's level model, CAS / DragonDOS `.BIN` / PAK / VDK media paths,
PAK snapshot smokes, optional patched-XRoar screenshot comparisons.

- **A — DragonDOS VDK exact controller timing/status/write.** Initial
  P2 controller reads work; exact timing and write paths need filling
  in from observed real-software failures.
- **S — Real DragonDOS ROM + VDK software smokes** at the same bar as
  the CAS / PAK paths.

## Oric-1 / Atmos — `emu198x-oric-atmos` (new, 2026-06-01)

Tenth donor-codebase extraction. **Zero new chip crates** — reuses
`mos-6502` ✓, `mos-via-6522` (we already had a more complete impl
than the donor — 898 vs 839 lines, plus serde + detailed SR mode
types), and `gi-ay-3-8912` ✓. The Oric's custom video ULA ports
inline into the machine layer like the Electron's.

Fresh-write machine layer with the distinctive **AY-via-VIA wiring**
(VIA port A = AY data bus, CA2 = AY BDIR, CB2 = AY BC1; software
puts PCR into one of four `(BDIR, BC1)` modes per AY operation),
8×8 keyboard via VIA port B column select + port A row read,
TEXT and HIRES display modes with serial-attribute rendering, full
BBC-Micro-compatible 8-colour 3-bit RGB palette.

Strong cultural anchor in **France** — Loriciels and ESAT made the
Atmos a de-facto French home computer in the mid-1980s.

- **L — BIOS not yet available.** The 16 KB BASIC + OS ROM is not
  in the TOSEC dump (Tangerine/Oric-1 & Oric Atmos/ exists as
  empty directories pending copy). Gated smoke at
  `crates/machine-oric-atmos/tests/bios_boot.rs` waits for
  `atmos.rom` (or `oric1.rom`) at
  `~/.emu198x/roms/oric-atmos/`. Defence-Force preservation
  archive (`defence-force.org`) is the canonical source.
- **A — Atmos RAM-under-ROM not fully wired.** 64 KB is allocated
  and writes go to RAM even at ROM addresses, but ROM still wins
  on reads (matching standard reset state). Bank-switching to
  expose the RAM at `$C000-$FFFF` for advanced software is not
  modelled.
- **A — TAP cassette loader.** Donor has Oric `.tap` parser
  ($16 sync / $24 marker / type + autorun + end + start + name
  + payload); not yet wired into our binary.
- **A — Display rendering** runs end-of-frame; mid-frame palette
  changes via serial attributes work within a line but not across
  scanlines mid-render.
- **A — Snapshot deferred** (shared family pattern).
- **S — Full shell parity**.

## Atari 2600 — `emu198x-atari-2600` (new, 2026-06-01)

Twelfth donor-codebase extraction. **Two new chip crates** —
`mos-riot-6532` (477 LoC, 11/11 tests) and `atari-tia` (1204 LoC +
inline NTSC/PAL palette module, 13/13 tests). Reuses our `mos-6502`
(running as the 6507 — the pin-limited 13-address-line 6502).

Fresh-write machine layer wiring the 6507 to TIA + RIOT through
the 2600's distinctive **A12/A7/A9 address decode**: A12 = cart at
`$1000-$1FFF`; A12 = 0, A7 = 0 = TIA registers; A12 = 0, A7 = 1 =
RIOT (A9 picks RAM vs I/O+timer). Master clock = TIA colour clock
at 3.58 MHz NTSC / 3.55 MHz PAL; 6507 and RIOT tick every 3rd
colour clock; TIA WSYNC line halts the CPU until next hblank.

Cartridge bank-switching support: 2 KB / 4 KB (no banking), F8
(8 KB / 2 banks at `$1FF8/$1FF9`), F6 (16 KB / 4 banks at
`$1FF6-$1FF9`), F4 (32 KB / 8 banks at `$1FF4-$1FFB`). Reads or
writes to the hotspot addresses trigger bank switches.

**Live boot verified 2026-06-01** with the 1977 Atari Combat
cart (2 KB, NTSC). Renders the canonical olive-on-peach
two-tank playfield: red tank left, blue tank right, missile
indicator at top, peach borders. Gated smoke at
`crates/machine-atari-2600/tests/cart_boot.rs` (picks first
`.a26` / `.bin` from `~/.emu198x/media/atari-2600/`) passes (1/1).

Bug fixed during boot bring-up: the donor's `mos-riot-6532`
reset the timer prescaler + cleared `post_underflow` state on
**every read of INTIM** (`$0284`). Real 6532 silicon doesn't
touch the prescaler from a read; the timer free-runs
independently. Combat (and many 2600 games) polls INTIM in a
tight loop expecting the prescaler to keep decrementing — with
the donor's behaviour the prescaler was reset every read and the
game wait-loop never exited. Fix: reading INTIM clears only the
underflow flag; prescaler and post-underflow state are
free-running.

- **A — TIA cycle-perfect timing.** The TIA is famously hard.
  Pixel-level rendering works for normal cart code paths, but
  HMOVE quirks, RESP starfield edge cases, and audio mixing
  refinements are in the accuracy backlog. Bigger games (Pitfall,
  Adventure, Star Raiders) may surface issues this initial port
  doesn't catch.
- **A — Audio output unwired.** TIA AUDx registers latch but
  the binary doesn't drain or write a WAV.
- **A — Joystick / console-switch input** exposed on the
  machine via `set_joystick_input(byte)` and `set_switch_input
  (byte)` but the binary doesn't have a runtime interactive
  surface yet.
- **A — Snapshot deferred** (shared family pattern).
- **S — Full shell parity**.

## Atari 5200 SuperSystem — `emu198x-atari-5200` (new, 2026-06-01)

Thirteenth donor-codebase extraction. **Three new chip crates** —
`atari-antic` (14/14 tests, display-list processor + DMA
controller), `atari-gtia` (9/9 tests, video output + player /
missile graphics + collision), `atari-pokey` (13/13 tests,
4-channel audio + paddle pot scanner + serial I/O). Same ANTIC
+ GTIA + POKEY chip family the 800XL / 130XE share — so this
extraction is the foundation for the rest of the 8-bit Atari
home-computer line.

Fresh-write machine layer (`machine-atari-5200`, 14/14 tests)
wiring the 6502 "Sally" to ANTIC + GTIA + POKEY through the
5200 memory map: 16 KB RAM at `$0000-$3FFF`, cart at
`$4000-$BFFF` (size-mirrored 4 KB / 8 KB / 16 KB / 32 KB), GTIA
`$C000-$CFFF`, ANTIC `$D400-$D5FF`, POKEY `$E800-$E9FF`, 2 KB
BIOS at `$F800-$FFFF` (cart's `$FFFC/$FFFD` mirror falls
through when BIOS is absent). Master clock = colour clock;
CPU + POKEY tick every 2nd colour clock = 1.79 MHz NTSC; ANTIC
processes one scan line at every 228-clock boundary and stalls
the CPU for its DMA budget at the start of each line.

Joystick wired through POKEY pots (0-228 each, 114 = centre)
via `set_joystick(x, y)`; fire button wired through GTIA TRIG0
via `set_fire(pressed)`.

**Live boot verified 2026-06-01** with the 1982 Atari Pac-Man
cart (16 KB, NTSC) + 5200 BIOS. Gated smoke at
`crates/machine-atari-5200/tests/cart_boot.rs` (picks first
`.a52` / `.bin` / `.car` from `~/.emu198x/media/atari-5200/`)
passes (1/1). Cart drives real ANTIC scan-line output —
captured screenshot shows partial title-screen pixels
(scoreboard fragments, dot field).

- **A — Partial render fidelity.** Pac-Man title boots but
  most of the title-screen sprites are missing. ANTIC scan-line
  indexing into GTIA's framebuffer and/or DMA budget are off;
  pin further once more carts are exercised. The pipeline is
  wired end-to-end — this is correctness, not structure.
- **A — Cycle-accurate WSYNC + DMA stealing.** Current model
  treats the DMA budget as a fixed CPU-cycle stall at the start
  of the line; real ANTIC interleaves DMA cycles through the
  scanline.
- **A — Audio output unwired.** POKEY buffer drained via
  `take_audio_buffer()` but the binary doesn't write a WAV.
- **A — Snapshot deferred** (shared family pattern).
- **S — Full shell parity**.
- **S — Atari 800XL.** Reuses ANTIC + GTIA + POKEY (now
  unblocked); adds MOS PIA-6520.

## Atari 7800 ProSystem — `emu198x-atari-7800` (new, 2026-06-01)

Fourteenth donor-codebase extraction. **One new chip crate** —
`atari-maria` (15/15 tests, 1009 LoC), the zone-based display
processor that replaces the 2600's race-the-beam model: MARIA
walks a Display List List (DLL) at every scanline, DMAs sprite /
tile data from RAM, and stalls the 6502C "Sally" for the DMA
budget. Reuses our `mos-6502` as the 6502C "Sally" (stock 6502
with Atari's HALT pin) and `mos-riot-6532` for joystick / console
switches / timer.

Fresh-write machine layer (`machine-atari-7800`, 18/18 tests)
wiring CPU + MARIA + RIOT + a thin TIA-audio register stub through
the 7800 memory map: TIA at `$0000-$001F` (audio only — MARIA
handles video), MARIA at `$0020-$003F`, zero-page RAM
`$0040-$00FF`, stack RAM `$0140-$01FF`, RIOT `$0280-$02FF`, main
RAM 4 KB at `$1800-$27FF` (mirrored to `$3FFF`), cart
`$4000-$FFFF`. Cartridge handling: flat 16 KB / 32 KB / 48 KB
mapping or 8 × 16 KB SuperGame banking with bank 7 fixed at
`$C000` and writes to `$8000-$BFFF` switching the middle window.
A78 header auto-stripped.

Joystick wired via RIOT port A bits 4-7 (active-low) through
`set_joystick(up, down, left, right)`; console switches (Reset /
Select / Pause) via RIOT port B through `set_console(...)`.

Gated cart-boot smoke at
`crates/machine-atari-7800/tests/cart_boot.rs` passes with TOSEC
Asteroids (1987)(Atari)(NTSC).a78 — cart loads, machine ticks
300 frames without panic, framebuffer correctly sized.

- **A — BIOS-driven boot not yet wired.** Native 7800 games
  depend on the 4 KB 7800 BIOS at `$F000-$FFFF` for region +
  encryption checks before transferring control. Without it both
  Asteroids and Dig Dug boot to black. Adding `--bios` and the
  BIOS-overlay toggle (via MARIA CTRL) is the next concrete
  follow-up on this thread.
- **A — TIA audio synthesis.** The 7800 uses TIA only for sound;
  six registers are stored but no synthesis path is wired.
- **A — Snapshot deferred** (shared family pattern).
- **S — Full shell parity**.

## Atari 800XL — `emu198x-atari-800xl` (new, 2026-06-01)

Fifteenth donor-codebase extraction — the last 8-bit Atari in
the donor. **One new chip crate** — `mos-pia-6520` (14/14 tests),
the MOS 6520 / Motorola 6821 Peripheral Interface Adapter used in
many early-1980s machines (here for joystick + console-key input
+ PORTB-controlled ROM banking). Reuses ANTIC, GTIA, POKEY now
that the 5200 has landed them, and our `mos-6502` as the 6502C
"Sally".

Fresh-write machine layer (`machine-atari-800xl`, 15/15 tests)
wiring CPU + ANTIC + GTIA + POKEY + PIA through the 800XL memory
map: 64 KB RAM, ROM overlays via PIA PORTB. Bit 0 = 1 enables OS
ROM at `$C000-$FFFF` (with the `$D000-$D7FF` I/O gap); bit 1 = 0
enables BASIC ROM at `$A000-$BFFF`; bit 7 = 0 enables a self-test
ROM at `$5000-$57FF`. Cartridges in `$A000-$BFFF` shadow BASIC,
16 KB carts cover `$8000-$BFFF`. Without OS ROM, the reset vector
is fetched from the cart entry point (cart-only boot).

I/O area at `$D000-$D7FF`: GTIA `$D000-$D0FF`, POKEY
`$D200-$D2FF`, PIA `$D300-$D3FF`, ANTIC `$D400-$D4FF`. Joystick
wired through PIA PORTA (active-low bits 0-3) via
`set_joystick(up, down, left, right)`; fire button through GTIA
TRIG0; console keys (START / SELECT / OPTION) through GTIA
CONSOL.

Scope of this slice is the **800XL model**: 64 KB RAM with
XL-style PORTB banking. The 400 / 800 (no XL banking) and 130XE
(extended bank-switched RAM) variants are deliberately deferred.

Gated OS-boot smoke at `crates/machine-atari-800xl/tests/os_boot.rs`
passes with the TOSEC `Atari OS Rev 2 (1983)(Atari)[800XL]`
ROM — machine ticks 200 frames without panic, framebuffer is
correctly sized.

- **A — BASIC ROM not bundled.** TOSEC's `8bit/Operating Systems`
  doesn't carry a `ataribas.rom` extract; without BASIC, the OS
  falls into its disk-boot loop and the canonical "READY" prompt
  doesn't appear. Sourcing an 8 KB Atari BASIC Rev C ROM is the
  next step.
- **A — POKEY audio synthesis unwired** in the binary.
- **A — XEX / disk loading not implemented.** Cart-only and
  cart-with-OS for now.
- **A — Snapshot deferred** (shared family pattern).
- **S — 130XE 128 KB extended-RAM banking.** PORTB bits 2-5
  drive the 4 × 16 KB extended banks; not modelled in this slice.
- **S — Atari 400 / 800 variants.** Same chip family, different
  RAM size, no XL banking; would only need a model-selector flag.
- **S — Full shell parity**.

## Acorn BBC Micro Model B — `emu198x-acorn-bbc-micro` (new, 2026-06-01)

Eleventh donor-codebase extraction. **One new chip crate** —
`motorola-6845` CRTC ported alongside (417 LoC, self-contained,
8/8 tests). Reuses our `mos-6502`, `mos-via-6522` (×2 — one
System VIA at `$FE40` for sound + keyboard + IC32 addressable
latch, one User VIA at `$FE60` for Centronics + user port),
`ti-sn76489` PSG, plus an inline Acorn Video ULA.

Fresh-write machine layer with the SHEILA I/O page at
`$FE00-$FEFF`, 16 sideways ROM banks at `$8000-$BFFF` switched
via `$FE30`, 32 KB RAM at `$0000-$7FFF`, 16 KB MOS at
`$C000-$FFFF`, and the BBC-specific **IC32 addressable latch** —
the SN76489 PSG isn't memory-mapped, it's written by putting the
PSG byte on System VIA port A and then **pulse-falling the latch
bit 0** through a port B write encoding `(addr=value&7,
data=value&8)`.

CRTC VSYNC drives System VIA CA1 (sets the line so the VIA edge
detector latches the interrupt). System VIA + User VIA IRQs OR
into the CPU `irq` pin.

**Live-tested with Acorn OS v1.2** (16 KB, SHA-256
`b0ad5c0b2e7d5776cc65d643989c02c66f7823df1f5c1c528833588c5a3e7a07`,
from TOSEC `Acorn/BBC/Operating Systems/`). The MOS executes
through to its sideways-ROM scan and ends up selecting bank 15
(the conventional BASIC slot — same scan logic as the Electron).
Framebuffer stays at MODE 7 backdrop (black) because the OS
defaults to MODE 7 teletext and the **SAA5050 teletext chip is
absent** from this port.

- **L — Acorn BASIC II ROM still missing.** Same blocker as the
  Electron — TOSEC has neither under Acorn/BBC nor anywhere
  searchable. Without BASIC the OS shows the canonical "Language?"
  error in MODE 7. Source from Stairway to Hell.
- **A — SAA5050 teletext chip not modelled.** MODE 7 stays blank.
  The OS uses MODE 7 by default (BASIC then usually moves to
  MODE 0/1 etc); without SAA5050 we can't see the boot screen
  text even with BASIC loaded.
- **A — Keyboard scan via System VIA + IC32 latch + addressable
  output not wired.** Keyboard matrix is allocated; scan path
  through the OS reads the column data which the System VIA
  pulls from IC32-controlled lines. Not yet implemented; affects
  any key-driven boot path.
- **A — CRTC bus contention timing.** Donor and this port both
  run CPU at flat 2 MHz; real BBC has the famous 1 MHz / 2 MHz
  alternating per-cycle scheme.
- **A — Snapshot deferred** (shared family pattern).
- **S — Floppy disk** (Intel 8271 or WD 1770 — different variants).
- **S — Full shell parity**.

## Acorn Electron — `emu198x-acorn-electron` (new, 2026-06-01)

Ninth donor-codebase extraction. **Zero new chip crates** — the
Electron's custom ULA is the only non-CPU chip, and donor's ULA is
~250 LoC of `match` statements that port cleanly inline into the
machine layer. Reuses our `mos-6502` (the 2A03 sibling chip-crate
from NES).

Fresh-write machine layer with the BBC-Micro-compatible 8-colour
palette, eight display modes (0-7 except MODE 7 teletext which the
Electron's ULA doesn't implement), 14×4 keyboard matrix scanned via
the address bus on `$FE00` reads, VBlank + 100 Hz RTC IRQ sources,
sound generation through the ULA's tone counter, and ROM-page
register at `$FE05`.

- **L — Acorn BASIC II ROM missing from TOSEC.** The Electron OS
  ROM (16 KB, SHA-256
  `b63f851d79498f598999d923b7c9f62e2525c34f0b9cd2d4b328b89d622dcda4`)
  is in TOSEC at
  `Acorn/Electron/Operating Systems/Acorn Electron OS (1983)(Acorn).zip`
  and now installed at `~/.emu198x/roms/acorn-electron/os.rom`.
  The 16 KB **Acorn BASIC II ROM is not in TOSEC's Acorn tree**
  (neither under Acorn/BBC nor Acorn/Electron — `Acorn BASIC` only
  appears as cassette/disk programs). Confirmed live: with a stub
  16 KB `$FF`-filled BASIC, the OS reaches MODE 6 display init
  and paints the canonical **all-red "Language?" error screen**
  (real-hardware behaviour when no valid language ROM sits in the
  sideways slot). Sourcing options: Stairway to Hell preservation
  archive, openMSX/B-em firmware bundles, or extraction from a
  real Electron / BBC.
- **A — ULA bus contention not modelled.** Real Electron CPU
  halves to 1 MHz during ULA RAM-fetch windows; this initial port
  runs CPU at a flat 2 MHz. Significant cycle-accuracy gap on a
  machine whose software is heavily sensitive to it (Elite, many
  scrollers).
- **A — Sideways ROM paging at `$FE05`** stores the page register
  but doesn't yet swap a paged-ROM array into the
  `$8000-$BFFF` window — only the default BASIC ROM is visible.
- **A — Cassette I/O via `$FE04`** is a write-stub.
- **A — Palette encoding** uses a simplified
  "physical = (value >> 4) & 0x07" decode for each register; real
  ULA encoding is more elaborate per the BBC-Micro-compatible
  spec.
- **A — Snapshot deferred** (shared family pattern).
- **S — Full shell parity**.
- **S — BBC Micro** is the natural next 6502-family extraction
  from this base. It reuses 6502 ✓ and SN76489 ✓, adds VIA-6522
  (new) and Motorola 6845 CRTC (new).

## Tatung Einstein TC-01 — `emu198x-tatung-einstein` (new, 2026-06-01)

Eighth donor-codebase extraction. **Zero new chips** — reuses
TMS9918A and AY-3-8910 (via `gi-ay-3-8912`). Fresh-write machine
layer with Einstein-specific port-`$21` ROM page-out (any write
flips the 8 KB X-TAL MOS ROM out of `$0000-$1FFF` and exposes the
full 64 KB RAM), AY-driven keyboard (8×8 matrix, row select on
AY R14 / port A, column read on port `$20`), and Z80 CTC channel 0
stub at `$28`.

- **L — BIOS boot reaches VDP init but no text output.** With the
  1983 X-TAL MOS v1.2 ROM (8 KB, SHA-256
  `401d0e0bf6f64ba82e68137525749171fbcf9bd9055a49e5ac9a47941a6a0ae1`)
  the BIOS sets the TMS9918A backdrop to blue and then hangs
  waiting for the **WD1770 floppy controller** that we don't
  model. Real Einstein hardware shows a "DISK FAIL" message or
  loads CP/M from disk; without the WD1770 the boot can't proceed
  to text. Gated smoke at
  `crates/machine-tatung-einstein/tests/bios_boot.rs` asserts the
  VDP-init stage (1024+ non-black pixels = display enabled with
  backdrop) so we know the chips are reachable.
- **A — Z80 CTC may also be required.** Channel 0 is stubbed at
  port `$28`; like Sord M5, the Einstein wires VDP /INT through
  the CTC for IM-2 vectoring. The X-TAL MOS does set IM 1 early
  (visible in the disassembly), so the immediate boot path is
  unlikely to need CTC vectors, but later software almost
  certainly will.
- **A — TMS9918A scanline-batched render** (shared family debt).
- **A — Cassette / printer ports** unwired.
- **A — Snapshot deferred**.
- **S — Full shell parity**.

## Mattel Aquarius — `emu198x-mattel-aquarius` (new, 2026-06-01)

Seventh donor-codebase extraction. **Zero new chips** — Z80-only
machine with custom character-display rendering (320×192 = 40×24
8×8 cells, TEA1002 16-colour palette, character generator at the
upper 2 KB of the 8 KB Microsoft BASIC ROM). 8-row keyboard read
through port `$FF` with the row select on address lines A8-A15
(active-low). VBlank drives Z80 NMI (50 Hz PAL pulse).

**Live boot verified 2026-06-01** with the 1982 Microsoft Aquarius
BASIC ROM (8 KB, SHA-256
`277b2655a0599861302e6ec86b027c65a29e284fe7aadc296c4570498ab0e249`).
BIOS reaches its idle loop (PC ≈ `$1EA7`) and paints the
characteristic magenta-and-black title screen — the cold-init fill
character produces the iconic repeated-tile background, with the
title text rendered as dark glyphs over the magenta backdrop. Gated
smoke at `crates/machine-mattel-aquarius/tests/bios_boot.rs` (1/1).

Bug fixed during boot bring-up: the donor's source comment on the
colour byte layout claimed `bits 0-3 = foreground`, but the BIOS
actually writes `high nibble = foreground, low nibble = background`.
Corrected with a note pointing at the donor's misleading comment.

- **A — Per-scanline display rendering.** The current port renders
  the whole framebuffer in one call at end-of-frame. Mid-frame
  changes to char / colour RAM won't be visible until the next
  frame.
- **A — TEA1002 palette tuning.** Donor palette is plausible but
  not calibrated against real-hardware photos.
- **A — 1-bit speaker downsample to 48 kHz** is exposed via
  `speaker_bit()` but the binary doesn't write a .wav yet.
- **A — Mini-Expander AY-3-8910 stub.** Port `$FC` writes are
  swallowed; some games rely on the optional Mini-Expander PSG.
- **A — Snapshot deferred** (shared family pattern).
- **S — Cassette I/O** via port `$FE` not yet wired.

## Spectravideo SVI-328 — `emu198x-spectravideo-svi-328` (new, 2026-06-01)

Sixth donor-codebase extraction. **Zero new chips** — reuses
TMS9918A, AY-3-8910 (via `gi-ay-3-8912`), and Intel 8255 PPI exactly
as MSX1 does. Fresh-write machine layer with Spectravideo-specific
simpler memory map: 32 KB system ROM at `$0000-$7FFF` overlaid with
RAM via port `$97` bit 0, 16 KB RAM at `$8000-$BFFF` overlaid with
cart via port `$97` bit 1, 16 KB RAM at `$C000-$FFFF` always. I/O
window tightened to `$80-$97` with 11×8 keyboard matrix via the
8255 (row select on port C, column read on port B).

- **L — BIOS not yet available.** The 32 KB SVI-318/328 system ROM
  (BASIC + OS) is not in the TOSEC dump and not in any standard
  emulator-bundle path on this machine. Gated smoke at
  `crates/machine-spectravideo-svi-328/tests/bios_boot.rs` waits
  for one to land at
  `~/.emu198x/roms/spectravideo-svi-328/svi-328.rom`. Real BIOS
  ships with openMSX/blueMSX firmware bundles; can also be
  extracted from a real SVI-328.
- **A — TMS9918A scanline-batched render** (shared family debt).
- **A — Centronics printer is a write-stub** (`$90-$91`).
- **A — Cassette I/O.** Real SVI-328 software loads from cassette
  through PPI port C bits 4-7; not yet wired (port C bits 0-3 do
  drive the keyboard row select correctly).
- **A — Snapshot deferred** (shared family pattern).
- **S — Full shell parity** for `emu198x-spectravideo-svi-328`
  follows once boot completes.
- **S — SVI-318 sibling.** Same chip stack, 16 KB RAM instead of
  32 KB, slightly different keyboard matrix. Mostly the same
  binary with a variant flag.

## Sega Master System — `emu198x-sega-master-system` (new, 2026-06-01)

Fifth donor-codebase extraction. Adds the **Sega VDP** (315-5124 /
315-5246) as a new chip crate — TMS9918A derivative with Mode 4
(4bpp tiles, dual 16-colour palettes from 64-colour pool, 8 sprites
per line, scroll registers, line interrupt counter, H/V counter
readback). Reuses SN76489 from the Coleco family. Fresh-write
machine layer with the **Sega mapper** (`$FFFC-$FFFF` bank
registers + cart RAM control), 8 KB RAM mirrored across
`$C000-$FFFF`, GG-specific extensions (`$00` START button + `$06`
PSG stereo), Pause→NMI line, no BIOS required.

**Live boot verified 2026-06-01.** Alex Kidd in Miracle World
(1986, US, 128 KB) boots straight to the canonical title screen on
first try after the cart-bank-masking fix landed —
"ALEX KIDD / IN MIRACLE WORLD" full Mode 4 multi-colour title,
character vignettes, "PUSH START BUTTON / © SEGA 1986" footer.
Gated smoke at `crates/machine-sega-master-system/tests/cart_boot.rs`
(picks first `.sms` from `~/.emu198x/media/sega-master-system/`)
passes (1/1).

- **A — Sega VDP only exposes `tick_scanline()`** (no per-dot
  tick), so the machine accumulates 228 T-states per scanline and
  issues one batched scanline tick at the boundary. More
  accuracy-relaxed than `ti-tms9918`'s per-dot tick. Refining
  `sega-vdp` to a per-dot model is the obvious next step.
- **A — Cart RAM at `$8000-$BFFF`** (when mapper control bit 3 is
  set) reads as `$FF` in this initial port; full SRAM
  write/read/persistence path needed for Phantasy Star, Wonder Boy
  III, Golvellius etc.
- **A — Sega mapper bank masking.** Real-hardware bug behaviour
  around non-power-of-two cart sizes not yet modelled; current
  impl uses `next_power_of_two() - 1` mask which is correct for
  the common power-of-two cart sizes (32 / 64 / 128 / 256 / 512 KB).
- **A — Line interrupt counter** wired through `vdp.interrupt` but
  programmer-side behaviour (R10 reload + status bit) needs
  validation against real software that scrolls split-screens.
- **A — YM2413 FM-PAC.** Mark III + some carts have an optional
  YM2413 FM synthesis chip mapped at `$F0-$F2`. Out of scope here;
  separate chip crate when needed.
- **A — Snapshot deferred** (shared family pattern).
- **S — Game Gear** is most of the way there — same chip stack,
  smaller 160×144 visible region inside the same VDP framebuffer.
  Stereo PSG via `$06` is wired; runtime exposes
  `SmsVariant::GameGear`. Lacks a `.gg` cart smoke test.
- **S — Full shell parity** for `emu198x-sega-master-system`
  (native verifier window).

## Sord M5 — `emu198x-sord-m5` (new, 2026-06-01)

Fourth donor-codebase extraction. Reuses TMS9918A + SN76489 from
ColecoVision / SG-1000; no new chips at the chip-crate level.
Fresh-write machine layer with Sord-specific memory map (cart at
`$2000-$6FFF`, 4 KB RAM at `$7000-$7FFF`, optional cart RAM at
`$8000-$BFFF`), 10×8 keyboard matrix on PPI port C → port B-style
row strobe + column read at `$30-$37` / `$20-$27`, and the same
correct 3:2 VDP-phase clock as SG-1000 / MSX.

- **L — BIOS boot does not complete.** The Monitor ROM uses IM 2
  with `I = $70` and expects the Z80 CTC channel that receives
  VDP `/INT` to deliver its programmed vector byte. We model VDP
  `/INT` as driving the Z80 `/IRQ` line directly, with IntAck
  returning `$FF` (the documented stub). The BIOS init loop
  reaches roughly `$0BFE` / `$14AC` but never crosses past VDP
  register init — the framebuffer stays all-backdrop and the
  CPU never reaches cart code at `$2000+`. The IM 2 vector
  table at `$7000-$7007` is correctly populated (`$186C` no-op,
  `$1861` VBlank, `$01DF` cassette / keyboard) but the CTC's
  channel-VDP wiring + vector-base programming aren't modelled.
  **Prereq: `zilog-z80-ctc` chip crate.**
- **A — Z80 CTC is the natural next chip-crate addition.** Four
  channels, counter / timer modes, control-register decode,
  channel-specific vector generation off clock pulses. The CTC
  is also used by Memotech MTX (keyboard timing) and Tatung
  Einstein (system timing), so the cost amortises across three
  machines on this list.
- **A — TMS9918A scanline-batched render** (shared family debt).
- **A — Snapshot deferred** (shared family pattern).
- **S — Full shell parity** for `emu198x-sord-m5` follows once
  boot completes.

## MSX1 — `emu198x-msx` (new, 2026-06-01)

Third donor-codebase extraction. Reuses TMS9918A from ColecoVision
and our existing `gi-ay-3-8912` PSG (software-equivalent to the
AY-3-8910 for MSX's joystick scheme); adds the **Intel 8255 PPI**
chip crate as the only new silicon. Fresh-write machine layer with
the **MSX-signature memory-slot system** (PPI port A → primary slot
per 16 KB page), an 11×8 keyboard matrix, two cart slots with
**MegaROM mapper support** (Plain / Konami / Konami SCC / ASCII 8 /
ASCII 16), and the **correct 3:2 VDP-dot-per-T-state phase clock**.
Headless binary `emu198x-msx` with `--bios`, `--cart`, `--mapper`,
`--region`, `--frames`, `--screenshot` flags. Gated BIOS-boot smoke
at `crates/machine-msx/tests/bios_boot.rs` waiting for a 32 KB
BIOS at `~/.emu198x/roms/microsoft-msx/msx.rom` (real BIOS from
TOSEC) or `cbios_main_msx1.rom` (free GPL C-BIOS replacement).

- **Live boot verified 2026-06-01.** The 1983 Microsoft US MSX
  System v1.0 + BASIC BIOS (32 KB, SHA-256
  `3b33130d959337be63182c4eae217797774b52322f8eb9e35ab20747412ed417`)
  boots cleanly to the canonical MSX BASIC prompt — `MSX BASIC
  version 1.0 / Copyright 1983 by Microsoft / 28815 Bytes free /
  Ok` plus the function-key strip — on a light-blue background.
  Slot 0 BIOS read, slot 3 RAM hydration, PPI port A slot select,
  TMS9918A text-mode render, and keyboard-matrix-quiescent BASIC
  init all verified through real BIOS code. Gated smoke at
  `crates/machine-msx/tests/bios_boot.rs` now passes (1/1).
- **A — TMS9918A scanline-batched render** (shared with Coleco +
  SG-1000; will resolve together).
- **A — Subslot expansion.** MSX1 doesn't need it; MSX2+ uses
  writes to `$FFFF` (when slot 3 is selected for page 3) to
  expand each primary slot into 4 subslots. Field recognised in
  the spec but disabled. Wire when targeting MSX2.
- **A — Joystick / cassette / printer ports.** PSG R15 selects
  joystick; PSG R14 reads joystick data. The hookup is in place
  on the chip side but no joystick input surface on the machine
  yet (host can poke registers via `psg_mut()` if needed). Cassette
  and printer through PPI port C bits 4-7 unwired.
- **A — Snapshot deferred** (shared pattern with Coleco + SG-1000).
- **S — Full shell parity** for `emu198x-msx` (native verifier
  window).
- **S — MSX2 / MSX2+ / TurboR.** V9938 / V9958 VDP, mapped RAM,
  YM2413 FM-PAC, subslots. Out of scope; current `machine-msx`
  is MSX1-only.
- **S — TMS9918 family expansion** continues to be cheap from
  here: Sord M5, Memotech MTX, Spectravideo SVI-328 all reuse
  TMS9918 + SN76489 (Sord/Memotech) or TMS9918 + AY-3-8910
  (SVI-328 same as MSX, basically).

## Sega SG-1000 / SC-3000 — `emu198x-sega-sg-1000` (new, 2026-06-01)

Second donor-codebase extraction landed: reuses the TMS9918A + SN76489A
chip pair from the ColecoVision extraction, no new chips. Fresh-write
machine layer with a **correct 3:2 VDP-dot-to-CPU-T-state phase
counter** (more accurate than ColecoVision's initial-port 3:1 ratio).
Headless binary `emu198x-sega-sg-1000` boots the canonical Tsukuda
Original "007 James Bond" Othello Multivision cart to its
level-select title screen. Gated cart-boot smoke at
`crates/machine-sega-sg-1000/tests/cart_boot.rs` (picks first `.sg`
file from `~/.emu198x/media/sega-sg-1000/` or `~/Downloads/`).

- **A — TMS9918A scanline-batched render** (shared with Coleco; will
  resolve together).
- **A — Upgrade ColecoVision to the 3:2 phase counter.** SG-1000 has
  it right; ColecoVision's initial port runs the VDP 3× too fast.
  Mechanical fix once the SG-1000 model is comfortable.
- **A — SC-3000 keyboard.** `set_pause_pressed` already drives the
  Z80 NMI line; full SC-3000 8255 keyboard matrix not yet modelled.
- **A — Snapshot deferred** (shared pattern with ColecoVision).
- **S — Full shell parity** for `emu198x-sega-sg-1000` (native
  verifier window).
- **S — Cart-mapper support.** SG-1000 ceiling is 48 KB; some Sega
  Mark III / late SG-1000 carts have bank-switching mappers (Sega,
  Codemasters, Korean variants). Out of scope for initial port; SMS
  will likely share the mapper layer when it lands.

## ColecoVision — `emu198x-colecovision` (new, 2026-06-01)

First donor-codebase extraction landed: TMS9918A + SN76489AN chip
crates ported from `Emu198x-Oldest`, machine wiring fresh-written
against the pin-driven bus pattern, headless binary boots the
canonical 1982 ColecoVision BIOS to its title screen
("COLECOVISION™ / TURN GAME OFF / © 1982 COLECO"). Gated BIOS-boot
smoke at `crates/machine-coleco-colecovision/tests/bios_boot.rs`
(loads BIOS from `~/.emu198x/roms/coleco-colecovision/`, runs 200
frames, asserts a non-trivial framebuffer).

- **A — Initial-port clock ratios.** Inherited from the donor:
  VDP runs 3 dots per CPU cycle with NTSC/PAL frame budgets of
  `342 × 262` and `342 × 313` CPU cycles. Real ColecoVision
  master crystal is 10.738635 MHz (CPU ÷ 3 = 3.579545 MHz; VDP
  dot ÷ 2 = 5.369 MHz), so the actual ratio is 1.5 dots per CPU
  cycle, not 3. Frame structure still completes correctly; real-time
  speed is off. Tracked here, fix when wall-clock matters.
- **A — TMS9918A scanline-batched render.** Donor renders the
  full scanline on dot-wrap-to-0 rather than incrementing pixels
  through the active display. Misses mid-scanline register writes
  and per-pixel effects. Refine when test ROMs (e.g. ColecoVision
  diagnostics, SCV graphics tests) point at visible defects.
- **A — Snapshot story.** Deferred from the machine layer. The
  current `ColecoVision` struct is unsynchronised; a runtime layer
  with proper `serde(skip)` design for chip framebuffer + audio
  buffer hydration is the natural home for save/restore.
- **A — IM 1 IntAck.** Returns `$FF` (floating bus) — matches BIOS
  expectation of `RST 38h` fetch. Real-hardware behaviour with a
  cartridge that drives the data bus during IntAck is unverified.
- **S — Full shell parity.** Headless-only `emu198x-colecovision`
  for now; native verifier window with `wgpu`/keyboard/audio/scripts
  matching `emu198x-nes`/`emu198x-c64` is a future commit.
- **S — TMS9918 family expansion.** Same chip crate is the
  foundation for SG-1000, MSX-1, Sord M5, Memotech MTX, Spectravideo
  SV-328. Same SN76489 also feeds SG-1000, SMS (with Sega VDP),
  BBC Micro. Pick the next extraction by curriculum / scene value.

## Cross-system shared work

- **A — Shared `wgpu` filter preset calibration** against hardware
  references — LCD for Game Boy, CRT for the TV / monitor systems
  (Spectrum, C64, NES, Amiga, Dragon). The presets exist; the
  calibration step is the work.
- **S — `scripts/verify-current-systems.sh` as the single CI gate.**
  Already runs unit/integration tests + conditional local-asset
  smokes. Worth keeping it the entry point as smoke counts grow rather
  than fragmenting into per-system scripts.

## Roadmap-adjacent (not active)

Status of the Emu198x-Oldest donor codebase extraction.

**Fifteen already extracted** in this run — see their dedicated
sections above:

| # | System | Live boot status |
|---|--------|------------------|
| 1 | ColecoVision | BIOS to title (live) |
| 2 | Sega SG-1000 / SC-3000 | Othello Multivision cart (live) |
| 3 | MSX1 | Microsoft BASIC (live) |
| 4 | Sord M5 | **Incomplete** — needs `zilog-z80-ctc` |
| 5 | Sega Master System | Alex Kidd in Miracle World (live) |
| 6 | Spectravideo SVI-328 | **Awaiting BIOS** (32 KB MSX-style system ROM) |
| 7 | Mattel Aquarius | Microsoft BASIC (live) |
| 8 | Tatung Einstein TC-01 | **VDP-init only** — needs WD1770 floppy |
| 9 | Acorn Electron | "Language?" red error (live) — needs Acorn BASIC II |
| 10 | Oric-1 / Atmos | **Awaiting BIOS** (16 KB Tangerine ROM) |
| 11 | Acorn BBC Micro Model B | OS bank-scan reaches BASIC slot (live) — needs SAA5050 + BASIC for full |
| 12 | Atari 2600 | Combat playfield (live) |
| 13 | Atari 5200 SuperSystem | Pac-Man title (live, partial render) |
| 14 | Atari 7800 ProSystem | Cart accepts (live); BIOS-driven boot pending |
| 15 | Atari 800XL | OS boots (live); BASIC ROM not yet sourced |

**Thirteen chip crates ported** as foundation:
`ti-tms9918`, `ti-sn76489`, `intel-8255`, `sega-vdp`, `motorola-6845`,
`mos-riot-6532`, `atari-tia`, `atari-antic`, `atari-gtia`,
`atari-pokey`, `atari-maria`, `mos-pia-6520`, plus our pre-existing
`gi-ay-3-8912` and `mos-via-6522` reused across the family.

**Still in the donor** (substantive, ready to port):
- The Amiga **AGA chipset scaffold** (Agnus AGA + Denise AGA —
  lighter, possibly incomplete; the current AGA path is the forward
  port).

**Donor stubs** (placeholder crates that aren't filled in — would
need writing from scratch): Jupiter Ace, Acorn Atom, ZX80 / ZX81,
Commodore PET, Commodore VIC-20,
Memotech MTX.

External-blocker holds:
- Sord M5 boot completion needs a `zilog-z80-ctc` chip crate (also
  unlocks Memotech MTX boot, plus Tatung Einstein's CTC channel 0
  stub).
- Tatung Einstein full boot needs a `western-digital-wd1770` floppy
  controller for the X-TAL MOS disk wait.

Extract on demand when expanding scope; do not rewrite from scratch.
