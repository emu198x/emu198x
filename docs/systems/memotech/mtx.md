# Memotech MTX500 / MTX512

## Status: Boots to MTX BASIC `Ready`

UK Z80A machine (1983). Boots to the BASIC `Ready` prompt with the OS+BASIC+ASSEM
ROM image. Headless extended system. Z80 + TMS9918A + SN76489 + Z80 CTC.

## What works

- **Boot to BASIC** (2026-06-03) — programs the VDP + CTC and renders `Ready`.
  Smoke `tests/boot_trace.rs` (`boots_to_basic_ready`).
- **VDP interrupt via Z80 CTC** (2026-06-04) — CTC at `$08-$0B`, VDP `/INT` feeds
  **CTC channel 0**, the CTC's INT drives the Z80 IRQ (IM 2 + RETI daisy release).
  The test asserts the OS programs `$08-$0B` and ch0 runs interrupt-enabled.
- **Paging** — OS fixed at `$0000`, 16K RAM blocks page the upper windows
  (`RELCPMH` CP/M mode), per MEMU `mem.c`.

## Not implemented / accuracy gaps

- **Keyboard matrix mapping** — the drive/sense *model* is correct (no-key +
  country read verified), but the physical key→(column, sense-bit) grid still
  needs aligning to MEMU `kbd2.c` for accurate typing.
- **Cassette / Centronics** unwired. **Snapshot** + `.mtx`/`.run` load — deferred.
- **No native window.**

## Known unknowns / disproven hypotheses

- **DISPROVEN (donor): "port `$00` bit 0 = page 0→RAM."** It swapped the executing
  OS ROM out the instant the power-on RAM-sizing loop wrote 1, derailing into
  zeroed RAM. Rewrote `resolve` after MEMU `mem.c`.
- **DISPROVEN (donor I/O map): SN76489 at `$03`; single keyboard port.** Real
  (MEMU): PSG `$06`; keyboard reads **both** `$05` (sense low) and `$06` (sense
  high + country).
- **DISPROVEN: "OS+BASIC ROM is enough."** A stock board carries OS+BASIC+**ASSEM**;
  the cold-start `RST $28 #$50` runs from the ASSEM ROM (paged subpage 1). With
  OS+BASIC only it hit `$FF` and reset-looped. Needs the 24K image.
- **Verification target** — finish the keyboard grid vs `kbd2.c`.

## Validated against

- MEMU (`github.com/Memotech-Bill/MEMU`) — `mem.c` paging, `memu.c` I/O, `kbd2.c`
  keyboard model. Full map: `knowledge/systems/memotech-mtx.md`.
- `boots_to_basic_ready` (asserts CTC ch0 live).

## Crates

| Crate | Role |
|-------|------|
| `zilog-z80` | CPU |
| `ti-tms9918` / `ti-sn76489` / `zilog-z80-ctc` | VDP · PSG · CTC |
| `machine-memotech-mtx` / `runtime-…` / `emu198x-memotech-mtx` | wiring + runner |

## ROMs

OS + paged-ROM image (OS+BASIC+ASSEM, 24K) at `~/.emu198x/roms/memotech-mtx/`.

## Launch

```sh
cargo run --release -p emu198x-memotech-mtx -- --frames 300 --screenshot mtx.png
```
