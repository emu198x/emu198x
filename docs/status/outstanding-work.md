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

System-scope expansion candidates extracted from the Emu198x-Oldest donor
codebase — substantive (1000+ lines per crate) implementations exist but
are not yet wired into the current workspace: **Atari** 2600 / 5200 / 7800
/ 800XL with full chipset (ANTIC, GTIA, MARIA, POKEY, TIA), **MSX**,
**ColecoVision**, **BBC Micro**, **Acorn** Atom / Electron, **Sega**
Master System / SG-1000, **Oric**, **ZX80** / **ZX81**, **Jupiter Ace**,
**Memotech MTX**, **Mattel Aquarius**, **Tatung Einstein**, **Spectravideo
SVI-328**, **Sord M5**. Plus the Amiga **AGA chipset scaffold** (Agnus AGA
+ Denise AGA — lighter, possibly incomplete; the current AGA path is the
forward port). Extract on demand when expanding scope; do not rewrite from
scratch.
