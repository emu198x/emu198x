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
