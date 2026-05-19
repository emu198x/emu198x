# Spectrum Test Results

> Note: only the CPU test section below has been refreshed against the fresh Rust workspace as of 2026-04-12. The later system, performance, and media sections are historical notes and should not be treated as current status until they are rerun.

## CPU tests

| Suite | Result | Command |
|-------|--------|---------|
| Tom Harte | 1,604,000 / 1,604,000 (100%) | `cargo test -p zilog-z80 --test single_step_tests run_all -- --ignored --nocapture` |
| ZEXDOC | Pass | `cargo test --release -p zilog-z80 --test zex_tests run_zexdoc -- --ignored --nocapture` |
| ZEXALL | Pass | `cargo test --release -p zilog-z80 --test zex_tests run_zexall -- --ignored --nocapture` |
| FUSE | 1,350 / 1,356 exact, 6 accepted disagreements, 0 unexpected | `cargo test -p zilog-z80 run_fuse_z80_reference_suite -- --ignored --nocapture` |
| z80test (raxoft) | 6 / 6 exercisers pass: z80doc, z80docflags, z80flags, z80full, z80ccf, z80memptr (with 2 accepted INIR/INDR MEMPTR disagreements — sibling of the FUSE table below). z80ccfscr is visual-only and not gated. | `cargo test --release -p machine-sinclair-zx-spectrum-48k --test z80test -- --ignored --test-threads=1 --nocapture` |

### z80test — Patrik Rak's exerciser

Added 2026-05-18. Patrik Rak's `z80test` (MIT) is the modern gold-standard Z80 exerciser; it catches MEMPTR/WZ propagation and undocumented X/Y flag behaviour that ZEXALL is silent on. Reference catalogue: [`Emu198x-Reference/_organised/by-topic/testing-suites/spectrum-test-roms.md`](../../../../Emu198x-Reference/_organised/by-topic/testing-suites/spectrum-test-roms.md).

The harness lives at [`crates/machine-sinclair-zx-spectrum-48k/tests/z80test.rs`](../../crates/machine-sinclair-zx-spectrum-48k/tests/z80test.rs). For each TAP it boots the 48K ROM to READY, injects the CODE block at `$8000` (the address recorded in the TAP CODE header), jumps in with interrupts disabled, and traps PC entries at `$0010` (RST 16 = `PRINT-A-1`) to capture the test's printed transcript. The scroll-prompt counter at `$5C8C` is held high so the ROM never pauses for a key. Whole-suite runtime ≈ 175 s in release, single-threaded.

Required fixtures (resolved in this order):

- `$EMU198X_SPECTRUM_48K_ROM`, defaulting to `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
- `$EMU198X_Z80TEST_DIR/<name>.tap`, defaulting first to `~/.emu198x/test-data/z80test/<name>.tap`, then to `~/Projects/Emu198x-Unclean/Zen/Other Images/<name>.tap`. The seven canonical TAPs are already locally cached in the Unclean copy.

Tests silently skip (returning `ok`) when either fixture is missing, so other developers on this branch can still run `cargo test --ignored` without these files.

#### z80memptr accepted disagreements (2)

| Test | Instruction | Disagreement |
|---|---|---|
| `102 INIR->NOP'` | INIR followed by an instruction into the alternate set | MEMPTR/WZ propagation differs from Patrik Rak's expected value |
| `103 INDR->NOP'` | INDR followed by an instruction into the alternate set | MEMPTR/WZ propagation differs from Patrik Rak's expected value |

**Pattern.** These are the same block-I/O MEMPTR cases as the existing FUSE accepted disagreements (`edb2_1 INIR` and `edba_1 INDR` in the table below). Tom Harte agrees with our current behaviour; FUSE and Patrik Rak's z80memptr both disagree. Until the underlying behaviour question is resolved against silicon-level evidence, the harness asserts that exactly these two tests fail — any other shape of disagreement (or these tests starting to pass without explanation) fails the run.

### FUSE disagreements (6 in allowlist) — 2 reclassified as tracked bugs, 4 still under investigation

These 6 cases are documented disagreements between FUSE and the current Z80 core; Tom Harte's vectors happen to agree with our core in each case. The FUSE harness compares exact event trace, final register state, memory effects, and final T-state counts; the allowlist is limited to these named cases only.

Per [`decisions/spectrum-test-oracle-priority.md`](../decisions/spectrum-test-oracle-priority.md) (2026-05-18), Spectrum-validated oracles outrank Tom Harte for Spectrum work. The two block-I/O cases below (`edb2_1 INIR`, `edba_1 INDR`) match independent failures from Patrik Rak's `z80test` (`102 INIR->NOP'`, `103 INDR->NOP'`) and are now **tracked Z80 bugs to fix**, not accepted disagreements — see the `z80memptr` table above. The other four cases (HALT PC convention, `CPDR`, `OTIR`, `OTDR`) remain under investigation: each is a single-oracle disagreement and may or may not survive a closer silicon-level look. The fix work and any allowlist removals are blocked on the research item filed in [`Emu198x-Reference/_organised/known-unknowns.md`](../../../../Emu198x-Reference/_organised/known-unknowns.md) § Zilog Z80.

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

## ULA / floating-bus / contention tests

Added 2026-05-18. System-level Spectrum-native tests sourced from Spectron's bundled corpus (<https://github.com/oldbit-com/Spectron>) and cached at `~/.emu198x/test-data/spectrum-system-tests/`.

| Test | Status | Notes |
|---|---|---|
| Woody `Float48K.tap` | **Load chain passes, T-state assertion fails** — `cargo test --release -p machine-sinclair-zx-spectrum-48k --test float_bus -- --ignored` | Harness at [`crates/machine-sinclair-zx-spectrum-48k/tests/float_bus.rs`](../../crates/machine-sinclair-zx-spectrum-48k/tests/float_bus.rs). Drives the real tape pipeline: boots ROM, types `LOAD ""` via the keyboard matrix, plays the TAP at cycle-accurate speed. Always saves a PNG screenshot of the final framebuffer to `$TMPDIR/float48k.png` for visual diagnosis. **Surfaced a real bug**: the BASIC probe iterates through T-states near 14338 expecting display bytes from `IN A,($FF)`; our floating bus returns `255` (ULA-idle fallback) for the entire searched window, not the expected display byte. Tracked in [`Emu198x-Reference/_organised/known-unknowns.md`](../../../../Emu198x-Reference/_organised/known-unknowns.md) under ZX Spectrum ULA. Set `EMU198X_FLOAT48K_STRICT=1` to make the T-state assertion hard (currently it asserts only that the load chain works). |
| Woody `Float128k.tap` | Not yet wired | Same shape as 48K, 128K-specific ULA timing. |
| Ramsoft `floatspy.tap` | Not yet wired | Visual test — Spectron reference at `tests/Results/floatspy_48.png`. |
| `halt2int.tap`, `halt2int128.tap` | Not yet wired | HALT-to-interrupt timing. |
| `btime.tap`, `ptime.tap` | Not yet wired | Beeper / port I/O timing. |
| Mark Woodmass `Super HALT Invaders Test` | Not yet wired | Game-shaped HALT/IRQ torture test. |

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
