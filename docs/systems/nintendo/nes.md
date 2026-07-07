# Nintendo Entertainment System (NES)

## Status: Cores at/near accuracy ceiling; breadth-limited

The NES has the **most finished chip cores in the fleet**. The 2A03 CPU is at the
Tom Harte ceiling (2.56 M single-step + nestest 8991/8991); the 2C02 PPU is
dot-exact and passes every PPU torture suite in the repo (sprite-0, overflow incl.
the hardware diagonal bug, vbl/NMI race, read-buffer); the APU passes the entire
blargg APU suite with an exact nonlinear mixer. It runs through the shared shell
with screenshots, audio capture, keyboard/gamepad, reset, snapshots, smoke-matrix
reporting, and Blargg-style `$6000` assertions.

The distance to 100% is therefore **not** the chips — it is breadth and a couple
of genuine bugs: mapper coverage (~75–82% of the NTSC library boots today),
controller-only peripherals (no Zapper/Four Score), PAL chip-ready but unwired,
a MMC5 register-read routing bug, and battery `.sav` that's parsed but never
persisted. Three narrow cycle-exact timing ROMs (DMA interleave + two CPU-timing
cases) remain on the core, closeable with reference traces.

## Hardware overview

- **CPU:** Ricoh 2A03 (6502 variant, no BCD mode, built-in audio)
- **Clock:** 21.477272 MHz master (NTSC), CPU at ÷12 (1.789773 MHz), PPU at ÷4 (5.369318 MHz)
- **Video:** PPU (2C02) — 256×240, 2 pattern tables (CHR ROM/RAM), 4 nametables, 64 sprites (8 per scanline), scrolling, palette of 64 colours (25 on screen)
- **Audio:** 2A03 built-in — 2 pulse channels, 1 triangle, 1 noise, 1 DPCM sample channel
- **Input:** Two controller ports (D-pad + A/B/Select/Start)
- **Storage:** Cartridge with mapper hardware (many variants)

## Implementation status

- **6502 CPU / 2A03 variant** — done and validated by `nestest`
- **PPU** — dot-driven 2C02 path with nametable mirroring and frame output
- **APU** — 5-channel 2A03 audio with host-side channel controls
- **Mapper system** — NROM, MMC1, UxROM, CNROM, MMC3, MMC5, AxROM, Color Dreams, VRC2a, Action 53, BxROM/BNROM, NINA-001, Sunsoft-4, and Camerica/Codemasters are implemented; the remaining long-tail mappers are compatibility-driven
- **iNES/NES 2.0** — ROM format parsing with mapper detection

## Automated test ROM checks

The headless `emu198x-nes` runner (`--no-default-features` skips the graphics stack) can assert Blargg-style test ROM output written at `$6000`. A passing ROM exits successfully and includes `test_result` in the JSON report; running, reset-requested, failed, or non-Blargg ROMs return a non-zero exit code.

```sh
cargo run --release -p emu198x-nes --no-default-features -- --rom apu_test.nes --frames 3000 --assert-blargg
cargo run --release -p emu198x-nes --no-default-features -- --smoke-root path/to/blargg/rom_singles --frames 1200 --assert-blargg --smoke-report tmp/nes-apu-blargg-report.json
```

## Not implemented / accuracy gaps

- **Mapper long tail** — 14 mapper numbers are in (NROM, MMC1, UxROM, CNROM,
  MMC3, MMC5, AxROM, Color Dreams, VRC2a, Action 53, BxROM/NINA-001, Sunsoft-4,
  Camerica); ~75–82% of the commercial NTSC library boots. **Missing cheap wins:**
  MMC2 (9) / MMC4 (10) — Punch-Out!!, Fire Emblem — and GxROM (66). **Missing tail
  (effort-heavy, mostly JP/niche):** the VRC4/6/7 IRQ + audio family, Namco 163,
  Sunsoft 5B. 155-ROM sweep (2026-06-05): 135 PASS / 5 FAIL / 15 VISUAL.
