# Spectrum Test Results

## CPU tests

| Suite | Result | Command |
|-------|--------|---------|
| Tom Harte | 1,604,000 / 1,604,000 (100%) | `cargo test -p zilog-z80 --test single_step_tests run_all -- --ignored --nocapture` |
| ZEXDOC | Pass | `cargo test --release -p zilog-z80 --test zex_tests run_zexdoc -- --ignored --nocapture` |
| ZEXALL | Pass | `cargo test --release -p zilog-z80 --test zex_tests run_zexall -- --ignored --nocapture` |
| FUSE | 1,351 / 1,356 (99.6%) | Integration test |

### FUSE failures (5 remaining) — investigated, accepted

These 5 failures are documented disagreements between FUSE and Tom Harte. Tom Harte is derived from silicon-level testing of real hardware and is the more accurate of the two. We pass Tom Harte at 100% (all 1,604,000 cases) and our values for these specific instructions match Tom Harte's expected outputs. The five FUSE cases below are reported because future regressions could touch these code paths and we want to know exactly which cases are "expected to disagree" rather than treat the failure count as a black box.

| FUSE test | Opcode | Instruction | Disagreement | Tom Harte agrees with us |
|-----------|--------|-------------|--------------|--------------------------|
| `76` | `0x76` | `HALT` | PC: got `0x0001`, FUSE expected `0x0000` | Yes |
| `edb2_1` | `0xED 0xB2` | `INIR` | F bits 2,3 (P/V + X-undocumented): got `0x00`, FUSE expected `0x0C` | Yes |
| `edb3_1` | `0xED 0xB3` | `OTIR` | F bits 2,4 (H + P/V): got `0x03`, FUSE expected `0x17` | Yes |
| `edb9_2` | `0xED 0xB9` | `CPDR` | F bit 3 (X-undocumented): got `0xAF`, FUSE expected `0xA7` | Yes |
| `edbb_1` | `0xED 0xBB` | `OTDR` | F bits 2,4 (H + P/V): got `0x03`, FUSE expected `0x17` | Yes |

**Pattern.** Four of the five are block I/O / block compare instructions (`INIR`, `OTIR`, `CPDR`, `OTDR`). The H, P/V, and X (undocumented bit 3) flag formulas for these instructions are notoriously underspecified — different reference sources publish different formulas, and FUSE's expected values predate the modern reverse-engineering work that underpins the Tom Harte suite. The Z80 silicon implements these flags in terms of intermediate values from the CPU's internal computation, which the older sources approximated incorrectly.

The fifth (`HALT` at `0x76`) is a PC-bookkeeping convention difference: when `HALT` executes, the real Z80 has already incremented PC past the HALT opcode, and resumes at PC after a wake-up interrupt. We model PC advancing past the HALT (Tom Harte's convention); FUSE leaves PC pointing at the HALT and re-fetches it on each wake-up cycle. The two are observationally equivalent because both produce the same execution sequence, but the recorded PC at the moment FUSE checks differs by one byte.

**What would actually be a regression.** If a future Z80 change causes any *other* FUSE test to start failing — i.e. the count goes above 5, or any test outside this list appears in the failure output — that's a real regression and needs investigation. The five tests above stay on this list as long as we side with Tom Harte; if any of them ever flip and we agree with FUSE on something we previously disagreed with, that's worth investigating too because it suggests an unintended behaviour change.

**How to verify.** `cargo test -p machine-sinclair-zx-spectrum-48k --test fuse_tests -- --nocapture` prints the current pass/fail count and the first ten failures. Should always be exactly 5 failures and the test names should match the table above.

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
