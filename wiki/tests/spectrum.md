# Spectrum Test Results

> Note: only the CPU test section below has been refreshed against the fresh Rust workspace as of 2026-04-12. The later system, performance, and media sections are historical notes and should not be treated as current status until they are rerun.

## CPU tests

| Suite | Result | Command |
|-------|--------|---------|
| Tom Harte | 1,604,000 / 1,604,000 (100%) | `cargo test -p zilog-z80 --test single_step_tests run_all -- --ignored --nocapture` |
| ZEXDOC | Pass | `cargo test --release -p zilog-z80 --test zex_tests run_zexdoc -- --ignored --nocapture` |
| ZEXALL | Pass | `cargo test --release -p zilog-z80 --test zex_tests run_zexall -- --ignored --nocapture` |
| FUSE | 1,350 / 1,356 exact, 6 accepted disagreements, 0 unexpected | `cargo test -p zilog-z80 run_fuse_z80_reference_suite -- --ignored --nocapture` |

### FUSE disagreements (6 accepted) — investigated, recorded

These 6 cases are documented disagreements between FUSE and the combination of Tom Harte plus the current fresh-workspace Z80 core. Tom Harte remains the primary CPU oracle. The FUSE harness now compares exact event trace, final register state, memory effects, and final T-state counts; the allowlist is limited to these named cases only.

| FUSE test | Opcode | Instruction | Disagreement | Tom Harte agrees with us |
|-----------|--------|-------------|--------------|--------------------------|
| `76` | `0x76` | `HALT` | PC: got `0x0001`, FUSE expected `0x0000` | Yes |
| `edb2_1` | `0xED 0xB2` | `INIR` | F bits 2,3 differ and `WZ` ends at `0x0001` instead of `0x0A41` | Yes |
| `edb3_1` | `0xED 0xB3` | `OTIR` | F bits 2,4 differ and `WZ` ends at `0x0001` instead of `0x02E1` | Yes |
| `edb9_2` | `0xED 0xB9` | `CPDR` | F bit 3 (X-undocumented): got `0xAF`, FUSE expected `0xA7` | Yes |
| `edba_1` | `0xED 0xBA` | `INDR` | `WZ`: got `0x0001`, FUSE expected `0x069E` | Yes |
| `edbb_1` | `0xED 0xBB` | `OTDR` | F bits 2,4 differ and `WZ` ends at `0x0001` instead of `0x033A` | Yes |

**Pattern.** Five of the six are block I/O / block compare instructions (`INIR`, `OTIR`, `CPDR`, `INDR`, `OTDR`). The flag formulas and `WZ` behaviour for these paths are among the messiest parts of Z80 compatibility work, and older references disagree. The important point for this repo is that the disagreements are now explicit and named instead of being hidden behind a vague pass count.

`HALT` at `0x76` remains the same PC-bookkeeping convention difference as before: we keep `PC` advanced past the HALT opcode, while FUSE leaves it pointing at the HALT.

**What would actually be a regression.** If a future Z80 change causes any other FUSE case to diverge, or changes the mismatch fields for one of the six listed above, that is a real regression and the harness will fail. The allowlist is explicit in the test code so the suite does not silently absorb extra disagreements.

**How to verify.** `cargo test -p zilog-z80 run_fuse_z80_reference_suite -- --ignored --nocapture` should report `1,350 / 1,356 exact, 6 accepted disagreements, 0 unexpected`.

## System tests

| Test | Status | Artefact |
|------|--------|----------|
| 16K boot | Pass | — |
| 48K boot | Pass | `test_output/spectrum_boot.png` |
| 128K boot | Pass | — |
| +2 boot | Pass | — |
| +2A boot | Pass | — |
| +2B boot | Pass | — |
| +3 boot | Pass | — |
| Pentagon boot | Pass | — |
| Scorpion boot | Pass | — |
| TC2048 boot | Pass | — |
| TS2068 boot | Pass | — |
| Border stripes (8 colours) | Pass | `test_output/spectrum_border_stripes.png` |
| Beeper tone | Pass | `test_output/spectrum_beeper_tone.wav` |
| Signal Part 3 (AY music + VU) | Pass | — |

## Performance baseline

Baseline `run_frame` throughput with a zeroed ROM (ULA + contention + memory + framebuffer hot path, NOT a realistic CPU workload). Measured via `cargo bench --bench run_frame` and expressed as multiples of realtime — how many frames of emulation the host can produce in one wall-clock frame (20 ms at 50 Hz).

| Variant | Frame time | Realtime multiple | Host |
|---------|-----------|-------------------|------|
| 48K     | ~2.47 ms  | ~8.1×            | Apple Silicon, dev build |
| 128K    | ~3.84 ms  | ~5.2×            | Apple Silicon, dev build |

128K is slower than 48K because of the extra T-states per frame (70,908 vs 69,888), the AY chip tick loop, the bank-switched memory model, and the split screen bank lookup.

The numbers leave generous headroom for rewind (serialise state every N frames), debugger instrumentation, audio time-stretching, and turbo tape loading. A realistic-workload bench with a real ROM is deferred until the test ROM strategy (see `test-roms/README.md`) resolves.

Run with:

```sh
cargo bench -p machine-sinclair-zx-spectrum-48k --bench run_frame
cargo bench -p machine-sinclair-zx-spectrum-128k --bench run_frame
```

## Running tests

```bash
# CPU unit tests
cargo test -p zilog-z80

# Integration tests (requires ROMs)
cargo test -p emu-sinclair-zx-spectrum --test integration -- --include-ignored --nocapture
```

## Supported media formats

| Format | Type | Notes |
|--------|------|-------|
| .TAP | Tape | Standard Spectrum tape format |
| .TZX | Tape | Extended tape format (15+ block types) |
| .Z80 | Snapshot | v1/v2/v3 supported |
| .SNA | Snapshot | 48K and 128K variants |
| .TRD | Disk | TR-DOS disk image (Beta disk interface — Pentagon, Scorpion) |
| .DSK/.EDSK | Disk | +3 disk image (NEC µPD765A, read-only, uniform tracks) |
| .ZIP | Archive | Any of the above can be loaded from within a ZIP |
| .RZX | Input recording | Replay format — parser and writer in `format-sinclair-zx-spectrum-rzx`. Replay harness pending the System trait (Phase 0.3). |

## Acid test: Signal Part 3

See [Signal Part 3](../systems/spectrum/signal-part-3.md). Music plays, VU meters pulse. Requires AY hardware (run as 128K, not 48K).