- **MMC5 register-read routing bug** — the machine returns open-bus for
  `$4020–$5FFF` and never calls `mapper.cpu_read` there, so MMC5's IRQ-status,
  multiplier, and ExRAM *reads* are dead (writes work). The mapper itself is
  implemented; the routing gap is flagged in-code (`machine-nintendo-nes/src/lib.rs:528`).
  Any MMC5 game that reads back IRQ status or the multiplier misbehaves. Small fix.
- **Battery `.sav` not persisted** — `has_battery` is parsed and PRG-RAM is
  battery-backed in the mappers, but no code loads/flushes a `.sav` file, so saves
  survive a session (and snapshots) but not across runs. RPGs are unplayable as
  intended. Small–Medium.
- **PAL/Dendy not selectable** — the APU has full PAL tables and the PPU accepts
  311 lines, but the machine hardwires NTSC (`Ppu::new()` + a fixed 3:1 divider;
  PAL needs 3.2:1) and the only runtime profile is `NesNtsc`. The whole PAL
  library runs at the wrong speed/timing today. Small–Medium to plumb through.
- **CPU edge timing** — `blargg_nes_cpu_test5` is **10/11** sub-tests; only
  `01-implied` lacks its `[OK]` marker (a side-effect on one implied/NOP opcode
  the looser standalone `instr_test` misses). `cpu_timing_test6` runs and reports
  a real `$F0` fail (frame-relative instruction timing) — both need a reference
  trace to localise, not guessing.
- **DMA interleave** — `sprdma_and_dmc_dma` (code 1) still fails; the OAMDMA
  odd-cycle penalty + DMC sample-DMA cycle interleave aren't cycle-exact. The
  read-side `dmc_dma_during_read4` ROMs are VISUAL (no `$6000` protocol), not
  fails. The remaining known core accuracy frontier — machine-layer DMA, not the
  APU or PPU.

## Test-ROM ledger (2026-06-07)

Landed as `#[ignore]`-gated ledger tests in `crates/machine-nintendo-nes/tests/`.
Run with `cargo test -p machine-nintendo-nes --test <file> -- --ignored`.

- **`mmc3_test` 5/6** (`blargg_ppu.rs`) — scanline-IRQ A12 counter. Was 0/6: the
  PPU only notified the mapper of A12 edges while rendering, so the `$2006`-driven
  counter clocks the suite uses during forced blank were dropped. Now notifies on
  every edge with the PPU cycle, and the MMC3 filter measures A12 low-duration
  (Mesen's `_a12LowClock`). `6-MMC6` is intentionally unreachable — it tests the
  MMC3 rev-A IRQ behaviour that contradicts `5-MMC3`, and the two ROMs share
  identical mapper-4 headers (no per-ROM chip database here).
- **`ppu_read_buffer` PASS** (`blargg_ppu.rs`) — Bisqwit's ~80-sub-test `$2007`
  read-buffer suite. See the corrected note below.
- **`cpu_dummy_reads` PASS** (`blargg_legacy.rs`) — 6502 dummy reads on
  abs,X / (zp),Y / (zp,X). Older shell, no `$6000`; graded by scanning the
  ascii.chr nametable for "Passed".
- **sprite_hit 11/11 + sprite_overflow 5/5** (`ppu_onscreen.rs`) — 2005 suites
  graded via the `$f8` result byte; sprite-0-hit and overflow timing all pass.

## Known unknowns / disproven hypotheses

- **CORRECTED: `test_ppu_read_buffer.nes` now passes via `$6000`.** The earlier
  (2026-06-01) "reports via screen + audio, not `$6000`" conclusion was wrong: the
  ROM *does* write the `$6000` shell block, but it's CNROM (mapper 3) and the
  mapper port carried no `$6000` work RAM, so the signature never landed and the
  run looked like an endless "running"/VISUAL state. Adding 8 KiB WRAM at
  `$6000-$7FFF` on CNROM (as NROM already does) makes it report `Some(0)` — and
  the `$2007` read-buffer behaviour itself was already correct (2026-06-07).
  *Lesson:* a `None`/VISUAL verdict from the `$6000` harness can mean the cart's
  mapper lacks `$6000` WRAM, not a real PPU/CPU bug — check the mapper first.
- **Open: the 01-implied culprit** — `cpu_test5` is 10/11; the one implied/NOP
  opcode side-effect that fails the stricter CRC is not yet isolated. (The earlier
  "2/20 CRC probe" figure was stale.)
- **Open: the 5 FAIL ROMs** in the 155-sweep — individual causes not catalogued
  on this page.
- **Verification targets** — exact PPU/APU timing claims are from secondary
  knowledge; confirm against the NESdev wiki + Visual2C02/Visual2A03, not just
  passing test ROMs.

## Validated against

- `nestest` 8991/8991; Blargg-style `$6000` test ROMs; the 155-ROM smoke sweep.
  Super Mario Bros. renders.
- Per-suite ledgers (2026-06-07): `mmc3_test` 5/6, `ppu_read_buffer`,
  `cpu_dummy_reads`, sprite_hit 11/11, sprite_overflow 5/5, `ppu_vbl_nmi` 10/10,
  `instr_test-v5`, `instr_misc`, `oam_read`, `cpu_exec_space` (see the test-ROM
  ledger above; harnesses in `crates/machine-nintendo-nes/tests/`).
- Reference: Mesen2, fceux, nestopia (`emulators/nes/`).

## Timing & cycle-accuracy

- **Master clock & dividers** — 21.477272 MHz NTSC. CPU = ÷12 (1.789773 MHz);
  PPU = ÷4 (5.369318 MHz) — the 3:1 PPU:CPU relationship.
- **Timing model realised** — strong: a **dot-driven 2C02 PPU** interleaved with
  the CPU at the 3:1 ratio. The remaining timing gaps are specific DMA/edge cases,
  not the core model.
- **CPU timing** — 2A03 cycle-accurate (§62; nestest 8991/8991 + Tom Harte prove
  the ISA).
- **Distance to full cycle-accuracy** — OAMDMA odd-cycle penalty + DMC sample-DMA
  interleave; `cpu_timing_test6`; the `blargg_nes_cpu_test5` 01-implied case.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp`; Blargg `$6000` assertion + smoke-matrix.
- **Native window** — yes (primary tier): shared `wgpu` `raw`/`lcd`/`crt`,
  keyboard/gamepad.
- **Disassembler** — pending the Asm198x shared 6502 disassembler.

## Peripherals & connectivity

- **Emulated now** — cartridge (many mappers), controllers.
- **Period peripherals (emulatable)** — Zapper light gun, R.O.B., Power Pad, Four
  Score, the Famicom Disk System.
- **Internet-capable** — **Yes (Japan)**: the **Famicom Modem / Famicom Network
  System** (1988) ran stock-trading and betting services — a real, documented,
  emulatable modem. Modern flash-cart WiFi is also feasible. Marginal in the West.

## Crates

| Crate | Role | Status |
|-------|------|--------|
| `mos-6502` | Shared 6502 core with 2A03 mode | Done |
| `ricoh-ppu-2c02` | NES PPU | Ported |
| `ricoh-apu-2a03` | NES APU | Ported |
| `format-nintendo-nes-ines` | iNES parser + NROM/MMC1/UxROM/CNROM/MMC3/MMC5/AxROM/Color Dreams/VRC2a/Action 53/BxROM/NINA-001/Sunsoft-4/Camerica mappers | Active |
| `machine-nintendo-nes` | NES machine wiring | Active |
| `runtime-nintendo-nes` | Shared shell runtime | Active |
| `emu198x-nes` | Native verifier shell | Active |

## Road to 100%

The remaining work is tiered: bugs first (MMC5 read routing, `.sav`), then
library coverage (MMC2/4, GxROM, PAL, Zapper/Four Score), then the cycle-exact
core finish, then expansion-audio + FDS breadth.
