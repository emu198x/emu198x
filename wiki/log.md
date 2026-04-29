# Wiki Log

Append-only record of ingests, queries, and lint passes.

---

## 2026-04-29 — C64 runtime split: queries / snapshot / input modules

**Type:** refactor (architectural)
**Trigger:** the user spotted that `runtime-commodore-c64/src/runtime.rs` (3013 lines) was a workspace outlier and asked whether the file size itself was a code smell. A survey confirmed it was: production code mixed snapshot envelope, query surface, input mapping, and lifecycle plumbing in one file with no separation; tests inline accounted for 1796 of those 3013 lines (60%); peer runtimes had already moved past the inline-tests pattern (Spectrum 19 tests in `tests/`, Amiga 37). Cov-4's "1225 uncovered lines" was diagnosis-by-aggregation — once split per module the gaps become legible.
**Result:** three new modules + `runtime.rs` shrunk to focus on lifecycle.

| File | Before | After | Concern |
|---|---|---|---|
| `runtime.rs` (production code) | 1216 | **692** | C64Runtime struct, lifecycle (`MachineCore` impl), trace state |
| `queries.rs` (new) | — | 442 | `C64SessionQueryProvider`, the 350+ query path strings, `c64_boot_status`, screen-text helpers, `parse_hex_u16` |
| `snapshot.rs` (new) | — | 89 | `SnapshotEnvelopeV1` + postcard `encode` / `decode` |
| `input.rs` (new) | — | 143 | `apply_input_event` + the 70-key C64 keyboard matrix |

**`runtime.rs`'s `MachineCore::snapshot` and `MachineCore::restore`** are now one-line delegators — `snapshot::encode(self)` / `snapshot::decode(self, bytes)`. The runtime gained a small set of `pub(crate)` accessors (`profile`, `iec_bus`, `drive8_cycle_accum`, `set_time`, `set_drive8`, `set_iec_bus`, `set_drive8_cycle_accum`) that the snapshot module uses to reach into runtime fields without making them all public. The inline `apply_input_event` call in `run_until` is now `crate::input::apply_input_event` — same signature, different home.

**Tests stay inline for now.** The 40 inline tests in `runtime.rs` (1786 lines) remain untouched in this commit; they'll move to `tests/` files split by topic in the follow-up. One small test (the input-mapping coverage check) moved into `input.rs` because it tests a private function (`c64_key_position`) that the new module owns.

**Verification:**
- `cargo test -p runtime-commodore-c64 --lib` — 25 passed, 15 ignored (15 ROM-backed kept ignored).
- `cargo test -p runtime-commodore-c64 --test boot_invariants -- --ignored` — KERNAL → READY. still passes (5.77s).
- `cargo test --workspace --lib` — 74 binaries all OK.
- `cargo clippy -p runtime-commodore-c64 --all-targets -- -D warnings` — clean.

**Architectural note.** This split is the first half of the broader cleanup the survey flagged. The user has indicated agreement to push the same shape to the other large files: `motorola-68000/src/{decode,cpu}.rs` (3578 + 4028 lines, mixed across CPU variants) — likely best handled by splitting the family into per-variant crates `motorola-68000`, `motorola-68010`, `motorola-68020`, `motorola-68030`, `motorola-68040` with shared infrastructure in `motorola-68k-common`. `format-nintendo-nes-ines/src/lib.rs` (3879 lines, 14 mappers in one file) wants per-mapper modules. `commodore-denise-ocs/src/lib.rs` (2407 lines) can move debug types to a `debug.rs` sibling. Codex-owned files (`motorola-6809`, `machine-dragon-32`, `emu198x-script-dragon/src/main.rs`) stay untouched while Codex iterates.

**Cov-4 follow-up.** The split makes Cov-4 directly tractable: per-module coverage will surface concrete gaps. The next commit on this track will move the 40 inline tests into per-topic `tests/` files (`tests/lifecycle.rs`, `tests/snapshot_roundtrip.rs`, `tests/queries.rs`, `tests/tape_autoload.rs`, `tests/disk_autoload.rs`), at which point Cov-4's 1225 uncovered lines will redistribute and become per-module gaps.

---

## 2026-04-29 — Cov-3 + Cov-2: 68000 carve-out + tick.rs correctness paths

**Type:** investigation + fix (Cov-2 and Cov-3 of [`docs/plans/2026-04-28-october-runup-plan.md`](../docs/plans/2026-04-28-october-runup-plan.md))
**Trigger:** continuing the coverage track after Cov-1. Two siblings:
- Cov-3 — `motorola-68000` had three files (`fpu.rs`, `mmu.rs`, `disasm.rs`) flagged as the largest absolute uncovered-line cluster in the workspace. The A500 (our only 68000-class target) has no FPU and no MMU; the disassembler is a debugging aid the running emulator never invokes.
- Cov-2 — `mos-6502/src/tick.rs` was at 44.1% line / 34.0% branch. Tom Harte covers opcodes; it does not drive reset, NMI, or RDY pins.

### Cov-3 — script-level carve-out

`scripts/coverage.sh` now passes `--ignore-filename-regex 'motorola-68000/src/(fpu|mmu|disasm)\.rs'` to every `cargo llvm-cov` invocation, with a top-of-file comment explaining each excluded file and pointing at this entry. The same regex is threaded through the run, the JSON summary, the LCOV report, and the HTML report so every output is consistent.

**Honest impact.** The carved files were not 100% uncovered — their `#[derive(Serialize, Deserialize)]` annotations, struct/enum scaffolding, and a few helper functions were exercised incidentally by serde tests in A.1. Removing them from the denominator removed ~2,400 lines of denominator and ~1,500 lines of numerator, lifting workspace line coverage by ~0.07 pp and branch coverage by ~0.8 pp (mid-run profdata snapshot). The point is honesty, not the headline number: those files describe code paths the active machines cannot reach, and tracking them as "uncovered" hid the real coverage of code that matters.

**Decode.rs / cpu.rs left in.** The plan's Cov-3 also flagged `decode.rs` (46.6%) and `cpu.rs` (45.2%) as containing 68010+ variant-specific code. Carving those out at the file level is wrong — they hold the 68000 paths too. A correct fix needs either (a) per-variant decode-table extraction or (b) explicit "I am running an M68000 model so these arms are unreachable" assertions. Deferred as a future ticket; not blocking.

### Cov-2 — directed tests for paths Tom Harte misses

Eight new tests landed in `mos-6502::tests`:

1. `reset_executes_seven_cycles_and_loads_vector` — confirms the 7-cycle reset shape (the 2026-04-18 chip-only-investigation lock-in).
2. `nmi_rising_edge_vectors_to_handler` — NMI safety-net path on first opcode-fetch.
3. `nmi_does_not_re_fire_while_held_high` — edge-triggered, not level-triggered.
4. `nmi_taken_with_interrupt_disable_set` — NMI is non-maskable.
5. `nmi_takes_priority_over_irq` — when both pending, NMI vector wins.
6. `sei_has_one_instruction_irq_delay` — parity with the existing CLI delay test.
7. `plp_clearing_i_has_one_instruction_irq_delay` — PLP closes the I-modifying-instruction-delay set.
8. `rdy_stalls_reads_but_lets_writes_through` — NMOS RDY behaviour (the C64 VIC-II badline depends on this).
9. `jam_opcode_halts_cpu` — JAM dispatch in tick.rs holds the CPU.

These are spec invariants the rest of the workspace already relies on, never directly asserted before. The NMI tests took two iterations — the chip's safety-net path at `tick.rs:59` (catches a rising edge on the very next opcode fetch) means NMI vectors faster than IRQ, which the test pattern had to mirror.

**Coverage delta on `tick.rs`:** L=44.1% → 44.6%, R=42.0% → 42.8%. Modest, because the bulk of `tick.rs` is per-addressing-mode/per-operation logic that Tom Harte exercises but the standing tests (without the corpus) cannot. Per the testing-policy, these are spec-driven additions; the coverage uplift is incidental.

### Verification

- `cargo test -p mos-6502 --lib` — 35 passed (was 26 after Cov-1, plus 9 new = 35).
- `cargo test --workspace --lib` — 74 binaries all OK.
- `cargo clippy -p mos-6502 --all-targets -- -D warnings` — clean.
- `cargo +nightly llvm-cov report` with the new `--ignore-filename-regex` confirms the FPU/MMU/disasm files no longer count toward the workspace total.

### Remaining coverage track

- ✓ Cov-1 (`mos-6502/cycle.rs`)
- ✓ Cov-2 (`mos-6502/tick.rs` correctness paths) — invariants captured; coverage uplift is small without Tom Harte in the standing run
- ✓ Cov-3 (`motorola-68000` script-level carve-out for FPU/MMU/disasm)
- ⏳ Cov-4 (`runtime-commodore-c64/src/runtime.rs` 1225 uncovered lines) — biggest absolute remaining gap
- ⏳ Cov-5 (Spectrum `.z80` snapshot format edge cases)
- ⏳ Cov-H1/H2/H3 — coverage hygiene (exclude wgpu / native-window glue, snapshot in releases, coverage-diff tool)

A clean re-run of `./scripts/coverage.sh` was attempted post-Phase-A but stalled on a failing Dragon test in Codex's working set (`emu198x-dragon::native_autoload_runs_real_textstar_when_available` — autoload doesn't reach Textstar within 180 frames). Not caused by these changes; the coverage refresh can wait until that's resolved.

---

## 2026-04-29 — Cov-1: mos-6502/cycle.rs investigation closed (14.8% → 100%)

**Type:** investigation + fix (Cov-1 of [`docs/plans/2026-04-28-october-runup-plan.md`](../docs/plans/2026-04-28-october-runup-plan.md))
**Trigger:** the workspace coverage report flagged `crates/mos-6502/src/cycle.rs` at 14.8% line coverage with 774 uncovered lines — surprising for a CPU validated to 2.47M Tom Harte cases. The plan asked: dead code, or unwired parallel path?
**Result:** **neither — it is the live decode-table consumed by `tick.rs` line 100, and the low coverage is honest.** The standing tests under `cargo test --workspace --lib` only cover three opcodes (`0xEA`, `0xAD`, `0x00`); the 2.47M Tom Harte cases that exercise the full opcode set live in `crates/mos-6502/tests/single_step_tests.rs::run_all`, which is `#[ignore]`'d behind a 1 GiB external corpus and never runs by default. The workspace coverage script doesn't pass `--ignored`, so it never sees those cases.

**Fix:** three new hermetic spec tests in `mos-6502::tests`:

1. `decode_table_covers_all_256_opcodes` — sweeps every `u8` opcode through `cycle::decode` and `Operation::category`. Asserts no panic, no missing arm.
2. `decode_table_categories_for_representative_opcodes` — locks in category assignments for representative read / write / read-modify-write / control / implied operations.
3. `decode_table_jam_opcodes_all_resolve_to_jam` — covers the 12 documented JAM stop-codes (`0x02 / 0x12 / 0x22 / ...`) and asserts they all decode to `AddrMode::Jam` + `Operation::Jam`.

These are spec-driven, not coverage-driven (per `docs/testing-policy.md`): "every opcode is mapped" and "every operation has a category" are real invariants the rest of the workspace already relies on, just never directly asserted. The Tom Harte sweep would have caught a missing arm at run time, but only when run with the corpus present.

**Coverage delta on `cycle.rs` under `cargo test -p mos-6502 --lib`:**

| Metric | Before | After |
|---|---|---|
| Lines | 134/908 (14.8%) | 908/908 (100.0%) |
| Regions | 47/242 (19.4%) | 242/242 (100.0%) |
| Functions | 3/3 (100.0%) | 3/3 (100.0%) |

774 uncovered lines closed in one move. The full 2.47M Tom Harte sweep stays as the deeper regression net (run via `cargo test -p mos-6502 --test single_step_tests run_all -- --ignored` when the corpus is present); the new hermetic sweep is the 0-cost daily check that the decode table stays well-formed.

**Verification:** `cargo test --workspace --lib` 74 binaries OK, clippy `-D warnings` clean.

**Consequence:** Cov-1 closed. Remaining coverage track investigations: Cov-2 (`mos-6502/src/tick.rs` at 44.1% line / 34.0% branch — probably needs directed unit tests for reset / IRQ / NMI / RDY-stall paths Tom Harte doesn't cover), Cov-3 (`motorola-68000` carve-out for FPU / MMU / disasm), Cov-4 (`runtime-commodore-c64/src/runtime.rs` 1225 uncovered lines), Cov-5 (Spectrum Z80 snapshot format edge cases).

The "Tom Harte 100% green / 14.8% file coverage" paradox the plan flagged is now resolved and documented. Future surprises of the same shape should hit `decode_table_covers_all_256_opcodes` first.

---

## 2026-04-29 — Paula owns disk read DMA end-to-end

**Type:** milestone (Phase A.4 of [`docs/plans/2026-04-28-october-runup-plan.md`](../docs/plans/2026-04-28-october-runup-plan.md), seam 2 of [`amiga-architecture-review.md`](decisions/amiga-architecture-review.md))
**Trigger:** the disk read DMA path was straddling four crates (floppy peripheral encoded MFM bytes, Agnus allocated DMA slots, Paula owned DSKLEN/DSKDATR/DSKBYTR/DSKSYN state, but the machine layer owned the WORDSYNC gate, the word countdown, and the DSKBLK trigger). The architecture review described this split as the source of the WORDSYNC residual bug and named it the actual Workbench-boot blocker. With seam 2 fixed, every disk-read responsibility lives in Paula, the chip the silicon assigns it to.

**Result.** Paula now owns:

- DSKLEN arming flip-flop (already there)
- Per-transfer word countdown (`disk_dma_words_remaining`)
- DSKLEN.WRITE direction at arm time (`disk_dma_is_write`)
- WORDSYNC suppression / sync-stripping (`disk_dma_wordsync_waiting`)
- The DSKBLK interrupt (already there, via `complete_disk_dma`)

The capture point is `write_dsklen` itself: when the second DMAEN write completes the arm sequence, the new fields snapshot the transfer parameters atomically. Zero-length transfers complete immediately at that instant — the same HRM semantics as before, now expressed in one place.

**New Paula API.** `tick_disk_dma_slot(&mut self, word: u16) -> Option<u16>` — the machine pumps in the next MFM word from the drive; Paula updates DSKBYTR/DSKDATR latches (via the existing `note_disk_read_word` primitive), checks the WORDSYNC gate, decrements the countdown, and either returns the word for the machine to write to chip RAM at DSKPT or returns `None` (suppressed: pre-sync, write-direction transfer, or no transfer in flight). When the countdown hits zero Paula self-clears `disk_dma_pending` and raises DSKBLK.

**Machine becomes glue.** `feed_next_mfm_word` is now ~5 lines around the `tick_disk_dma_slot` call:

```rust
if let Some(write_word) = self.paula.tick_disk_dma_slot(word) {
    let addr = self.agnus.dsk_pt & 0x001F_FFFE;
    self.memory.write_word(addr, write_word);
    self.agnus.dsk_pt = self.agnus.dsk_pt.wrapping_add(2);
}
```

DSKPT lives on Agnus (current workspace shape), so chip-RAM write + pointer increment stay machine-side. Everything else moved.

**Deletions from `machine-commodore-amiga-ocs`:**

- `DiskDmaRuntime` struct (29 lines)
- `service_disk_dma_word` fn (29 lines)
- `start_disk_dma_transfer` fn (21 lines)
- `disk_dma_runtime: Option<DiskDmaRuntime>` field on `AmigaOcs`
- The arm-detection block in the per-CCK tick loop (12 lines)
- `disk_dma_runtime` field on `AmigaOcsSnapshot` and the corresponding clone/restore lines

`note_disk_read_word` stays as a public primitive — Paula's existing tests (`paula_phase2_machine.rs`, `adkcon.rs`) drive it directly to verify DSKBYTR/DSKDATR latching without involving the DMA state machine. `tick_disk_dma_slot` calls it internally on the live data path.

**Verification:**

- `cargo test -p commodore-paula-8364 --lib` — 7 passed (existing Paula suite still green).
- `cargo test -p machine-commodore-amiga-ocs --lib` — 49 passed.
- `cargo test -p runtime-commodore-amiga --tests` — every existing diag + golden + ram-variant + snapshot suite still green.
- `cargo test -p runtime-commodore-amiga --test boot_invariants -- --ignored` — **Kickstart 1.3 → insert-disk** and **Workbench 1.3 → desktop** both still green (27.63s — same as the A.3 run, no measurable performance regression).
- `cargo test --workspace --lib` — 74 binaries, all OK.
- `cargo clippy -p commodore-paula-8364 -p machine-commodore-amiga-ocs -p runtime-commodore-amiga --all-targets -- -D warnings` — clean.

**Consequence.** The architecture review's seam 2 is closed. Phase A is now complete (A.0 + A.1 + A.2 + A.3 + A.4 all green). The architecture-review document should move from "Proposed (draft for review)" to "Implemented" for seams 1, 2, 4 and 5; only seam 3 (chip-owned `read_register_word` for byte-write merging) remains, and it's the lowest-priority follow-up.

The Paula disk DMA state is now also automatically captured by snapshots — Paula already derives serde, and the new fields ride on that derivation. A snapshot taken mid-disk-read will restore with the DMA transfer in the correct state, including the WORDSYNC gate and the remaining word count.

---

## 2026-04-29 — Amiga CPU bus refactor (BusTransaction / BusResponse)

**Type:** milestone (Phase A.3 of [`docs/plans/2026-04-28-october-runup-plan.md`](../docs/plans/2026-04-28-october-runup-plan.md), seams 1 + 4 of [`amiga-architecture-review.md`](decisions/amiga-architecture-review.md))
**Trigger:** `service_cpu_bus` in `machine-commodore-amiga-ocs` had grown to 324 lines with four byte-lane conventions in flight. Two of the three boot-blocker bugs the chip-only push fixed lived in this function. The plan named it the largest refactor in Phase A.
**Result:** the function is now **78 lines**. Every chip-select arm becomes a small handler that returns `BusResponse`, and a single `apply_bus_response` dispatcher applies the byte-lane extraction rule once.

New types:
- `BusTransaction { addr, is_read, is_word, data }` — the snapshotted bus cycle.
- `BusResponse::{ Byte(u8), Word(u16), WriteAck }` — what the chip drove on the data lines.

New per-chip handlers (in the AmigaOcs impl block, immediately after `service_cpu_bus`):
- `dispatch_cia_a` — handles CIA-A reads/writes, including the overlay update.
- `dispatch_cia_b` — handles CIA-B reads/writes, including DF0 control on PRB / DDRB writes.
- `dispatch_rtc` — old-address battery-backed RTC at `$DC0000-$DC003F`.
- `dispatch_autoconfig` — Zorro-II probe window with byte-write nibble mirroring.
- `dispatch_fast_ram` — autoconfig fast-RAM serving (after the board's base address is set).
- `dispatch_custom_register` — chipset registers (`$DFFxxx`) with all the per-offset reads and the existing `dispatch_custom_write` write path.
- `dispatch_memory` — chip RAM / slow RAM / ROM / unmapped fallback, including the `debug_watch_addr` instrumentation.

Lane rule, applied once in `apply_bus_response`:
- `Byte(b)` → `u16::from(b)`, regardless of access width.
- `Word(w)` → for word reads, `w` as-is; for byte reads, even address (UDS) takes high byte, odd address (LDS) takes low byte.
- `WriteAck` → `BusStatus::Ready(0)`.

`Memory::set_last_bus_value` is now driven uniformly from `apply_bus_response` — chip arms no longer thread it through their own paths.

**Architecture-review fix landed in the same pass.** `cia::decode_cia_a` previously returned `None` for even addresses (`addr & 1 == 1` check), making word reads and even-address byte reads to `$BFExxx` fall through to floating-bus instead of triggering CIA-A side effects. The architecture review flagged this as a landmine ("ICR-pending bit could go uncleared"). The parity check is removed; CIA-A now decodes on every access in its address space and the dispatcher delivers the byte via `BusResponse::Byte`. The `decode_cia_a` test was updated to assert the new behaviour and document the rationale.

**Verification:**
- `cargo test -p machine-commodore-amiga-ocs --lib` — 49 passed.
- `cargo test -p runtime-commodore-amiga --lib --test boot_invariants --test snapshot_roundtrip` — 25 passed.
- `cargo test -p runtime-commodore-amiga --test boot_invariants -- --ignored` — **Kickstart 1.3 → insert-disk** (25.69s) and **Workbench 1.3 → desktop** (same suite) both still green.
- `cargo test --workspace --lib` — 74 binaries, all OK.
- `cargo clippy -p machine-commodore-amiga-ocs --all-targets -- -D warnings` — clean.

**Consequence:** the boot-blocking-bug-prone seam is gone. Adding a new chip arm is now "write a `dispatch_*` returning `BusResponse`, slot it into `service_cpu_bus`'s `or_else` chain". The `BusResponse::Word`/`Byte` shape is also the natural input for [Phase A.4](decisions/amiga-architecture-review.md#seam-2--disk-dma-path-straddling-four-crates) — Paula owning disk read DMA — when that lands next.

The architecture-review document moves from "Proposed (draft for review)" to "Implemented" for seams 1 and 4 with this commit; seams 2 and 3 remain open.

---

## 2026-04-28 — Boot-invariant test suites land for all four anchors

**Type:** milestone (Phase A.2 of [`docs/plans/2026-04-28-october-runup-plan.md`](../docs/plans/2026-04-28-october-runup-plan.md))
**Trigger:** Each anchor family's known-good waypoints lived in scattered diagnostic tests that ran with `#[ignore]`, machine-layer unit tests that depended on real ROMs, or one-off scripts. Phase A.2 promotes them into a single named regression gate per family so future refactors land on a green bar instead of a hand-rolled diagnostic.
**Result:** new `tests/boot_invariants.rs` in each of the four anchor runtime crates. Each file follows the same pattern: a hermetic block that runs on every `cargo test --workspace`, and a `#[ignore]`'d ROM-backed block that resolves assets from `~/.emu198x/`.

| Anchor | Hermetic | ROM-backed |
|---|---|---|
| Amiga (`runtime-commodore-amiga`) | RAM presets construct, runtime ticks past first frame, snapshot fixed point, RAM defaults stable | Kickstart 1.3 → insert-disk; Workbench 1.3 → desktop |
| Spectrum 48K (`runtime-sinclair-zx-spectrum`) | Dummy ROM constructs, runtime advances, snapshot fixed point | Real 48K ROM runs 30 frames |
| C64 (`runtime-commodore-c64`) | Dummy ROMs construct, runtime advances, snapshot fixed point | Real KERNAL → `READY.` (screen-RAM scan) |
| NES (`runtime-nintendo-nes`) | Minimal iNES loads, runs one frame, snapshot fixed point | nestest.nes loads + runs |

**Workspace effect:** `cargo test --workspace --test boot_invariants` now runs **13 hermetic invariants** across the four anchors, plus **5 ROM-backed waypoints** that activate when local fixtures are present.

**Verification:**
- `cargo test --workspace --test boot_invariants` — 4 suites, 13 passed / 0 failed / 5 ignored.
- `cargo clippy -p runtime-* --tests -- -D warnings` — clean across all four runtimes.

**Consequence:** Phase A.3 and A.4 (the Amiga seam refactors) now have a regression net. Any change to `service_cpu_bus`, the disk DMA path, the byte-lane conventions, or the chip stack will fail loudly here before it can break Workbench boot. Future bugs that get fixed get one more test added to the relevant anchor's `boot_invariants.rs`; the file is the canonical promotion target.

The plan's per-anchor waypoint list has more entries than this initial commit covers (Manic Miner, Bruce Lee, SMB, MMC1 Zelda etc.). Those add over time as their dependencies land — for now the file shape is in place and the cheapest hermetic invariants are wired.

---

## 2026-04-28 — Amiga snapshots — postcard round-trip across the full chip stack

**Type:** milestone (Phase A.1 of [`docs/plans/2026-04-28-october-runup-plan.md`](../docs/plans/2026-04-28-october-runup-plan.md))
**Trigger:** The Amiga was the only anchor family without snapshot support — explicitly called out in the README's "Notably not claimed yet" list and breaking the [`save-state-format.md`](decisions/save-state-format.md) rule "derive on everything from day one". Phase A.1 closed the gap.
**Result:** `Serialize` / `Deserialize` derived across every Amiga chip and the machine + runtime layers, with a versioned postcard envelope and a hermetic round-trip test:

- **Chip stack (8 crates, ~70 derive sites):**
  - `mos-cia-8520`, `commodore-gary`, `commodore-amiga-autoconfig`, `peripheral-commodore-amiga-keyboard`: flat structs, single derive each.
  - `peripheral-commodore-amiga-floppy`: `#[serde(skip)]` on the trait-object disk plus a manual `Clone` impl matching the skip semantics. Disk media is re-mounted by the runtime envelope on restore.
  - `commodore-paula-8364`: 9 types (IntSource, AudioField, PaulaChannel, ChannelControl, AudioControls, AudioOutputEvent, AudioChannel, AudioChannelSnapshot, Paula8364).
  - `commodore-agnus-ocs`: 13 types (Agnus + Copper + Blitter + slot/state enums); no skips, no big-array.
  - `commodore-denise-ocs`: 10 types; the 256-entry `palette_24` needed `serde-big-array`.
  - `motorola-68000`: 37 types across `cpu`, `alu`, `bus`, `microcode`, `mmu`, `model`, `registers`, `addressing`, `fpu`. No skips needed; modern serde supports `[T; 32]` via const generics.
- **Machine layer (`machine-commodore-amiga-ocs`):** derived on `Memory`, `RomRegion`, `Copper`, `Denise`, `DmaClaim`, `RamConfig`, `JoystickState`, `DiskDmaRuntime`. The RTC's `host_reference: SystemTime` is `#[serde(skip, default = "default_host_reference")]` — re-anchored to `SystemTime::now()` on restore. New `pub struct AmigaOcsSnapshot` aggregates chip + machine state; new `snapshot_state()` / `restore_snapshot_state()` methods on `AmigaOcs`. Diagnostic logs (`debug_*` fields) are explicitly **not** snapshotted — they are observability, not state, and clear on restore.
- **Runtime envelope (`runtime-commodore-amiga`):** `SnapshotEnvelopeV1 { version, model, ram_config, time, machine, floppy0_bytes, frame_count, ... }` replaces the previous `UnsupportedOperation` returns. Restore validates version + model and re-mounts DF0 from the captured ADF bytes.

**Verification:**
- `cargo test -p runtime-commodore-amiga --test snapshot_roundtrip` — 4 tests pass: snapshot-restore-snapshot is a fixed point; restored runtime + 8000 ticks matches original + 8000 ticks bit-for-bit; wrong model rejected; garbage bytes rejected.
- `cargo test --workspace --lib` — green; no regressions.
- `cargo clippy --workspace --lib --tests -- -D warnings` — clean.

**Consequence:** the Amiga is now first-class for snapshots alongside Spectrum, C64, NES, and Game Boy. The READE disclaimer is gone; the [`commodore-amiga.md`](systems/commodore-amiga.md) overview lists the round-trip test as `Validated`. With snapshots in place, Phase A.2 (boot-invariant test suites) can use them as one of the regression vectors. Phase B.4 (native verifier UI snapshot save/load) is now unblocked.

**Files touched:** 11 chip + machine + runtime crates, 1 new test file, README, wiki overview. Single milestone commit per the plan's exit criteria.

---

## 2026-04-26 to 2026-04-28 — Dragon 32 family stood up (Codex-owned summary)

**Type:** ingest (summary; Codex owns detail)
**Trigger:** With the chip-only Amiga KS 1.3 boot resolved and the four-anchor stack stable, the project added a sixth implementation family: Dragon 32. The line of work is owned by Codex while in flight; this entry summarises from outside so the wiki at least records the family's existence.
**Result:** Dragon 32 now reaches BASIC, accepts CAS tape media (BASIC + machine-code paths via `CLOAD`/`CLOADM`), mounts ROM/DGN cartridges, restores PC-Dragon PAK snapshots as machine state, plays PIA-driven audio, accepts joystick input (validated with Frogger), and compares smoke-screenshots against patched XRoar with 11/12 exact matches across a 12-title application set. Crates landed: `motorola-6809`, `motorola-pia-6821`, `motorola-sam-6883`, `motorola-vdg-6847`, `format-dragon-cas`, `format-dragon-pak`, `machine-dragon-32`, `runtime-dragon`, `emu198x-dragon`, `emu198x-script-dragon`. The MC6809 instruction core is built across roughly 14 commits (`690227e` through `327122f`); the cassette / video / audio / joystick / cartridge / PAK paths follow.
**Why this entry exists.** Until Codex completes the Dragon line, the wiki records the family's existence and the high-level shape rather than duplicating Codex's detailed commit work. When Dragon stabilises, this entry should be replaced with proper milestone summaries per chip and per port.
**Status at end of this period:** Dragon is the sixth supported family. It is not in the October launch scope but shapes the 6809 family for Wave 3 (Dragon 64, CoCo, Vectrex). Codex's current focus is PC-Dragon PAK XRoar reference comparison precision.

---

## 2026-04-26 — Current-system verification gate

**Type:** infrastructure
**Trigger:** With six families implemented (Spectrum, C64, NES, Amiga, Game Boy, Dragon), running each family's regression suite by hand had become impractical and the failure modes had drifted apart across `cargo test`, headless smoke runs, ROM-backed integration tests, Blargg / mooneye-gb assertions, and XRoar comparison runs.
**Result:** `scripts/verify-current-systems.sh` lands as a single entry-point that runs both in-repository unit/integration checks and local ROM/media smoke checks across all six families, with `--unit-only` and `--local-only` modes for tight iteration. Output is a per-system JSONL report under `target/current-system-verification/`. Missing local ROMs are reported as `skipped`, not `failed`, so the script runs cleanly without environment-specific assets.
**Coverage:** Spectrum 48K (ROM + Manic Miner + Jet Set Willy); C64 (KERNAL + tape + 1541 disks); NES (Blargg APU `rom_singles` + nestest); Amiga (Kickstart 1.3 + Workbench 1.3); Game Boy (Blargg + mooneye-gb); Dragon (CAS + machine-code CAS + audio + joystick + optional XRoar reference). Configurable via the `EMU198X_*` environment variables documented in the script's usage block.
**Consequence:** the project now has one command that says "are the current-system claims still true?", with a machine-readable answer ready for CI integration and PR-comment automation.

---

## 2026-04-25 — NES gains first-pass MMC5 support

**Type:** feature
**Trigger:** After VRC2a and Action 53 landed, the only valid ROMs
still failing the local NES smoke matrix were mapper 5 / MMC5 rows.
**Result:** `format-nintendo-nes-ines` now supports mapper 5 (MMC5)
well enough for ordinary memory execution: PRG modes 0-3, CHR modes
0-3, PRG RAM write-protection, internal ExRAM, mapper-owned nametable
mapping with fill mode, and `$5205/$5206` multiplication. Expansion
audio and scanline IRQ precision are explicitly still pending. The
mapper unit suite is now 86 tests. The 300-frame local smoke matrix
now runs every valid local `.nes` file: 627/629 entries pass, and the
only remaining errors are two invalid-header `LINUSMUS.NES` duplicates.

---

## 2026-04-25 — NES MMC5 audio and scanline IRQs land

**Type:** feature
**Trigger:** The first MMC5 slice made valid mapper-5 ROMs boot, but
left the two main hardware behaviours explicitly unfinished: expansion
audio and MMC5's PPU-read-pattern scanline IRQ.
**Result:** The mapper boundary now exposes narrow default hooks for
CPU-cycle mapper ticking, expansion-audio sampling, side-effecting CPU
reads, and PPU read observation. MMC5 uses those hooks for two
pulse-style expansion channels, raw PCM write/read mode with IRQ
acknowledge behavior, `$5204` scanline IRQ status acknowledge, and
scanline detection from three matching nametable reads followed by the
next PPU read. The mapper unit suite is now 90 tests.

---

## 2026-04-25 — NES adds VRC2a and Action 53 mappers

**Type:** feature
**Trigger:** The local NES smoke matrix had narrowed the easy
compatibility failures to unsupported mapper 22 and mapper 28 rows,
leaving MMC5 as the only large mapper family in the local failure set.
**Result:** `format-nintendo-nes-ines` now supports mapper 22
(Konami VRC2a) and mapper 28 (Action 53). VRC2a covers two switchable
8 KiB PRG banks, fixed final 16 KiB PRG, 1 KiB CHR banking, H/V
mirroring, and the VRC2 `$6000-$6FFF` latch behavior. Action 53
covers register-selected CHR RAM, inner/outer PRG banking in 16/32 KiB
modes, and switchable H/V/1-screen mirroring. The mapper unit suite is
now 79 tests. The 300-frame local smoke matrix scanned 629 `.nes`
files: 619 ran successfully, with mapper 22 at 2/2, mapper 28 at 4/4,
and the remaining expected failures limited to unsupported MMC5 rows
plus two invalid-header files.

---

## 2026-04-25 — NES smoke matrix, snapshots, DMC DMA, and mapper 11/68

**Type:** feature
**Trigger:** The NES path had enough mapper breadth that the next
compatibility work needed structured local ROM feedback instead of
single-ROM guessing.
**Result:** `format-nintendo-nes-ines` now supports mapper 11
(Color Dreams) and mapper 68 (Sunsoft-4, including CHR-ROM nametable
reads for `After Burner`). Mapper state is serializable through an
explicit snapshot enum, the NES runtime now exports/imports version-1
snapshots, DMC sample fetches steal a CPU cycle, and
`emu198x-script-nes --smoke-root` emits a JSON compatibility matrix.
The local one-frame matrix scanned 629 `.nes` files: 613 reached a
frame, with remaining expected failures in unsupported mapper 5/22/28
or invalid headers.

---

## 2026-04-25 — NES gains UxROM and MMC1 mapper support

**Type:** feature
**Trigger:** The NES native/headless paths needed the first real
bank-switching mappers to move beyond NROM-only software.
**Result:** `format-nintendo-nes-ines` now parses and instantiates
Mapper 2 (UxROM) and Mapper 1 (MMC1) alongside NROM. UxROM covers
switchable low 16 KiB PRG plus fixed high PRG and CHR RAM; MMC1 covers
serial-register writes, PRG 16/32 KiB modes, CHR 4/8 KiB modes,
dynamic mirroring, PRG RAM, and CHR RAM. NES runtime and native shell
paths inherit support through the existing boxed mapper boundary.

---

## 2026-04-24 — Game Boy gains battery-save sidecars

**Type:** feature
**Trigger:** The Game Boy native window is now usable enough that
cartridge progress should survive emulator sessions.
**Result:** `GameBoyRuntime` can import/export cartridge external RAM
and preserves it across runtime reset. `emu198x-game-boy` and
`emu198x-script-game-boy` now load/write `.sav` sidecars for
battery-backed RAM by default, with explicit `--battery-save PATH` and
`--no-battery-save` controls.

---

## 2026-04-24 — Shared native video gains first filter presets

**Type:** feature
**Trigger:** Once every current native verifier shell used the shared
`wgpu` presenter, CRT/LCD presentation could be implemented once instead
of per emulator.
**Result:** `emu198x-native-video` now exposes `raw`, `lcd`, and `crt`
presentation filters. Game Boy, NES, C64, Spectrum, and Amiga native
windows accept `--video raw|lcd|crt`; raw remains the default so
debugging captures and existing golden comparisons stay exact.

---

## 2026-04-24 — Shared native video presenter starts on Game Boy

**Type:** architecture
**Trigger:** CRT/LCD filters need a shared GPU presentation surface
rather than one-off per-emulator `pixels` blitters.
**Result:** Added `emu198x-native-video`, a shared `wgpu` presenter
with RGBA/indexed-frame upload, nearest-neighbour sampling, centred
integer scaling, and reusable presentation settings. `emu198x-game-boy`
now renders through that presenter, establishing the migration seam for
future LCD filters and for moving NES, C64, Spectrum, and Amiga onto the
same native video path.

---

## 2026-04-24 — Shared native video presenter reaches all current windows

**Type:** architecture
**Trigger:** The Game Boy `wgpu` presenter smoke test worked, so the
remaining native verifier windows no longer needed separate `pixels`
blitters.
**Result:** `emu198x-spectrum`, `emu198x-c64`, `emu198x-nes`, and
`emu198x-amiga` now render through `emu198x-native-video`, matching
Game Boy. The shared presenter handles both indexed and RGBA runtime
frames, centred integer scaling, and host resize presentation for all
current native windows.

---

## 2026-04-24 — Shared host gamepad input reaches native shells

**Type:** feature
**Trigger:** Keyboard-emulated joystick/controller input and physical
gamepads should feed the same machine-facing event path instead of
growing per-frontend special cases.
**Result:** `emu198x-shell` now owns a shared host-control mapper and
`gilrs`-backed physical gamepad poller that emit stable
`InputEvent::Button` events. NES and Game Boy native windows map
keyboard and gamepad controls through the same controller tables, while
the C64 native window maps physical gamepads to joystick port 2 and
adds a host-only `Page Up` toggle for arrow/space key emulation of
that same joystick path.

---

## 2026-04-24 — Amiga native shell gains port-1 joystick input

**Type:** feature
**Trigger:** The Amiga native verifier had keyboard, mouse, and live
Paula audio, but most games need joystick input on controller port 1.
**Result:** `machine-commodore-amiga-ocs` now models a digital
joystick on port 1, driving `JOY1DAT` direction bits and active-low
CIA-A `FIR1` fire. `runtime-commodore-amiga` routes
`InputEvent::Button` into that hardware path, and `emu198x-amiga`
maps physical gamepads plus an optional host-only `Page Up`
arrow/space mode to the same port-1 joystick events.

---

## 2026-04-24 — Native shells gain host-side audio controls across current systems

**Type:** feature
**Trigger:** Game Boy and NES had per-channel host controls, but the
other live native verifier shells still exposed only raw audio output.
**Result:** C64 SID, Amiga Paula, and Spectrum speaker output now have
host-side mute/gain controls surfaced through their machine/runtime
layers and native windows. C64 and Amiga use numpad shortcuts for
voice/channel toggles and gain cycling, while Spectrum 48K exposes
speaker mute/gain on the numpad. The controls are explicitly outside
emulated chip register state.

---

## 2026-04-24 — NES native shell gains APU channel controls

**Type:** feature
**Trigger:** NES now uses the shared native audio output path, but
debugging audio needs per-channel isolation without changing emulated
APU register state.
**Result:** `ricoh-apu-2a03` now exposes host-side `AudioControls`
over Pulse 1, Pulse 2, Triangle, Noise, and DMC. The controls are
kept outside `$4015` and channel length semantics, then surfaced
through `machine-nintendo-nes`, `runtime-nintendo-nes`, and
`emu198x-nes`. The native shell maps `1`-`5` to channel toggles and
`6`-`0` to channel gain cycling.

---

## 2026-04-24 — Game Boy native shell gains APU channel controls

**Type:** feature
**Trigger:** The shared host audio layer now preserves stereo and
plays Game Boy audio, but usability needs per-channel inspection
without pushing chip-specific mute/gain policy into the generic host
output path.
**Result:** `nintendo-game-boy-apu` now has serializable host-side
`AudioControls` over the four APU channels: pulse 1, pulse 2, wave,
and noise. The controls are explicitly outside ROM-visible NR50/NR51
and NR52 state. `machine-nintendo-game-boy` and
`runtime-nintendo-game-boy` expose the same controls, and
`emu198x-game-boy` maps them to `1`-`4` channel toggles, `5`-`8`
channel gain cycling, and `0` reset.

---

## 2026-04-24 — Shared native audio output reaches NES and Game Boy

**Type:** refactor
**Trigger:** After fixing host conversion to preserve stereo, the
native shells still duplicated CPAL setup and queueing logic, and the
NES/Game Boy windows still discarded runtime audio packets.
**Result:** `emu198x-shell` now owns the CPAL-backed native audio
output sink: device setup, bounded callback buffering, stream
callbacks, sample-rate conversion, and host channel conversion.
`emu198x-amiga`, `emu198x-spectrum`, and `emu198x-c64` use the shared
sink instead of frontend-local copies, while `emu198x-nes` and
`emu198x-game-boy` now play live runtime audio. Per-chip and
per-channel mute/gain remains intentionally below this layer in each
system's native mixer.

---

## 2026-04-24 — Native audio conversion preserves stereo

**Type:** refactor
**Trigger:** The Amiga native shell could play Paula audio, but the
host conversion path downmixed every machine packet to mono before
duplicating it across the output device. That would erase stereo
placement for Amiga and upcoming stereo systems.
**Result:** Added shared `emu198x-shell` host audio conversion used by
the Amiga, Spectrum, and C64 native verifier shells. It preserves
matching channel layouts, duplicates mono packets to multi-channel
host output, averages only when the host has fewer channels than the
machine packet, and silence-fills extra host channels for non-mono
sources.

---

## 2026-04-24 — Amiga native shell plays live Paula audio

**Type:** feature
**Trigger:** The Amiga runtime emitted Paula-backed audio packets, but
the native verifier shell still discarded them.
**Result:** `emu198x-amiga` now owns a CPAL output stream and drains
runtime audio packets into a bounded callback buffer, matching the
Spectrum/C64 native audio pattern. Host sample-rate/channel conversion
is covered by unit tests. Joystick input and broader software smoke
coverage remain pending.

---

## 2026-04-24 — Amiga mouse input wired end-to-end

**Type:** feature
**Trigger:** The native Amiga verifier window existed, but Workbench
was not practically usable without mouse input.
**Result:** `emu198x-amiga` now emits host mouse movement/buttons as
shared pointer events, `runtime-commodore-amiga` routes `mouse-1` to
controller port 0, and `machine-commodore-amiga-ocs` exposes the
movement through JOY0DAT plus active-low CIA/POTGOR button inputs.
Joystick input and live native audio remain pending.

---

## 2026-04-24 — Native Amiga verifier window added

**Type:** feature
**Trigger:** With Kickstart/Workbench video and Paula-backed headless
audio in place, the largest Amiga usability gap was that the only
fresh-workspace launch path was still headless capture.
**Result:** Added `emu198x-amiga`, a minimal native OCS verifier shell
over `runtime-commodore-amiga`. It supports A1000/A500-family model
selection, ROM directory or explicit firmware loading, optional DF0
`ADF` insertion, windowed `pixels`/`winit` video, basic keyboard
input, and hard reset. Mouse/joystick input and live native audio
remain the next usability work.

---

## 2026-04-24 — Amiga runtime drains Paula audio

**Type:** feature
**Trigger:** The Amiga headless path had proven Kickstart and
Workbench video, but `MachineCore` still emitted empty audio packets.
**Result:** `runtime-commodore-amiga` now samples Paula's live stereo
mix through a persistent 48 kHz phase accumulator and emits non-empty
stereo audio packets once per runtime frame. The script runner's WAV
capture path now receives real runtime audio data instead of an empty
placeholder. Native Amiga UI remains the next usability step.

---

## 2026-04-24 — Native verifier windows added for Game Boy and NES

**Type:** feature
**Trigger:** After adding the Game Boy headless runner, the next
usability gap was that NES and Game Boy still lacked windowed native
launch paths.
**Result:** Added `emu198x-game-boy` and `emu198x-nes`. Both are
minimal native verifier shells over the existing runtimes: ROM load,
windowed video through `pixels`/`winit`, controller/joypad keyboard
mapping, hard reset, and integer scaling. Live native audio is
deliberately left for a later pass so Amiga Paula runtime audio can
remain the next explicit audio task.

---

## 2026-04-24 — Game Boy headless runner added for current-system usability

**Type:** feature
**Trigger:** The current systems are close enough that the next product
pressure is practical launchability, not only per-core accuracy.
**Result:** Added `emu198x-script-game-boy`, giving the Game Boy
runtime the same headless runner shape as the other current families.
It accepts `--rom`, `--media`, `--model`, `--frames`, shared JSON
scripts, screenshots, audio capture, and snapshot load/save. The
current-system usability matrix now records the launch path and next
usability step for Spectrum, C64, NES, Amiga, and Game Boy.

Verification:

- `cargo fmt --all --check`
- `cargo test -p emu198x-script-game-boy`

---

## 2026-04-24 — Current non-Game-Boy system docs refreshed

**Type:** docs
**Trigger:** After the Game Boy documentation refresh, the other
system pages still mixed current fresh-workspace status with older
archive-era notes.
**Result:** Updated the current-system documentation for C64, NES,
Amiga, Spectrum, and the wiki index.

Highlights:

- `wiki/systems/commodore-c64.md` now treats the C64 as a live
  fresh-workspace system, including runtime snapshots, TAP media,
  PRG/BAS/T64 import, and the optional ROM-backed 1541/`D64`
  drive-8 path.
- `wiki/systems/nintendo-nes.md` now has a dated current-status
  summary matching the NROM-only `MachineCore` runtime, `nestest`
  proof, `Super Mario Bros.` rendering, and remaining snapshot /
  DMC-DMA gaps.
- `wiki/systems/commodore-amiga.md` now reflects the OCS PAL
  runtime catalogue: real A1000 bootstrap/WOM support, A500-family
  RAM profiles, Workbench 1.3 desktop golden coverage, and the
  current empty runtime-audio placeholder.
- `wiki/systems/spectrum/overview.md` now describes the 11-model
  catalogue, 7 machine-crate shape, generic runtimes for non-48K
  variants, and the current `emu198x-spectrum` runner.

---

## 2026-04-24 — Game Boy Phase 2 gate green and docs refreshed

**Type:** milestone
**Trigger:** The Game Boy runtime now passes the local ignored
verification harness rather than merely exposing the host boundary.
**Result:** Phase 2 is green for the current DMG-class scope:
Blargg `cpu_instrs`, `instr_timing`, `mem_timing` v1/v2,
`dmg-acid2`, and the broad mooneye-gb sweep all pass locally. The
mooneye broad sweep reports 103 passing ROMs and zero failures,
timeouts, or load errors across `acceptance`,
`emulator-only/mbc1`, `emulator-only/mbc2`, and
`emulator-only/mbc5`.

Docs refreshed:

- `wiki/systems/nintendo-game-boy/overview.md` now reflects current
  runtime scope, skipped-boot profiles, MBC2 support, timer reload
  accuracy, OAM DMA status, and Phase 2 verification status.
- `wiki/systems/nintendo-game-boy/timing.md` is no longer marked as
  a stub and calls out the remaining OAM DMA bus-blocking gap.
- `wiki/chips/sharp-lr35902.md` now treats the pin interface and
  state-machine shape as implemented, with system-level Blargg /
  mooneye coverage.
- `wiki/index.md` now describes the Game Boy page as current DMG
  runtime documentation rather than a planned port.

The remaining major Game Boy-family work is CGB, boot-ROM execution,
full OAM-DMA non-HRAM bus blocking, persistent battery saves,
link cable, and long-tail cartridge hardware.

---

## 2026-04-23 — Game Boy Phase 1 complete: runtime crate landed, family is on the host boundary

**Type:** milestone
**Trigger:** With `machine-nintendo-game-boy` orchestrating SM83 +
PPU + APU + timer + cartridge per m-cycle, the only piece left in
Phase 1 was the runtime — the family's seat at the
`emu198x-shell` boundary alongside Spectrum, C64, NES, and Amiga.
**Result:** `runtime-nintendo-game-boy` lands as the ninth and
final Phase-1 crate. The bridge is the same shape as the other
families' runtimes: `GameBoyRuntime` carries a `MachineProfile`,
optionally a loaded `GameBoy`, kept-cartridge bytes for reset
rebuilds, a `MachineTime` cursor, and an audio drain buffer.

Highlights:

- `Family::GameBoy` added to `emu198x-shell` so the shared
  query surface knows the family name (`game-boy`).
- `Model::Dmg` populates the catalogue today; the catalogue
  already accepts a future `Cgb` model without restructuring.
- `MachineCore::run_until` drives `GameBoy::run_frame` until the
  requested time, pushes `Indexed8` frames against the four-shade
  `DMG_GREYSCALE_RGBA` palette, and drains the APU's interleaved
  stereo at 48 kHz once per frame.
- `MachineCore::load_media` accepts a `Cartridge` image at slot
  `cartridge`, parses it through `format-nintendo-game-boy-
  cartridge`, and rebuilds the machine. Unknown slot or wrong
  media kind both surface real `MachineError` variants.
- `MachineCore::snapshot` / `restore` use a versioned, profile-
  id-checked postcard envelope so a future CGB snapshot can't
  silently deserialise into a DMG runtime.
- Joypad input maps `a/b/select/start/up/down/left/right`
  (case-insensitive) to `JoypadButton`, accepted from either
  `InputEvent::Key` or `InputEvent::Button`.

9 unit tests cover blank construction, valid load + invalid
slot/kind, run-with-no-cartridge → `WaitingForInput`, run-with-
cartridge → `ReachedTarget` and time advance, joypad input
round-trip via snapshot, snapshot/restore preserving state, and
profile-id mismatch rejection. Workspace tests stay green.

The deferred items from earlier steps remain deferred: boot ROM,
OAM DMA bus blocking, per-PPU-mode VRAM/OAM gating, 1-m-cycle
TIMA reload delay, MBC2. Phase 2 (Blargg + mooneye + dmg-acid2)
will pull on the ones it needs.

`wiki/systems/nintendo-game-boy/overview.md` updated to mark step
9 done with the runtime's surface called out.

---

## 2026-04-23 — sharp-lr35902 ported and externally validated to 49,600 SM83 tests

**Type:** milestone
**Trigger:** With the sharp-lr35902 crate's full opcode table,
interrupt dispatch, HALT bug, and EI delay all in place (92 unit
tests), the natural next step was external validation against a
known canonical CPU test corpus.
**Result:** the
[Adam Tennant SM83 single-step corpus](https://github.com/adtennant/sm83-test-data)
at `~/Projects/Emu198x-Unclean/GameboyCPUTests/v2/` ran clean on
first attempt — 49,600 / 49,600 tests pass (240 top-level opcodes ×
100 tests + 25,600 CB sub-table permutations). The corpus omits
HALT ($76), STOP ($10), DI ($F3), EI ($FB), and 11 illegal
opcodes; all of those gaps are filled by the crate's unit tests.

The harness lives at `crates/sharp-lr35902/tests/single_step_tests.rs`
and is `#[ignore]`'d (preservation-grade test data isn't checked
into the repo). Run with:

```sh
cargo test -p sharp-lr35902 --test single_step_tests run_all \
  -- --ignored --nocapture
```

The test file's header comment documents the pipelined-model
adapter: the corpus assumes a decode-execute-prefetch loop while
our CPU is pin-level-pipelined the other way, so a one-line
`pc += 1` synthesises the prefetch's PC-increment that the corpus
expects in its `final.pc`.

**What this validates beyond the unit tests:** every documented
opcode gets 100 randomised tests covering exotic register / flag
combinations, AND each test verifies the per-m-cycle bus activity
(reads, writes, internal cycles) — not just the final register
state. So this validates not only correctness but also the
m-cycle-by-m-cycle pin contract from
[`cpu-bus-interface.md`](decisions/cpu-bus-interface.md). That's a
much stronger guarantee than Blargg cpu_instrs would have given
(Blargg only validates final register state via a serial-output
trick), and it doesn't need a cartridge fixture to run.

`wiki/chips/sharp-lr35902.md` updated with the test status and a
"future Blargg coverage" section deferred to once the Game Boy
machine layer exists.

---

## 2026-04-22 — Game Boy port Phase 0 docs landed

**Type:** ingest (planning)
**Trigger:** The next family to port (Nintendo Game Boy, lifting
from `~/Projects/Emu198x-Zig/`) had no wiki presence yet. Phase 0
of the port is "write down the shape and the binding decisions so
crates land against a known target".
**Result:** four pages added:

1. `wiki/decisions/sm83-abstraction-level.md` — m-cycle chosen over
   T-cycle. Generalises [half-cycle signals](decisions/half-cycle-signals.md)
   into the rule "match the finest-grained observation any
   component makes of the CPU". Lists every SM83 observer (bus,
   PPU, APU, DMA, timer, interrupts) and confirms none go below
   m-cycle. Pin-level rule still applies at m-cycle grain.
2. `wiki/systems/nintendo-game-boy/overview.md` — family home.
   Phased crate plan (CPU → common → PPU → APU → timer → MBC →
   format → machine → runtime) and the acceptance bar (Blargg
   `cpu_instrs`, `instr_timing`, `mem_timing`; mooneye-gb
   acceptance; `dmg-acid2`).
3. `wiki/chips/sharp-lr35902.md` — chip stub. Instruction-set
   deltas from Z80 / 8080, planned pin interface, m-cycle state
   machine shape lifted from `sm83.zig`, interrupt model, test
   coverage plan.
4. `wiki/systems/nintendo-game-boy/timing.md` — master clock
   (4.194304 MHz), m-cycle derivation, PPU mode splits,
   timer / frame-sequencer rates, OAM DMA timing.

`wiki/index.md` updated with links in Chips, Systems, and
Decisions sections. No code written; `sharp-lr35902` crate and
the Game Boy machine / runtime remain unimplemented.

---

## 2026-04-22 — Spectrum family expansion: 11 variants on the workspace floor

**Type:** milestone (multi-port wave)
**Trigger:** Spectrum 48K had been the only Spectrum on the fresh-workspace floor, yet the [product roadmap](decisions/product-roadmap.md) treats "Spectrum" as a family of eleven variants. The October curriculum needs 48K, 128K, +2/+2A/+3, and Pentagon at minimum. The day before Game Boy Phase 0, the family was finished in a single push.
**Result:** the workspace gained the full Spectrum line in one day:
1. **128K stack:** `sinclair-ula-7k010e` (17.7 MHz crystal, phase-1 contention) and `gi-ay-3-8912` (PSG, /8 prescaler, Bresenham downsampling).
2. **+2A/+3 stack:** `amstrad-ula-40077` (gate array, MREQ-only contention, no floating bus), `nec-upd765a` (FDC), `format-amstrad-dsk` (DSK + EDSK loader).
3. **Snapshot format:** `format-sinclair-zx-spectrum-z80` for `.z80` v1/v2/v3 and `.sna`.
4. **Eastern Bloc + Timex variants:** `pentagon-ula`, `scorpion-ula`, `timex-scld`, `beta-disk-interface` (Russian disk interface).
5. **Variant machines:** `machine-pentagon-128`, `machine-scorpion-zs256`, `machine-timex-tc2048`, `machine-timex-ts2068`.
6. **128K and +2/+2A/+3 machines:** `machine-sinclair-zx-spectrum-128k`, `machine-sinclair-zx-spectrum-plus`.
7. **Generic runtime:** `runtime-sinclair-zx-spectrum` wraps every variant in a `MachineCore` shape; the `SpectrumDriver` trait (designed 2026-04-08) is finally implemented across all seven machines.
8. **Family decision:** [`wiki/decisions/within-family-layering.md`](decisions/within-family-layering.md) — five-piece structure (common / chip / format / machine / runtime) the family follows. Future families inherit this template; Game Boy validates it the next day.

**Verification:** all 11 Spectrum variants boot in headless and native shells. ZEXDOC / ZEXALL / FUSE stay green on the shared Z80 core.
**Consequence:** Spectrum is the first family to fully realise the within-family-layering pattern. The shape is now copy-pasteable for any Z80, 6502, 68000, or 6809 family — and within hours, was reused for the Game Boy port.

---

## 2026-04-21 — Amiga archive-port wave + Workbench MFM investigation

**Type:** milestone (multi-port wave + investigation)
**Trigger:** The chip-only KS 1.3 restart (M0–M9) had rebuilt the Amiga from a clean spine. Before pursuing Workbench boot, the broken-out chip crates (`commodore-agnus-ocs-archive`, `commodore-paula-8364-archive`, `commodore-denise-ocs-archive`, etc.) needed to be ported into the live machine using the codified [archive-port methodology](decisions/archive-port-methodology.md).
**Result:** in a single day, the live Amiga absorbed seven peripheral and chip archives:
1. **`commodore-agnus-ocs` + Blitter** — bits module, machine-facing register writers, full DMA arbitration ported.
2. **`commodore-paula-8364`** — INTENA/INTREQ/ADKCON, audio register storage, audio DMA + AUDx IRQs, disk register storage, disk-completion + MFM-sync IRQ paths, Paula serial UART (SERDAT/SERPER/SERDATR + TBE/RBF), POTGO + POTxDAT + POTGOR analog inputs.
3. **`commodore-denise-ocs`** — BPLCON1/2 + colour palette absorbed, pixel pipeline delegated, LACE + sprite DMA wired (Phases 2a–2c).
4. **`peripheral-commodore-amiga-floppy`** — 18 characterisation tests; DF0 drive wired into the machine.
5. **`peripheral-commodore-amiga-keyboard`** — 7 characterisation tests; controller wired.
6. **`commodore-gary`** — 7 tests; address decoder wired.
7. **`runtime-commodore-amiga` + `emu198x-script-amiga`** — retargeted at `machine-commodore-amiga-ocs`; `boot.*` queries restored.

The seven `*-archive` crates retire in the same wave. Cross-cutting boot integration tests landed (task #180). Configurable chip + slow RAM sizes shipped as a 3-step plan: chip RAM size → Zorro-II autoconfig fast RAM → runtime presets.

**Workbench investigation.** With the chip stack live, the focus moved to Workbench 1.3 boot. The day's golden-image matrix at PAL-cropped 752×572 against FS-UAE showed the chain reaching `trackdisk` but stalling at MFM compatibility. Specific fixes landed:
- `DMACONR` byte-read upper-byte semantics on even addresses (regression from earlier OCS work).
- Copper MOVEs route through the machine-wide custom-register dispatch (so the 2-CCK pipeline applies).
- Denise: trailing DDF block fetched, framebuffer origin follows the Standard viewport, `COLOR00` paints the full border.
- Disk DMA transfers complete so `trackdisk`'s DSKBLK fires.
- MFM encoder boundary clock bits + post-track gap-fill (later partially reverted; KS trackdisk wants the wrap).
- Several diagnostic examples (`bootblock_writers`, `validation_trace`, `every_blit_in_bootblock`) chase the residual silent failure.

**Status at end of period:** the Amiga reaches Kickstart insert-disk reliably. Workbench boot is closer but still failing on what the diagnostics describe as "chained QBlits never run" — a residual MFM/blitter issue, picked up later as Phase A.4 (architecture-review seam 2: Paula owns disk read DMA) in [`docs/plans/2026-04-28-october-runup-plan.md`](../../docs/plans/2026-04-28-october-runup-plan.md).

---

## 2026-04-20 — Archive-port methodology codified

**Type:** decision (process)
**Trigger:** The chip-only KS 1.3 restart wave (M0–M9) was completing, and the next wave was a multi-archive port across `commodore-paula-8364-archive`, `commodore-agnus-ocs-archive`, `commodore-denise-ocs-archive`, `peripheral-commodore-amiga-floppy-archive`, and several others. Without a documented process, the same shape would be reinvented per chip.
**Result:** [`wiki/decisions/archive-port-methodology.md`](decisions/archive-port-methodology.md) lands. Three phases per archive crate:
1. **Phase 1 — characterise.** Read the archive crate. Write characterisation tests against the *archive's* current behaviour (gap list, register-by-register coverage). The tests live in the *live* crate from the start.
2. **Phase 2 — port-with-tests.** Re-author the archive's API in the live crate against the post-rewrite rules (pin-level CPU bus, no Bus trait, named bits, typed audio fields, private state). The Phase 1 tests become Phase 2's regression net.
3. **Phase 3 — integrate.** Wire the live crate into the machine. Retire the `*-archive` crate in the same commit so the workspace never has both.

The methodology is the parent of the 2026-04-21 archive cleanup wave that retired seven archive crates in a single day.
**Consequence:** the methodology turned multi-week archive-port projects into single-day shipping events. Wave 2 systems (BBC Micro, Atari 2600) and the remaining Amiga work (Blitter scheduling improvements, Akiko, AGA chips) inherit the same shape.

---

## 2026-04-19 to 2026-04-20 — Amiga restart M0–M9 (chip-only KS 1.3 boot resolved)

**Type:** milestone (extended investigation + rebuild)
**Trigger:** The fresh-workspace Amiga was reaching the Kickstart insert-disk screen but had a long-running corruption issue: ExecBase's free-list was being mangled by something. Diagnostics (`bootblock_writers`, `freetwice_trace`, `cop1lc_write_log`, CPU `watch_range` instrumentation) traced the corruption to the copper writing into ExecBase as if it were a copper list. The root cause was eventually confirmed against WinUAE: COP2LC was being seeded with `GfxBase->LOFlist`, which still pointed to chip-RAM ExecBase between the two `LoadView` calls KS 1.3 makes during boot — and *the copper happily executed it*.
**Result (the fix):** copper MOVE to a register address `< $80` while CDANG (COPCON bit 1) is clear must halt the copper, per WinUAE `custom.cpp::test_copper_dangerous` and vAmiga `Copper.cpp::isIllegalAddress`. KS 1.3 leaves CDANG = 0 by default, so the protection rescues real chip-only A500s from this exact toxicity. Commit `9270a9b` adds the halt; the chip-only boot now reaches WAITBLIT.

**Restart M0–M9.** With the root cause understood, the Amiga was rebuilt from the chip up under the [archive-port methodology](decisions/archive-port-methodology.md) on a milestone-by-milestone schedule:
- **M0** — CPU + ROM + OVL: bare minimum that runs the reset vector.
- **M1** — chip RAM + CPU bus integration.
- **M2** — custom-register storage.
- **M3** — OVL clear via CIA-A.
- **M4** — chip-RAM aliasing for the size probe.
- **M5** — bootstrap ExecBase placement.
- **M6** — beam counter + VBL interrupt.
- **M7** — chipset read fidelity (VPOS / VHPOS + CIA-A inputs).
- **M8** — CIA-A timers + ICR + CIA→Paula IRQ.
- **M9** — CIA-B basics + 8520 one-shot auto-start on TxHI write.
- **+ copper CDANG halt** — the fix described above.

**Consequence:** the Amiga has a clean spine from CPU through chipset that passes Phase 0/3/5/9 boot-invariant checks against `Emu198x-Older` golden frames. Workbench boot becomes the next focus (see 2026-04-21 entry).
**Wiki updates:** [`amiga-chip-only-boot-failure.md`](decisions/amiga-chip-only-boot-failure.md) marked **resolved**; [`amiga-restart-plan.md`](decisions/amiga-restart-plan.md) tracks M0–M9 status.

---

## 2026-04-19 — Amiga architecture review identifies five seams to tighten

**Type:** decision (proposed)
**Trigger:** After two weeks of repeated boot-blocking bugs in the Amiga (CIA double-read, byte-lane conventions, MFM compatibility, free-list corruption), the project needed an honest review of *where* the friction was concentrated. The architectural spine ([CPU bus interface](decisions/cpu-bus-interface.md), [No Bus trait](decisions/no-bus-trait.md), [System-specific run loops](decisions/system-specific-run-loops.md)) had proven correct on Spectrum / C64 / NES; the question was whether the Amiga implementation needed re-foundation or only seam-level tightening.
**Result:** [`wiki/decisions/amiga-architecture-review.md`](decisions/amiga-architecture-review.md) lands, status **Proposed (draft for review)**. The verdict: the spine stays; five implementation seams need work:
1. **`service_cpu_bus`** in `machine-commodore-amiga` — restructure into a `BusTransaction` / `BusResponse` shape; the function is currently a 3000-line traffic cop with four byte-lane conventions in flight.
2. **Disk DMA path** straddles four crates — Paula should own the read state machine end-to-end (WORDSYNC, sync-stripping, DSKBYTR, DSKBLK IRQ).
3. **Custom register byte-write merge latch** is hand-maintained machine-side — chip-owned `read_register_word` instead.
4. **Byte-lane response conventions** — four conventions in flight; need a single canonical `BusResponse::{Byte, Word, Float}` shape.
5. **No standing per-system boot-invariant suite** — diagnostic examples are append-only, never promoted to regressions; add `tests/boot_invariants.rs` per anchor runtime.

The order of work is sized for leverage: seam 2 first (actual boot blocker), seams 1+4 together (touch the same file), seam 5 (cheapest defensive value), seam 3 last (lowest urgency).
**Consequence:** the review is the source document for several Phase A items in [`docs/plans/2026-04-28-october-runup-plan.md`](../../docs/plans/2026-04-28-october-runup-plan.md). It also confirms the spine holds — the Amiga is the first system to stress multi-master bus arbitration and DMA-driven I/O, and the answer is "the architecture is correct; the implementation has growing pains".

---

## 2026-04-18 — Project relicensed to GPL-2.0-or-later

**Type:** decision (legal)
**Trigger:** The Amiga port consumes structural and behavioural information from vAmiga (GPL-3.0-or-later); the NES port from Mesen2 (GPL-2.0-or-later). The original MIT licence created a one-way licence-incompatibility risk if any port crossed from "idea-level" into "derivative work" territory.
**Result:** workspace relicensed to **GPL-2.0-or-later** via `9254c3e`. `LICENSE`, every member crate's `Cargo.toml`, and the workspace `[workspace.package] license` field now read `GPL-2.0-or-later`. The "or-later" clause keeps GPL-3 reference material (vAmiga) consumable — GPL-2.0-or-later code can be relicensed forward.
**Verification:** `cargo metadata --format-version 1` confirms every member crate declares `GPL-2.0-or-later`. README updated.
**Consequence:** the project can now consume GPL-2 and GPL-3 reference code without licence-compatibility worry. Reverse-direction copy from this project must respect GPL terms; `wiki/decisions/archives-as-source.md` records the source provenance for every archive port.

---

## 2026-04-17 to 2026-04-18 — Chip-fix wave from the chip-only Amiga investigation

**Type:** milestone (multi-chip fixes)
**Trigger:** While debugging the chip-only A500 KS 1.3 boot, the diagnostics (`signal_watch`, `cia_a_timer_b_trace`, `microhz_handler_trace`, etc.) repeatedly fingered chip-level inaccuracies that had been good enough for Spectrum / C64 / NES but broke under the Amiga's denser bus traffic. Rather than localising the fixes to the Amiga, the chips were corrected at source so every system benefits.
**Result:** chip-level fixes landed across nine crates in two days:
- **`mos-6502`:** RDY stall halts at the right cycle boundary; reset is a real **7-cycle** sequence (was 4); IRQ/NMI sample on the **penultimate** cycle of every instruction (was last); CLI/SEI/PLP introduce the documented one-instruction interrupt-latency delay; BCD ADC/SBC flag semantics fixed against Oxyron's reference. Test fixtures regenerated for the 7-cycle reset; `reset_phase` exposed for testability.
- **`mos-cia-6526`:** SP rate fixed; alarm semantics tightened; 50/60 Hz selector exposed; SP ↔ TOD interaction corrected.
- **`mos-cia-8520`:** 8520-specific TOD halt behaviour separated from 6526; `/DSKRDY` handling brought into line with the floppy ID stream.
- **`mos-via-6522`:** ORA-alt write decoded; IER bit 7 (set/clear flag) implemented; shift register all 7 modes implemented; external CB1 driver wired.
- **`mos-sid-6581`:** noise taps corrected; ADSR rates calibrated; TEST bit semantics; envelope gate-bug; **4096-entry combined waveform ROM tables imported from reSID** for the OSC3 read.
- **`mos-vic-ii`:** unused-bit read mask; sprite fetch spread across the designated p-access cycles (was bunched); independent border flip-flops for the open-border trick.
- **`commodore-agnus-ocs`:** NTSC short/long line constants; `DMACONR` byte-read upper-byte semantics on even addresses.
- **`commodore-paula-8364`:** DSKLEN arming flip-flop; Copper HP full resolution.
- **`motorola-68000`:** several cycle-count fixes uncovered alongside the chip-only work; the Tom Harte sweep was brought back to green at 1,000,058 / 1,000,058 — see the separate 2026-04-16 entry below.

**Verification:** Tom Harte regressions stay green on Z80, 6502, 68000 (and SM83, when it lands the next week). C64 KERNAL → READY stays green. NES nestest stays green. Spectrum boot tests stay green. Amiga chip-only KS 1.3 advances to WAITBLIT.
**Consequence:** the chip stack reached its "Amiga-grade" accuracy bar — not because the Amiga is special, but because the Amiga is the first system to actually exercise these edges. Every other system inherits the fixes for free.

---

## 2026-04-16 — 68000 Harte sweep brought to green, with two invalid `ASL.b` rows quarantined

**Type:** milestone
**Trigger:** The fresh-workspace Amiga boot path had reached the real Kickstart insert-disk screen, but Workbench disk boot was still stuck far enough downstream that the `68000` core itself became a plausible upstream suspect again. Running the in-tree Tom Harte harness against the local `68000` corpus confirmed that suspicion: the first smoke pass was only `328,887 / 346,795` (`94.8%`) and the full sweep still had real failures in `MOVE`, `MOVEM`, `DBcc`, `CHK`, `LINK`, `ADDX/SUBX`, `DIVS/DIVU`, and shift groups.
**Result:** the `motorola-68000` core and harness were tightened until the full sweep went green on all runnable fixture rows:
1. fixed the Harte harness to resolve the active local corpus under `~/Projects/Emu198x-Unclean/680x0/68000/v1` instead of only the stale archive path
2. made instruction-boundary detection robust by tracking instruction starts directly, which stopped false branch/loop timeouts
3. fixed several real 68000 core bugs that Harte exposed and that are directly load-bearing for Amiga ROM/device code:
   - address-error frame IR / saved-PC selection
   - `DBcc` odd-target address-error state
   - long `MOVE` write address-error PC calculation
   - `ADDX/SUBX` predecrement long address-error undo/frame rules
   - `LINK A7,#` push semantics
   - `CHK` in-range flag handling
   - `DIVS/DIVU` overflow and divide-by-zero frame semantics
4. added a focused ignored Harte entry point for the final opcode groups so remaining failures could be iterated in seconds instead of waiting for another full multi-minute sweep
5. isolated the final `ASL.b` remainder to two exact rows, both for opcode `E502`, whose expected `D2` values mutate the upper 24 bits on a byte-sized shift; the harness now quarantines those two rows as invalid fixture data instead of warping the CPU core around impossible state

**Verification:** locally, this slice passes:
- `cargo test -p motorola-68000`
- `cargo clippy -p motorola-68000 --all-targets -- -D warnings`
- `cargo test -p motorola-68000 --test tom_harte harte_focus_remaining -- --ignored --nocapture`
- `cargo test -p motorola-68000 --test tom_harte harte_full_sweep -- --ignored --nocapture`

**Sweep result:** `1,000,058 / 1,000,058` runnable Harte rows passing (`100.00%`), with exactly `2` invalid `ASL.b` rows skipped and documented.

**Consequence:** the `68000` core is no longer an unbounded “maybe” under the Amiga boot blocker. The next Amiga pass can target the DF0 ready/select/read path and later `trackdisk.device` bring-up from a materially cleaner CPU baseline.

## 2026-04-16 — Amiga Kickstart insert-disk screen restored

**Type:** milestone
**Trigger:** The fresh-workspace Amiga baseline was booting far enough to spin DF0 and reach a live display state, but the no-disk KS1.3 path was still only proving “visible output,” not the real insert-disk hand screen. Comparison against `Emu198x-Oldest` showed the old blessed screen was four-colour, while the fresh runtime had regressed to a two-colour white frame.
**Result:** the missing bug was in the custom-register write path, not in Exec or trackdisk. CPU writes were queueing the Agnus→Denise 2-CCK pipeline, but Copper writes were bypassing it, which silently dropped Copper palette writes and left `BPLCON0`/`COLORxx` in the wrong steady state. This slice:
1. moved the custom-register pipeline queue into `machine-commodore-amiga::write_custom_reg()` so CPU and Copper writes share the same path
2. added a machine-level regression proving `write_custom_reg()` applies pipelined `BPLCON0` and palette writes after the expected 2 CCK delay
3. switched the Amiga runtime framebuffer export from the temporary crop to Denise’s standard viewport extractor
4. strengthened the ignored runtime proof from “visible output” to a real no-disk Kickstart 1.3 insert-disk screen, asserting the steady-state palette and display mode
5. compared the fresh screenshot directly against the old blessed screenshot; the images are visually identical and differ by only 32 pixels at the chosen capture frame

**Verification:** locally, this slice passes:
- `cargo fmt --all --check`
- `cargo test -p machine-commodore-amiga custom_reg_write_applies_pipelined_palette_and_bplcon0`
- `cargo test -p runtime-commodore-amiga`
- `cargo test --release -p runtime-commodore-amiga real_kickstart13_boot_reaches_insert_disk_screen -- --ignored --nocapture`
- `cargo run --release -q -p emu198x-script-amiga -- --kickstart /Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom --frames 1700 --screenshot /tmp/amiga-kick-1700-viewport.png --print-query boot.detected --print-query boot.reason --print-query amiga.agnus.bplcon0 --print-query amiga.denise.bplcon0 --print-query amiga.display.color00 --print-query amiga.display.color01 --print-query amiga.display.color02 --print-query amiga.display.color03`
- `compare -metric AE /tmp/amiga-kick-1700-viewport.png /Users/stevehill/Projects/Emu198x-Oldest/test_output/amiga/boot_kick13_a500_display.png null:`

**Consequence:** the fresh-workspace Amiga baseline now has a real Kickstart-screen proof again. The next blocker is no longer “can it draw the hand screen?” It is the later Workbench/game boot path, starting with the existing `real_workbench13_disk_bootblock_reaches_chip_ram` regression.

## 2026-04-15 — Fresh-workspace Amiga headless baseline restored

**Type:** milestone
**Trigger:** With Spectrum, C64, and NES all back on real fresh-workspace footing, the next project goal was to stop describing Amiga as archive-only and give it the same minimum modern baseline: runnable machine crates, a fresh runtime, and a headless proof that a real Kickstart ROM executes.
**Result:** the fresh workspace now has a real Amiga baseline again:
1. restored the A500 OCS chip/machine crates from `Emu198x-Older`: `motorola-68000`, `mos-cia-8520`, `commodore-gary`, `commodore-agnus-ocs`, `commodore-denise-ocs`, `commodore-paula-8364`, `format-commodore-amiga-adf`, `peripheral-commodore-amiga-floppy`, `peripheral-commodore-amiga-keyboard`, and `machine-commodore-amiga`
2. added `runtime-commodore-amiga`, a fresh `MachineCore` runtime over the A500 OCS PAL machine with Kickstart validation, RGBA framebuffer output, stereo audio output, DF0 media insertion, shared keyboard input, and a small boot/disk query surface
3. added `emu198x-script-amiga`, a headless runner that resolves Kickstart ROMs from the shared ROM directory conventions, accepts zipped or plain `ADF` images, runs scripted keys, and captures PNG/WAV output
4. kept the boundary honest: the fresh workspace now proves Kickstart-visible output and DF0 media insertion, but it does not yet claim a native verifier UI, snapshot support, or a full Workbench/game boot proof
5. cleaned the imported `motorola-68000` warnings so the new slice can live under the workspace `fmt`/`clippy -D warnings` bar

**Verification:** locally, this slice passes:
- `cargo fmt --all --check`
- `cargo test -p motorola-68000 -p mos-cia-8520 -p commodore-gary -p commodore-agnus-ocs -p commodore-denise-ocs -p commodore-paula-8364 -p format-commodore-amiga-adf -p peripheral-commodore-amiga-floppy -p peripheral-commodore-amiga-keyboard -p machine-commodore-amiga -p runtime-commodore-amiga -p emu198x-script-amiga`
- `cargo clippy -p motorola-68000 -p mos-cia-8520 -p commodore-gary -p commodore-agnus-ocs -p commodore-denise-ocs -p commodore-paula-8364 -p format-commodore-amiga-adf -p peripheral-commodore-amiga-floppy -p peripheral-commodore-amiga-keyboard -p machine-commodore-amiga -p runtime-commodore-amiga -p emu198x-script-amiga --all-targets -- -D warnings`
- `cargo run --release -p emu198x-script-amiga -- --rom-dir ~/.emu198x/roms/commodore-amiga --wait-for-boot 300 --screenshot /tmp/amiga-kick13.png`
- `cargo run --release -p emu198x-script-amiga -- --rom-dir ~/.emu198x/roms/commodore-amiga --disk '/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip' --wait-for-boot 300 --frames 1000 --print-query amiga.disk.inserted --print-query amiga.disk.motor_on --print-query amiga.disk.motor_spinning --print-query amiga.cpu.pc`

**Consequence:** the repo no longer needs to say “no fresh-workspace Amiga product path.” The current honest state is: the Amiga now has a real headless A500 OCS PAL baseline in the active workspace, while native UI, snapshots, and stronger software proofs are still pending.

## 2026-04-15 — Fresh-workspace NES headless path restored

**Type:** milestone
**Trigger:** With Spectrum and C64 now both in the “working but incomplete” bucket, the next project goal was to get the third platform back onto a real fresh-workspace footing instead of leaving NES as only historical wiki claims and archive references.
**Result:** the fresh workspace now has a live NES baseline again:
1. restored `format-nintendo-nes-ines`, `ricoh-ppu-2c02`, `ricoh-apu-2a03`, and `machine-nintendo-nes` from the older workspace into the active cargo workspace
2. added a new current-style `runtime-nintendo-nes` crate on the shared `MachineCore` boundary instead of reviving the older `System` trait wrapper
3. added `emu198x-script-nes`, a fresh headless runner that loads `cartridge-1` media, runs native NES frames, saves screenshots/audio, and accepts shared scripted input
4. left the boundary honest: firmwareless cartridge boot works, but snapshots are still unsupported and mapper coverage is still NROM-only
5. added one ignored local `Super Mario Bros.` regression hook in `runtime-nintendo-nes`

**Verification:** locally, this slice passes:
- `cargo test -p format-nintendo-nes-ines -p ricoh-ppu-2c02 -p ricoh-apu-2a03 -p machine-nintendo-nes -p runtime-nintendo-nes -p emu198x-script-nes`
- `cargo clippy -p format-nintendo-nes-ines -p ricoh-ppu-2c02 -p ricoh-apu-2a03 -p machine-nintendo-nes -p runtime-nintendo-nes -p emu198x-script-nes --all-targets -- -D warnings`
- `cargo run --release -p emu198x-script-nes -- --rom '/Users/stevehill/Projects/Emu198x-Unclean/Reference/nintendo/nes/test-suites/other/nestest.nes' --frames 60 --screenshot /tmp/nes-nestest.png`
- `cargo run --release -p emu198x-script-nes -- --rom '/Users/stevehill/Projects/Emu198x-Unclean/Reference/nintendo/nes/Super Mario Bros. (1985-09-13)(Nintendo)(JP-US).nes' --frames 240 --screenshot /tmp/nes-smb.png`

**Consequence:** the repo no longer needs to describe NES as having “no fresh-workspace product path.” The current honest state is: headless NES cartridge boot exists and runs real NROM software, while mapper breadth, snapshots, and a native verifier UI are still pending.

## 2026-04-15 — 1541 live-disk blocker synthesized into a drive bring-up note

**Type:** note
**Trigger:** After the live `1541` path reached a stable `SEARCHING FOR *` stall on plain `D64` titles, the project needed a concise working note that merged the new 1541-specific references into one debug map instead of leaving the relevant facts scattered across manuals, OCR, and ROM listings.
**Result:** added [1541-DISK-BRINGUP-NOTES.md](/Users/stevehill/Projects/Emu198x/docs/platforms/commodore-64/hardware/1541-DISK-BRINGUP-NOTES.md), which captures:
1. the exact current boundary of the live disk problem
2. what is already ruled out
3. the 1541 board split between `UC3` serial-side `VIA` and `UC2` read/mechanics `VIA`
4. the `6522` interrupt / handshake behavior that still matters
5. the key DOS ROM landmarks and RAM variables around the current hot loop
6. the next focused trace checklist for getting from `SEARCHING FOR *` to `LOADING`
**Consequence:** the 1541 work now has a single repo-local reference for the current blocker, so the next implementation/debugging pass can target the live DOS/VIA/IEC handoff directly instead of re-deriving the same board and ROM facts from raw sources.

## 2026-04-15 — `Bomb Jack` becomes a third live-1541 disk proof

**Type:** milestone
**Trigger:** After `Aztec Challenge` became the second readable disk anchor, the next useful title to probe was something loader-heavier to check whether the same live 1541 path could survive a multi-stage disk flow without collapsing back to a one-stage BASIC proof.
**Result:** local `Bomb Jack (1986)(Elite)` now provides that third proof on the same attached-drive path:
1. typed `LOAD"*",8,1` reaches a visible multi-stage loader (`SEARCHING FOR *`, `LOADING`, `SEARCHING FOR .1`, `LOADING`, `SEARCHING FOR .2`, `LOADING`)
2. after the loader settles and the drive goes idle, the real framebuffer shows the readable Bomb Jack title screen with `PRESS FIRE TO PLAY`
3. joystick port 2 does not move that title, but joystick port 1 does: port-1 fire advances to a later `TOP TEN BOMBJACKERS` screen
4. an ignored ROM-backed regression now preserves the title-to-port-1-fire transition in `runtime-commodore-c64`

**Verification:** locally, this slice passes:
- `./target/release/emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Bomb Jack (1986)(Elite).zip' --autoload-disk --script /tmp/bombjack_very_long.json`
- `./target/release/emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Bomb Jack (1986)(Elite).zip' --autoload-disk --script /tmp/bombjack_fire_port1.json`

**Consequence:** the live 1541 path now has three distinct disk software anchors with different pressure:
- `Bruce Lee` for later-title input-driven progression
- `Aztec Challenge` for readable post-load menu/instruction flow
- `Bomb Jack` for multi-stage loader survival plus bitmap-title input response

## 2026-04-15 — `Aztec Challenge` becomes a second readable C64 disk anchor

**Type:** milestone
**Trigger:** After `Bruce Lee` proved the live 1541 path could go beyond `LOADING`, the next honest question was whether that path generalized to another plain `D64` title instead of only one software stack.
**Result:** local `Aztec Challenge (1983)(Cosmi)` now provides that second anchor on the same live attached-drive path:
1. typed `LOAD"*",8,1` reaches `LOADING`
2. the first disk stage returns to BASIC cleanly with the drive idle
3. typing `RUN` reaches a readable player-select screen
4. pressing `F1` reaches the readable `THE GAUNTLET` instruction screen with `PRESS FIRE BUTTON TO START`
5. an ignored ROM-backed regression now preserves that path in `runtime-commodore-c64`

`Bomb Jack (1986)(Elite)` was also probed on the same path and already shows a useful multi-stage loader signature (`SEARCHING FOR .1`, `LOADING`, `SEARCHING FOR .2`, `LOADING`), but it was not promoted to the main disk anchor in this slice.

**Verification:** locally, this slice passes:
- `./target/release/emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Aztec Challenge (1983)(Cosmi).zip' --autoload-disk --script /tmp/aztec_run.json`
- `./target/release/emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Aztec Challenge (1983)(Cosmi).zip' --autoload-disk --script /tmp/aztec_f1.json`
- `./target/release/emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Bomb Jack (1986)(Elite).zip' --autoload-disk --script /tmp/bombjack_longer.json`

**Consequence:** the live 1541 path is no longer anchored only on `Bruce Lee`. The repo now has a second plain `D64` title with a readable post-load software state, which is a better basis for judging whether future disk fixes are general or title-specific.

## 2026-04-15 — `Bruce Lee` now advances past the title and responds to joystick input on the live 1541 path

**Type:** milestone
**Trigger:** Once the live 1541 path could reach `LOADING` and then Bruce Lee's title screen after `RUN`, the next honest question was whether the attached-drive path had only reached a static presentation layer or whether the loaded software was actually live enough to respond to controller input.
**Result:** the fresh workspace now has a stronger disk-software proof on top of the same ROM-backed `Bruce Lee (1984)(Datasoft)` path:
1. C64 joystick input is now wired into the board through CIA1 port reads instead of only the keyboard matrix.
2. After the existing `LOAD"*",8,1` plus `RUN` path, joystick fire on port 2 advances Bruce Lee beyond the title screen into a stable later scene with different screen codes and colours.
3. From that post-title state, joystick-right changes the rendered framebuffer again while the live 1541 is already idle, which is a much stronger sign that real loaded software is still running rather than the machine parking on another static splash screen.
4. Ignored ROM-backed regressions now preserve both the post-title fire transition and the later joystick-motion response in `runtime-commodore-c64`.
**Verification:** locally, this slice passes:
- `cargo test -p machine-commodore-c64 -p runtime-commodore-c64`
- `./target/release/emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Bruce Lee (1984)(Datasoft).zip' --autoload-disk --script /tmp/bruce_lee_after_fire.json`
- `./target/release/emu198x-script-c64 --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Bruce Lee (1984)(Datasoft).zip' --autoload-disk --script /tmp/bruce_lee_fire_then_right_compare.json`
**Consequence:** the current live-disk path is now beyond “title starts after `RUN`.” The next honest C64 disk question is no longer whether the machine can leave BASIC and paint a title screen; it is how far this live 1541 path can be driven into clearly playable/control-responsive software across Bruce Lee and the next plain `D64` titles.

## 2026-04-15 — C64 IEC output inversion fix moves live 1541 path to `LOADING`

**Type:** fix
**Trigger:** The live `1541` path had narrowed to a stubborn `SEARCHING FOR *` stall even though the drive ROM, mounted `D64`, GCR read path, and disk-controller `VIA` activity were all live. Focused tracing showed the 1541 never latched the expected serial `ATN` interrupt path, and the suspicious clue was that the drive already saw `ATN` low at the C64 `READY.` prompt before `LOAD"*",8,1` even started.
**Result:** the C64-side IEC glue in `machine-commodore-c64` now matches VICE more closely by inverting the mixed `CIA2 Port A` drive state before handing it to `common-commodore-iec`. That fixes the active-low serial output mapping and removes the false pre-command `ATN` assertion on the bus. A new ignored ROM-backed regression proves that local `Bruce Lee (1984)(Datasoft)` now advances from `SEARCHING FOR *` to `LOADING` on the live `1541` path.
**Verification:** locally, this slice passes:
- `cargo test -p machine-commodore-c64`
- `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --wait-for-boot 200 --print-query c64.cia2.port_a_latch --print-query c64.cia2.ddra --print-query c64.iec.cpu_port --print-query c64.iec.drive_port`
- `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Bruce Lee (1984)(Datasoft).zip' --autoload-disk --frames 400 --print-screen-text`
**Consequence:** the live disk path is still not yet full end-to-end 1541 DOS-sector loading, but it has crossed the main stalled boundary. The next honest target is no longer “leave `SEARCHING FOR *` at all”; it is “how far past `LOADING` does the real attached-drive path get on plain disk software.”

## 2026-04-15 — `Bruce Lee` now reaches its title screen after `RUN` on the live 1541 path

**Type:** milestone
**Trigger:** Once the IEC polarity fix moved the live attached-drive path from `SEARCHING FOR *` to `LOADING`, the next honest question was whether a plain disk title would actually start correctly after the normal post-load BASIC action instead of dropping back into another broken state.
**Result:** local `Bruce Lee (1984)(Datasoft)` now behaves like a real multi-stage disk title on the live 1541 path:
1. typed `LOAD"*",8,1` reaches `LOADING`
2. the first disk stage returns to BASIC cleanly
3. typing `RUN` through the normal C64 keyboard path reaches the title screen, with the live drive still active for the next stage
4. an ignored ROM-backed regression now preserves that path in `runtime-commodore-c64`
**Verification:** locally, this slice passes:
- `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Bruce Lee (1984)(Datasoft).zip' --autoload-disk --script /tmp/bruce_lee_run_after_load.json --print-screen-text`
- `cargo test -p runtime-commodore-c64 real_d64_autoload_bruce_lee_starts_after_run -- --ignored --nocapture`
**Consequence:** the current disk path is now well beyond “KERNAL banner plus head movement.” The repo has a real title-start proof on the live `1541` path, which is a much better anchor for the remaining disk work than the earlier `SEARCHING FOR *` stall.

## 2026-04-14 — Live 1541 path now enters real BASIC disk autoload

**Type:** milestone
**Trigger:** After mounted `D64` media and the first drive-side mechanics/status slice landed, the next honest disk step was to stop assuming `SHIFT+RUN/STOP` would enter the 1541 path and prove a real BASIC-side disk command over the live attached drive.
**Result:** the fresh workspace now has the first ROM-backed live-disk autoload proof:
1. `runtime-commodore-c64::autoload_basic_disk()` now types the real BASIC command `LOAD"*",8,1` instead of incorrectly assuming the tape-oriented `SHIFT+RUN/STOP` shortcut would enter the disk path.
2. `emu198x-script-c64` and `emu198x-c64` now expose that host-side workflow through `--autoload-disk`, while keeping it clearly above the emulation boundary.
3. `machine-commodore-1541` now reads VIA1 Port B with a VICE-style mixed IEC/status byte instead of a generic DDR-masked port value, which makes the live DOS-side port view more faithful.
4. A real local `Bruce Lee (1984)(Datasoft)` `D64` proof now reaches the KERNAL `SEARCHING FOR` banner and then moves the live 1541 head, which is the first honest sign that the attached drive path has entered command-side motion beyond mere media insertion.
**Verification:** locally, this slice passes:
- `cargo fmt --all`
- `cargo test -p machine-commodore-1541 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-c64`
- `cargo clippy -p machine-commodore-1541 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-c64 --all-targets -- -D warnings`
- `cargo test -p runtime-commodore-c64 real_d64_autoload_bruce_lee_starts_drive_motion -- --ignored --nocapture`
**Consequence:** the live disk path is still not yet full DOS-sector/GCR loading, but the C64 is now entering the real disk search path through the BASIC editor and provoking observable 1541 head movement instead of stopping at “disk mounted” plus board scaffolding.

## 2026-04-14 — Live 1541 runtime now owns mounted D64 media

**Type:** milestone
**Trigger:** After the optional live 1541 runtime attachment landed, the next honest disk step was to stop treating `D64` as only a host-side import helper and let the attached drive actually own inserted disk media.
**Result:** the fresh workspace now has the first mounted-disk path on the live 1541:
1. `machine-commodore-1541` now owns mounted `D64` state directly, including raw image bytes plus parsed disk-name / id / directory metadata.
2. `runtime-commodore-c64` now accepts `drive-8` disk media when a 1541 ROM is attached, exposes `c64.drive8.disk.*` query paths, and fails honestly with missing-firmware when callers try to mount a disk without a live drive.
3. `emu198x-script-c64` and `emu198x-c64` now accept `--disk PATH`, which inserts a `D64` into the live drive-8 path rather than faking a load.
4. A real local `Bruce Lee (1984)(Datasoft)` `D64` mount now reports `c64.drive8.attached=true`, `c64.drive8.disk.inserted=true`, `c64.drive8.disk.name=\"BRUCELEE\"`, and `c64.drive8.disk.id=\"00\"` through the headless runner.
**Verification:** locally, this slice passes:
- `cargo fmt --all`
- `cargo test -p machine-commodore-1541 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-c64`
- `cargo clippy -p machine-commodore-1541 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-c64 --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --disk '.../Bruce Lee (1984)(Datasoft).zip' --wait-for-boot 200 --print-query c64.drive8.attached --print-query c64.drive8.disk.inserted --print-query c64.drive8.disk.name --print-query c64.drive8.disk.id --frames 1`
**Consequence:** the disk path is still not yet DOS/IEC-backed file loading, but the attached 1541 now owns real disk media instead of the runtime stopping at an empty board plus host-side `D64` import shortcuts.

## 2026-04-14 — C64 runtime can now attach a live 1541 and observe it honestly

**Type:** milestone
**Trigger:** After the first shared IEC line-state slice landed, the next honest step was to stop keeping the 1541 board isolated and actually let the C64 runtime run with an attached live drive before disk media mechanics exist.
**Result:** the fresh workspace now has the first runtime-level C64+1541 execution path:
1. `runtime-commodore-c64` can optionally consume a `commodore-1541-dos-rom` firmware image and attach a live `machine-commodore-1541` board on the shared IEC bus.
2. The C64 query surface now exposes attached-drive visibility (`c64.drive8.*` plus raw IEC port views), so later DOS/IEC work can be debugged through the same shell/session surface instead of bespoke probes.
3. Attached-drive snapshots are now explicit and tested via a dedicated `Drive1541Snapshot`, rather than relying on accidental serde support for large board arrays.
4. Synthetic runtime tests now prove drive attachment, cycle advancement, and snapshot round-trip; a local ignored ROM-backed test also proves real 1541 ROM execution is visible through the runtime query surface.
**Verification:** locally, this slice passes:
- `cargo fmt --all`
- `cargo test -p machine-commodore-1541 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-c64`
- `cargo clippy -p machine-commodore-1541 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-c64 --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo test -p runtime-commodore-c64 query_provider_reports_real_attached_drive_progress -- --ignored --nocapture`
**Consequence:** the disk path is still not real 1541 media loading yet, but the repo now has a live runtime-level C64↔1541 execution/debug surface instead of only separate board crates and host-side `D64` import.

## 2026-04-14 — C64 and 1541 now share a real IEC line-state model

**Type:** milestone
**Trigger:** After the standalone 1541 substrate landed, the next honest disk step was to stop treating IEC as future glue and wire the C64 CIA2 side and the 1541 VIA1 side through one shared serial-bus model.
**Result:** the fresh workspace now has the first line-level C64↔1541 IEC path:
1. Added `common-commodore-iec`, a new shared crate that mirrors the open-collector IEC DATA/CLOCK/ATN line encoding closely enough for both boards to agree on one bus state.
2. Extended `machine-commodore-1541` with custom VIA1 port-B reads, IEC-aware board reads/writes, and a `tick_with_iec_bus` path instead of pretending VIA1 port B behaves like a generic 6522 register.
3. Extended `machine-commodore-c64` with matching IEC-aware CIA2 reads/writes and a `tick_with_iec_bus` path, while keeping the standalone C64 board behaviour unchanged.
4. Added line-level tests proving that drive-side DATA pulls reach CIA2 input bits and that C64-side ATN changes are visible through the 1541 VIA1 register view.
**Verification:** locally, this slice passes:
- `cargo fmt --all`
- `cargo test -p common-commodore-iec -p mos-via-6522 -p machine-commodore-1541 -p machine-commodore-c64`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
**Consequence:** the disk path is no longer just a host-side `D64` parser plus an isolated 1541 board. The fresh workspace now has the first honest shared C64/1541 serial-bus state, which is the right base for the next drive-side DOS/IEC command work.

## 2026-04-14 — 1541 path now has a real second-computer substrate

**Type:** milestone
**Trigger:** After the `D64` container/parser slice, the next honest disk step was to stop treating the 1541 as future glue and build the drive-side computer that IEC and disk mechanics will eventually talk to.
**Result:** the fresh workspace now has the first real 1541 board substrate:
1. Added `mos-via-6522`, a new standalone VIA crate with live port direction, `ACR`/`PCR`, `IFR`/`IER`/`IRQ`, `T1`/`T2`, and edge-triggered `CA1`/`CA2`/`CB1`/`CB2` behavior.
2. Added `machine-commodore-1541`, which wires a real `mos-6502` to 2 KB mirrored RAM, 16 KB DOS ROM, and the two VIA windows at `$1800` and `$1C00`.
3. Proved the reset-vector path and board decode locally with direct unit tests: ROM reset boot, RAM mirroring, VIA register mirroring, and CPU writes through the board bus into VIA space.
**Verification:** locally, this slice passes:
- `cargo test -p mos-via-6522 -p machine-commodore-1541`
- `cargo clippy -p mos-via-6522 -p machine-commodore-1541 --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
**Consequence:** the C64 disk path is no longer only a `D64` parser plus host import bridge. The repo now has the first honest second-computer substrate needed for real 1541/IEC work.

## 2026-04-14 — Thing on a Spring now proves a real post-load interaction, not just menu reach

**Type:** milestone
**Trigger:** After `Thing on a Spring` became the strongest C64 tape regression by reaching a stable readable menu, the next useful question was whether the loaded title would actually respond correctly to live keyboard input instead of merely sitting at a post-load attract/menu state.
**Result:** the fresh-workspace C64 now has a stronger ROM-backed `Thing on a Spring` proof:
1. Probed the live title headlessly and confirmed that pressing `SPACE` after the menu is reached transitions reliably into a distinct started/game screen.
2. Confirmed that the post-`SPACE` decoded screen state is stable across later frame windows, rather than a transient redraw.
3. Added an ignored ROM-backed regression that boots, autoloads the real TAP, reaches the menu, presses `SPACE` through the normal input path, and checks both a stable started-screen signature and a changed framebuffer.
**Verification:** locally, this slice passes:
- `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --tape '.../Thing on a Spring (1985)(Gremlin).zip' --autoload-tape --script /tmp/thing-space-after.json`
- `cargo test --release -p runtime-commodore-c64 real_tap_autoload_thing_on_a_spring_starts_after_space -- --ignored --nocapture`
**Consequence:** `Thing on a Spring` is now more than a menu anchor. It is the first fresh-workspace C64 tape title that proves real post-load interaction through the live keyboard path.

## 2026-04-14 — C64 disk bootstrap begins with D64 container parsing and host-side import

**Type:** milestone
**Trigger:** After the first C64 tape title reached a real post-load interaction proof, the next storage step was disk. The fresh workspace still had no 1541/VIA/IEC path, so the smallest honest bootstrap was the `D64` container layer first.
**Result:** the fresh-workspace C64 now has a real `D64` parser plus host-side import support:
1. Added `format-commodore-c64-d64`, which parses standard 35-track `D64` images, reads the BAM/directory, and follows PRG sector chains with loop detection.
2. Wired `.d64` into the existing C64 `--load PATH` host-convenience path, alongside `.prg`, `.bas`, and `.t64`.
3. Chose original single-file `Bruce Lee (1984)(Datasoft)` and `Aztec Challenge (1983)(Cosmi)` as the first disk-software anchors, because they are cleaner initial targets than cracked multi-stage or multi-disk images such as `Last Ninja`.
**Verification:** locally, this slice passes the new direct `format-commodore-c64-d64` tests plus the updated C64 runtime/script tests and clippy.
**Consequence:** this is not 1541 emulation yet, but it gives the disk path a real container/parser substrate and an immediately usable host-side bridge while the drive/VIA/IEC machine work is still ahead.

## 2026-04-14 — Thing on a Spring becomes the strongest C64 tape regression so far

**Type:** milestone
**Trigger:** After Ghostbusters was repaired enough to reach its copyright/later-loader state, the next useful question was which real C64 tape title would give the cleanest, most reproducible end-to-end regression target instead of another loader-only checkpoint.
**Result:** three candidate titles were triaged headlessly with the same fresh-workspace PAL C64 tape path:
1. `Thing on a Spring (1985)(Gremlin)` reached the clearest stable end state by far: a readable post-load menu with score-table and control text.
2. `Impossible Mission (1984)(Epyx)` reached a later state that still appears to want `RUN`, but it is not yet as clean or self-evident a regression target.
3. `Paperboy (1986)(Elite)` reached a later graphical state, but not one with decoded text good enough to use as the next primary software proof.
4. Added an ignored ROM-backed `Thing on a Spring` regression that boots, autoloads the real TAP, runs to the stable menu state, checks multiple readable menu/control lines, and confirms the full TAP has been consumed.
**Verification:** locally, this slice passes:
- `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --tape '.../Thing on a Spring (1985)(Gremlin).zip' --autoload-tape --frames 25000 --print-screen-text`
- `cargo test --release -p runtime-commodore-c64 real_tap_autoload_thing_on_a_spring_reaches_menu -- --ignored --nocapture`
**Consequence:** `Thing on a Spring` is now the best real-software C64 tape regression in the fresh workspace. It is a better next anchor than Ghostbusters for broad C64 software confidence because it reaches a stable, readable menu instead of only a loader or graphics-heavy transitional state.

## 2026-04-14 — C64 VIC colour-write tracing shows Ghostbusters border flashes start late, not “dropped per frame”

**Type:** milestone
**Trigger:** After the 6510 banking fix, Ghostbusters loads far enough to reach the copyright screen, but the expected early loading bars still did not appear in the native verifier shell. The next question was whether those bars were being lost in host presentation, or whether the machine was simply not generating the corresponding VIC colour writes yet.
**Result:** the fresh workspace now has one targeted C64 debug tool and one concrete finding:
1. Added `emu198x-script-c64 --trace-vic-colours`, which traces `D020`/`D021` changes during the explicit `--frames` run window and records machine time, raster line, cycle-in-line, and `PC`.
2. Added one narrow runtime hook that emits these trace events only when explicitly enabled, so normal runs remain unchanged.
3. Used the new trace against Ghostbusters after `--autoload-tape`.
4. Confirmed that there are **no** `D020`/`D021` changes at all in the first `1200` post-autoload frames.
5. Confirmed that later in the load there is a dense stream of `D020` toggles, all from the loaded-code border-flash routine around `PC=$CEC0`, alternating border colours `11` and `14` over many raster positions.
**Verification:** locally, this slice passes:
- `cargo test -p emu198x-shell -p runtime-commodore-c64 -p emu198x-script-c64`
- `cargo clippy -p emu198x-shell -p runtime-commodore-c64 -p emu198x-script-c64 --all-targets -- -D warnings`
- `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --tape '.../Ghostbusters (1984)(Activision).zip' --autoload-tape --frames 1200 --trace-vic-colours`
- `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --tape '.../Ghostbusters (1984)(Activision).zip' --autoload-tape --frames 6000 --trace-vic-colours --trace-limit 200`
**Consequence:** the missing early bars are not well explained by “the frontend only updates on full frames” when turbo is off. The current machine path simply does not execute the relevant VIC colour-write activity in that earlier window. The remaining discrepancy is more likely an earlier loader/timing difference than a generic presentation bug.

## 2026-04-14 — C64 Ghostbusters root cause narrowed to wrong 6510 banking bits; later loader state now proven

**Type:** milestone
**Trigger:** manual Ghostbusters verification showed that the fresh-workspace C64 datasette path was still wrong in a way that loader-banner tests had not exposed. The machine reached `FOUND MAIN`, but not the expected later loading behaviour, and the next task was to prove whether that was CPU, tape, CIA, or bank-selection related.
**Result:** the critical C64 bug was not in the 6502 core or the raw TAP parser. It was in the 6510 memory-configuration wiring:
1. Added temporary-but-useful C64 query paths for CIA timer latches and 6510 port state (`port_ddr`, `port_data`, `effective_port`, `io_visible`), on top of the earlier Ghostbusters trace surface.
2. Used those queries to prove that the late Ghostbusters state had `effective_port = $16`. On a real C64 that still leaves `CHAREN=1`, but the fresh workspace was interpreting the low three bits incorrectly and was hiding I/O when it should have been visible.
3. Corrected the 6510 banking-bit assignments in `machine-commodore-c64`: bit 0 is `LORAM`, bit 1 is `HIRAM`, and bit 2 is `CHAREN`.
4. Tightened the datasette model at the same time with explicit motor spin-up/spin-down delay, separate tape sense vs motion, and cleaner tape-state queries so the debug surface matched the service manuals and VICE more closely.
5. Added a new ignored ROM-backed Ghostbusters regression that proves the current machine now moves beyond the first-stage `FOUND MAIN` banner into a later graphics-heavy loader state, with I/O visible and CIA2 Timer A programmed (`latch = 280`), instead of stalling at the old post-banner state.
**Verification:** locally, this slice passes:
- `cargo test -p machine-commodore-c64 -p runtime-commodore-c64 -p emu198x-script-c64`
- `cargo clippy -p machine-commodore-c64 -p runtime-commodore-c64 -p emu198x-script-c64 -p mos-cia-6526 --all-targets -- -D warnings`
- `cargo test --release -p runtime-commodore-c64 real_tap_autoload_ghostbusters_reaches_later_loader_state -- --ignored --nocapture`
- repeated `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --tape '.../Ghostbusters (1984)(Activision).zip' --autoload-tape --frames 25000 ...`
**Consequence:** the fresh-workspace C64 datasette path is now materially more credible, and Ghostbusters is no longer evidence that the whole tape path is fundamentally broken. The remaining work is narrower: complete-title handoff, more real-title proofs, and any remaining CIA/VIC integration gaps that show up after the later loader state.

## 2026-04-14 — C64 trace/query surface proves Ghostbusters consumes the full TAP and still stalls in loaded code

**Type:** milestone
**Trigger:** After the datasette-state split and the full 6502 verification push, `Ghostbusters` was still not reaching a useful post-load title state. At that point the project needed machine-level visibility into the live C64 state instead of more guesswork about whether the parser or CPU core were broadly wrong.
**Result:** the fresh-workspace C64 runtime and headless runner now expose a temporary-but-useful machine trace surface:
1. Added C64 query paths for CPU registers and pins (`pc`, `a/x/y`, `sp`, `p`, `rw`, `addr`, `data`, `sync`, `irq`, `nmi`, `rdy`, `total_cycles`), CIA1 tape-relevant state (`flag`, `icr_status`, `icr_mask`, `timer_a`, `timer_b`), VIC border/background colour, frame count, and datasette pulse position/motor state.
2. Added `emu198x-script-c64 --print-query PATH`, so headless reproductions can dump the exact post-run machine state without adding one-off debug code for each title.
3. Used that surface to prove that `Ghostbusters` is multi-stage, does restart the motor after `FOUND MAIN`, and eventually consumes the full TAP pulse stream.
4. Confirmed that after the full TAP stream is consumed, the machine still does not reach a title/menu state: it settles with the tape at end-of-stream, the screen stuck on loader-era text, and the CPU held in a late loaded-code state around `$CEBE`.
**Verification:** locally, this slice passes `cargo test -p runtime-commodore-c64 -p emu198x-script-c64` and `cargo clippy -p runtime-commodore-c64 -p emu198x-script-c64 --all-targets -- -D warnings`. The headless Ghostbusters repro is now directly inspectable with repeated `--print-query` arguments on `emu198x-script-c64`.
**Consequence:** the remaining Ghostbusters issue is now much less likely to be “the TAP parser is obviously wrong” or “the 6502 is broadly wrong”. The next debugging surface should be deeper C64 machine behaviour after load handoff: RAM-side loader state, CIA tape/IRQ timing, VIC-visible loader behaviour, or related integration gaps.

## 2026-04-14 — C64 datasette state split tightened against manuals/VICE; Ghostbusters still stops after first stage

**Type:** milestone
**Trigger:** manual `Ghostbusters` verification showed that the fresh-workspace C64 datasette path still was not credible as a full software-loading proof, and the first implementation review against the Commodore service manuals and VICE exposed that the live datasette state was still collapsing distinct physical signals together.
**Result:** the live C64 datasette path is now less lossy and more explicit:
1. Split the datasette state so the latched PLAY button (`sense`) is no longer conflated with actual tape motion. The machine now distinguishes “button down” from “motor running with pulses left”.
2. Removed the fabricated bit-5 “motor input” from the 6510 port read path; bit 5 remains the motor-control output unless/until a real external motor-input source is modeled.
3. Added a new query path, `c64.tape.sense`, so scripts and debugging can inspect the latched transport-button state separately from `c64.tape.playing`.
4. Corrected `emu198x-script-c64 --wait-for-tape-stop` so it now waits for `c64.tape.playing` to become true and then false, rather than returning immediately when PLAY is latched before the motor starts.
**Verification:** locally, the updated slice passes:
- `cargo test -p machine-commodore-c64`
- `cargo test -p runtime-commodore-c64 -p emu198x-script-c64`
- manual headless Ghostbusters repro via `cargo run --release -p emu198x-script-c64 -- --rom-dir ~/.emu198x/roms/commodore-c64 --tape '.../Ghostbusters (1984)(Activision).zip' --autoload-tape --wait-for-tape-stop 12000 --frames 120 --print-screen-text`
**Consequence:** this did not solve Ghostbusters. The title still reaches `SEARCHING`, `FOUND MAIN`, and then stops after the first loader stage. That makes the remaining issue more likely to be deeper C64 machine behavior around datasette stage transitions, CIA timing, or VIC-visible loader behavior, rather than the runner or the 6502 core.

## 2026-04-14 — C64 tape wording corrected: current proofs are loader-banner states, not full title loads

**Type:** correction
**Trigger:** The active C64 docs had drifted into implying that the current `Thinker` and `Thomas the Tank Engine` TAP regressions proved successful end-to-end software loading. That overstates what the tests actually show.
**Correction:** the current C64 TAP regressions prove stable observable KERNAL loader-banner states under the real datasette path:
1. `Thinker` reaches `FOUND THINKER`, `LOADING`, and a following `READY.` line.
2. `Thomas the Tank Engine` reaches `FOUND THOMAS`, `LOADING`, and a following `READY.` line.
3. These are useful loader-stage proofs, but they are not yet proof that either title fully loaded, auto-started, or reached a software-complete title/game state.
**Follow-up:** the next real tape milestone should be a stronger end state on at least one title, plus a direct cross-check of TAP behavior against a known-good reference path where useful.

## 2026-04-14 — 6502 core reaches full Harte, Lorenz, and Dormann green; Ghostbusters remains a C64-system issue

**Type:** milestone
**Trigger:** After the first 6502 verification slice, Ghostbusters still failed on the fresh-workspace C64 datasette path. That made it necessary to keep pushing CPU verification until the remaining gap was small enough to either explain the title failure or rule the CPU out cleanly.
**Result:** the live `mos-6502` core is now externally clean on all three currently wired verification families:
1. Completed the missing undocumented-opcode semantics in the live core for `ARR`, `ANC`, `ANE`, `LXA`, `LAS`, `SHA`, `SHX`, `SHY`, and `TAS`, instead of leaving them as `NopRead` placeholders.
2. Fixed a second real timing bug in the addressing-mode scheduler: zero-page and absolute indexed paths had been deciding whether they were indexed from the runtime index value (`X=0`/`Y=0`) instead of from the opcode's addressing mode, which broke both documented and undocumented timing on cases such as `ORA zp,X` and `ASL zp,X` when the index register happened to be zero.
3. Corrected the `ANE` unstable mask to match the external vectors and added a decimal-mode `ARR` path that matches the real NMOS behavior more closely instead of panicking in debug builds.
4. Added `tests/dormann_tests.rs`, wired to the packaged local Klaus Dormann functional memory image under `~/Projects/Emu198x-Unclean/6502_65C02_functional_tests`.
5. Extended test fixture discovery so Tom Harte, Lorenz, Dormann, and the C64 KERNAL ROM all resolve consistently from local external paths or explicit environment variables.
**Verification:** locally, the current state is:
- Tom Harte full NMOS 6502 corpus: `2,560,000 / 2,560,000` passed
- Lorenz CPU subset: `222 / 222` passed
- Dormann functional test: pass, reaching success loop `$3469` in `96,241,367` cycles
- `cargo test -p mos-6502`
- `cargo clippy -p mos-6502 --all-targets -- -D warnings`
**Consequence:** Ghostbusters still does not progress into a useful end state after the first tape stage even with the now-verified 6502 core, so the remaining issue is no longer credibly “the CPU is probably wrong.” The next debugging surface should be the wider C64 machine path: datasette stage transitions, CIA/VIC-visible loader behavior, and machine-level trace/query instrumentation.

## 2026-04-14 — 6502 verification harnesses land, and the first Tom Harte fix closes a real C64-relevant timing bug

**Type:** milestone
**Trigger:** `Ghostbusters` on the fresh-workspace C64 datasette path exposed a real gap in confidence: the tape loader reached `FOUND MAIN` / `LOADING`, then stalled without progressing into useful software state. At the same time, branch coverage had slipped, and the 6502 still lacked the same kind of external verification bar the Z80 already had.
**Result:** the 6502 now has real external verification harnesses, and they have already paid off:
1. Added `tests/single_step_tests.rs` to `mos-6502`, wired to the local Tom Harte NMOS 6502 corpus under `~/Projects/Emu198x-Unclean/65x02/6502/v1` (or `EMU198X_6502_TOM_HARTE_DIR`).
2. Added `tests/lorenz_tests.rs` to `mos-6502`, wired to the local Wolfgang Lorenz C64 CPU suite plus a real KERNAL ROM. The harness mirrors the archived C64-style trap setup instead of inventing a new execution environment.
3. Added fixture discovery helpers in `tests/support/mod.rs` for Tom Harte, Lorenz, and the local C64 KERNAL ROM.
4. Corrected the Lorenz harness budgeting so long-running ADC/SBC and flow/branch cases stop being misreported as CPU failures when they simply exceed an unrealistically low cycle cap.
5. Fixed a real documented-core bug in `tick_absolute`: absolute indexed reads without page crossing were incorrectly taking the longer page-cross path. Tom Harte documented-opcode failures for `ORA abs,X` / `ORA abs,Y` dropped to zero immediately after the fix.
**Verification:** locally, the current state is:
- Lorenz smoke `ldab`: pass (`14,259,836` cycles)
- Lorenz `adcb`: pass (`20,888,066` cycles)
- Lorenz CPU subset: `212 / 222` passed, with the remaining failures concentrated in undocumented-opcode families (`ARR`, `ANC`, `ANE`, `LXA`, `LAS`, `SHX`, `SHY`, `SHS`, plus one remaining long-running `ancb` path)
- Tom Harte full NMOS 6502 corpus: improved from `2,402,361 / 2,560,000` to `2,485,211 / 2,560,000` passing after the `tick_absolute` fix
- `Ghostbusters` still does not complete end-to-end, which is now better framed as a remaining CPU/machine-verification problem rather than a blind TAP-loader guess
**Next dependency:** keep driving Tom Harte and Lorenz until the remaining failure surface is small and named, then circle back to `Ghostbusters` and other real C64 titles with that stronger CPU baseline.

## 2026-04-14 — C64 datasette software validation now includes Thomas alongside Thinker

**Type:** milestone
**Trigger:** After the first ROM-backed C64 tape regression (`Thinker`) was in place, the next useful step was not more transport plumbing but a second real title with a fast, queryable, decoded screen state. Several local TAP candidates either stayed visually blank under text decoding or never settled into a useful automated stop condition, so the target needed to be chosen by actual headless probe rather than by filename.
**Result:** the fresh-workspace C64 now has a second ROM-backed datasette software regression:
1. Added a local TAP fixture helper for `Thomas the Tank Engine (1990)(Alternative Software)`.
2. Added an ignored runtime test that boots the real PAL C64 ROM set, inserts the Thomas TAP archive, drives the real `SHIFT+RUN/STOP` KERNAL autoload path, and proves the decoded screen reaches `FOUND THOMAS`, `LOADING`, and then `READY.` on the following line.
3. Updated the active README and C64 system note so the current software-validation state reflects both `Thinker` and `Thomas`, instead of implying there is only one real-title tape proof.
**Verification:** `cargo test -p runtime-commodore-c64 real_tap_autoload_reaches_thomas_loading_ready_banner -- --ignored --nocapture`, `cargo test -p runtime-commodore-c64`, and `cargo clippy -p runtime-commodore-c64 --all-targets -- -D warnings` should pass locally.
**Next dependency:** the next honest C64 media step is either one more well-chosen TAP title with a stronger end state, or moving on to the 1541/disk path once datasette coverage feels sufficient.

---

## 2026-04-13 — Native shell tape controls are standardized around play, stop, turbo, reset

**Type:** milestone
**Trigger:** After the first C64 native-shell tape pass, the live hotkeys had diverged in the wrong way. Spectrum still used `F5`-`F8`, while C64 had `F9`/`F10` plus an `F11` autoload macro. That made the host layer inconsistent and mixed startup workflow actions with ongoing transport actions.
**Result:** the current native verifier shells now share one temporary host-control layout, while keeping autoload as a startup workflow:
1. `emu198x-spectrum` now uses `F9` start, `F10` stop, `F11` turbo, and `F12` reset instead of the older `F5`-`F8` bindings.
2. `emu198x-c64` keeps `F9` start, `F10` stop, and `F12` reset, but `F11` now toggles cycle-faithful tape turbo instead of triggering a live autoload macro.
3. Added `--turbo-tape` to `emu198x-c64`, matching the existing Spectrum startup option so both verifier shells can launch with tape turbo armed.
4. Left `--autoload-tape` in place for both families as a startup-only host workflow over the real ROM/KERNAL path, not a normal live transport control.
**Verification:** `cargo fmt --all --check`, `cargo test -p emu198x-c64 -p emu198x-spectrum`, `cargo clippy -p emu198x-c64 -p emu198x-spectrum --all-targets -- -D warnings`, and `cargo run -p emu198x-c64 -- --help` / `cargo run -p emu198x-spectrum -- --help` should pass.
**Next dependency:** if a later family needs different raw keys, standardize the host actions instead of forcing the same unmodified key range across machines.

---

## 2026-04-13 — Native C64 tape controls now reuse the same runtime autoload path

**Type:** milestone
**Trigger:** Once the headless C64 path could insert TAP media, drive real `SHIFT+RUN/STOP`, and prove a ROM-backed Thinker load, the native verifier shell still lagged behind. It could boot and import host-side files, but it had no equivalent tape insertion or autoload workflow above the same runtime boundary.
**Result:** `emu198x-c64` now speaks the same tape control language as the headless runner instead of inventing a UI-only path:
1. Added startup `--tape`, `--autoload-tape`, and `--start-tape` handling in the native shell, with the same conflict rules and media loading semantics as `emu198x-script-c64`.
2. Added live `F9` start, `F10` stop, and `F11` autoload controls. `F11` routes through `runtime-commodore-c64::autoload_basic_tape()` over a temporary `HeadlessSession`, so it still drives the real KERNAL prompt and datasette transport instead of synthesizing machine state.
3. Updated the native window title and shell help so tape state is visible (`tape playing`, `tape loaded`, or `no tape`) and the verifier workflow is discoverable.
4. Added CLI coverage for the new tape flags in `emu198x-c64` tests.
**Verification:** `cargo fmt --all`, `cargo test -p emu198x-c64 -p runtime-commodore-c64`, `cargo clippy -p emu198x-c64 --all-targets -- -D warnings`, and `cargo run -p emu198x-c64 -- --help` all pass locally.
**Next dependency:** the next honest C64 tape milestone is a native-shell regression target or additional real software validation above the same TAP path, not a second UI-only media flow.

---

## 2026-04-13 — C64 tape autoload now drives the real KERNAL path, and Thinker reaches post-load READY.

**Type:** milestone
**Trigger:** Once TAP playback was wired through the 6510 and `CIA1 FLAG`, the remaining question was not whether pulses existed but whether the actual C64 ROM workflow could use them. We needed a host helper over the real machine path and at least one concrete software proof above raw transport control.
**Result:** the fresh-workspace C64 now has a real tape-autoload workflow and a ROM-backed tape regression:
1. Added `runtime-commodore-c64::autoload_basic_tape()`, which waits for `READY.`, presses the real `SHIFT+RUN/STOP` KERNAL shortcut, waits for `PRESS PLAY ON TAPE`, and only then starts `tape-1`.
2. Added decoded `screen.text.lines` plus `boot.row` to the C64 query provider, so scripting and future MCP automation can observe text-mode KERNAL states directly instead of relying on screenshots.
3. Wired `--autoload-tape` into `emu198x-script-c64`, keeping it explicitly separate from raw `--start-tape`.
4. Added a new ignored ROM-backed runtime test against the local `Thinker, The (1984)(Atlantis)` TAP archive. The current proof reaches `FOUND THINKER`, `LOADING`, and then a second post-load `READY.` line under the real KERNAL tape path.
**Verification:** `cargo test -p runtime-commodore-c64 -p emu198x-script-c64`, `cargo clippy -p runtime-commodore-c64 -p emu198x-script-c64 --all-targets -- -D warnings`, and `cargo test -p runtime-commodore-c64 real_tap_autoload_reaches_post_load_ready -- --ignored --nocapture` all pass locally. The stronger ignored `Thinker` proof completed in `43.37s`.
**Next dependency:** the next honest C64 tape milestone is either a full end-of-load software regression on a tractable TAP title or native-shell tape insertion/control above the same runtime path.

---

## 2026-04-13 — C64 T64 support lands as a host-side container import, not fake datasette media

**Type:** milestone
**Trigger:** With TAP now living on the real datasette path, the separate `T64` request needed to be handled without eroding the repo’s “no shortcuts” accuracy bar.
**Result:** `T64` support now exists, but on the correct side of the emulation boundary:
1. Added `format-commodore-c64-t64`, which parses `T64` headers and extracts the first loadable entry as a PRG byte stream.
2. Extended the shell asset loader so zipped or plain `.t64` files are recognized as host-side program assets.
3. Updated `runtime-commodore-c64::file_loader` so `--load demo.t64` imports the first loadable archive entry into RAM, just like a host-side PRG convenience path.
4. Kept the boundary explicit in code and docs: `T64` is a container import path under `--load`, not a claim of pulse-timed datasette playback.
**Verification:** `cargo test -p format-commodore-c64-t64 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-shell` and `cargo clippy -p format-commodore-c64-t64 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-shell --all-targets -- -D warnings` both pass locally.
**Next dependency:** if we want richer `T64` handling later, the next honest extension is entry selection and metadata exposure, not pretending the container is equivalent to raw TAP pulse media.

---

## 2026-04-13 — C64 datasette TAP path is live through the 6510 and CIA1 FLAG

**Type:** milestone
**Trigger:** Once the fresh-workspace C64 could boot to `READY.`, emit video/audio, and import host-side `.prg` / `.bas` files, the biggest remaining honesty gap on the software-loading side was obvious: the datasette slot existed in profile metadata, but there was still no real tape medium on the board path.
**Result:** the first honest C64 tape path now exists:
1. Added `format-commodore-c64-tap`, which parses Commodore TAP headers and pulse streams into native machine-cycle pulse lengths rather than flattening them into a fake file loader.
2. Added a board-local datasette component to `machine-commodore-c64`, serialized in snapshots and advanced one `phi2` cycle at a time.
3. Wired datasette sense and motor semantics into the 6510 `$0001` port boundary, so PLAY state and motor control live on the machine path instead of in the runtime.
4. Wired datasette flux changes into `CIA1 FLAG`, which is the real interrupt-visible read path the C64 uses for tape input.
5. Updated `runtime-commodore-c64` and `emu198x-script-c64` so `tape-1` now supports media load plus start/stop transport control and exposes `c64.tape.loaded` / `c64.tape.playing` queries.
6. Kept scope honest: this is TAP only for now. `T64` remains a separate follow-up format and is not being misrepresented as pulse-accurate datasette media.
**Verification:** `cargo fmt --all`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` should pass. New coverage includes TAP parser tests, board tests for 6510 sense + motor gating + CIA1 FLAG delivery, and runtime/runner integration staying green under the broader workspace gates.
**Next dependency:** the next meaningful C64 media step is real software validation on this datasette path, then either native-shell tape insertion/control or the 1541/disk side once the second-computer scope is justified.

---

## 2026-04-13 — Native Spectrum and C64 shells now step in sub-frame slices

**Type:** milestone
**Trigger:** After the first C64 verifier-shell pass, keyboard input still felt soft even once the immediate wake-up fix landed. The remaining problem was host scheduling granularity: both native shells still advanced the machine only one full frame at a time outside turbo mode, so host input was effectively quantized to frame boundaries.
**Result:** both native verifier shells now step the machine in smaller real-time slices while keeping presentation at real frame completion:
1. Added sub-frame scheduling to `emu198x-c64`, so normal execution now advances in 1/8-frame slices instead of whole-frame chunks. Input events are applied on the next slice, but redraw still waits for a real new machine frame.
2. Applied the same change to `emu198x-spectrum` outside tape-turbo mode, so both native shells now share the same lower-latency host loop shape.
3. Kept timing honest: the shells still use the same underlying machine clocks and frame cadence; only host wake-up granularity changed.
4. Added shell-local timing-budget tests so future refactors do not quietly collapse the slice scheduler back to full-frame stepping.
5. The shells remain verifier-grade rather than polished frontends. Subjective input feel is improved but still softer than target, so the remaining work is documented rather than silently implied away.
**Verification:** `cargo fmt --all`, `cargo test -p emu198x-c64 -p emu198x-spectrum`, and `cargo clippy -p emu198x-c64 -p emu198x-spectrum --all-targets -- -D warnings` should pass.
**Next dependency:** if input still feels soft after this, the next place to look is not the host scheduler but machine-side keyboard handling cadence under the ROM/KERNAL paths themselves.

---

## 2026-04-13 — Native shell naming is prefixed and C64 now has a verifier window

**Type:** milestone
**Trigger:** Once the fresh-workspace C64 had live 6502/CIA/VIC-II/SID execution, boot detection, snapshots, and host-side program import, the next practical gap was the same one Spectrum had already exposed: a thin native verifier shell makes real-machine checking dramatically faster. At the same time, the shell naming had drifted toward short `emu-*` package names even though cross-project infrastructure already uses the `emu198x-*` prefix.
**Result:** the native shell surface is now consistent and the C64 has its first windowed runner:
1. Added `emu198x-c64`, a thin `winit` + `pixels` native verifier shell over `runtime-commodore-c64`. It boots PAL or NTSC breadbin profiles through the shared shell/runtime boundary, renders the live VIC-II framebuffer, plays the runtime's mono audio stream, forwards keyboard input into the real matrix, and supports hard reset plus optional startup snapshot/program import.
2. Expanded the C64 host key namespace in `runtime-commodore-c64` so the native shell can drive real function keys, cursor aliases, delete/home, control keys, and common punctuation without inventing a second input path.
3. Renamed the Spectrum native shell from `emu-spectrum` to `emu198x-spectrum` at both the package and crate-path level, so the workspace and public runner name now agree on the `emu198x-*` prefix.
4. Updated the active README/frontend/docs surface so current commands and crate names point at `emu198x-spectrum` and `emu198x-c64` instead of the shorter `emu-*` forms.
**Verification:** `cargo fmt --all`, `cargo test -p emu198x-c64 -p runtime-commodore-c64 -p emu198x-spectrum`, `cargo run -p emu198x-c64 -- --help`, and `cargo run -p emu198x-spectrum -- --help` should pass before the full workspace gates.
**Next dependency:** with both Spectrum 48K and the C64 now having working native verifier shells, the next honest C64 step is real media hardware rather than more shell scaffolding.

---

## 2026-04-13 — C64 SID is live and the runtime now emits audio

**Type:** milestone
**Trigger:** Once the fresh-workspace C64 could boot to `READY.` and import host-side programs, the biggest remaining board-level dishonesty was obvious: the SID was still only a register shadow, so the machine claimed a C64 audio path without actually modelling or emitting one.
**Result:** the fresh-workspace C64 now has a real SID and end-to-end runtime audio:
1. Added `mos-sid-6581` to the workspace with the archived three-voice oscillator, ADSR envelopes, state-variable filter, and downsampled audio buffer, including its 9-chip tests.
2. Replaced `machine-commodore-c64`'s shadowed SID register array with a live `Sid6581`, clocked once per `phi2` tick and serialized as part of machine snapshots.
3. Kept the board boundary honest: `$D400-$D7FF` now routes to the real SID register bus, and the machine exposes a real mixed audio buffer instead of pretending that writes alone constitute sound support.
4. Updated `runtime-commodore-c64` so frame execution now also drains the SID buffer and emits mono audio packets through the shared shell audio sink.
5. Tightened the fresh-workspace summaries so the C64 no longer describes SID as shadowed in the runtime/profile/readme layer.
**Verification:** `cargo test -p mos-sid-6581 -p machine-commodore-c64 -p runtime-commodore-c64`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass. New coverage includes C64 board tests for live SID register writes and generated audio samples, plus a runtime test proving one frame now emits both RGBA video and mono audio packets.
**Next dependency:** the next C64 step is still not more shell work. It is either a minimal native verifier shell over this now-audible runtime, or the first honest media path once the corresponding hardware is modelled far enough to deserve the claim.

---

## 2026-04-13 — C64 headless runner now imports PRG and plain-text BASIC

**Type:** milestone
**Trigger:** Once the fresh-workspace C64 had a booted runtime, snapshots, and a headless runner, the next practical gap was software injection. The immediate need was developer-grade program loading, including plain-text BASIC source rather than only pre-tokenised artifacts.
**Result:** the C64 host workflow now has an explicit software-import path above the runtime boundary:
1. Added `format-commodore-c64-prg`, which parses PRG files, imports them into RAM through a narrow `RamAccess` trait, and relinks BASIC pointers when the load address is `$0801`.
2. Added `format-commodore-c64-bas`, which tokenises UTF-8 plain-text Commodore BASIC source into PRG bytes without pretending that source import is a shared cross-family format concern.
3. Added `runtime-commodore-c64::file_loader`, which treats `.prg` and `.bas` as host-side convenience imports over the live machine instead of as emulated media devices.
4. Wired `--load PATH` into `emu198x-script-c64`. The runner now waits for `READY.` automatically before importing, so BASIC/KERNAL startup does not immediately overwrite the injected program.
5. Kept the boundary honest in the docs: this is not tape or disk support, and Spectrum will need its own family-specific text-import path later because tokenisation rules differ.
**Verification:** `cargo test -p format-commodore-c64-prg -p format-commodore-c64-bas -p machine-commodore-c64 -p runtime-commodore-c64 -p emu198x-script-c64`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass. A real runner proof also succeeds locally with `emu198x-script-c64 --load /tmp/emu198x-demo.bas --save-snapshot /tmp/emu198x-demo.c64.pst`.
**Next dependency:** the next honest C64 software path is real media support, starting with PRG-friendly disk or tape workflows only when the corresponding hardware path is modeled directly enough to deserve the claim.

---

## 2026-04-13 — C64 snapshots and the first fresh-workspace headless runner land

**Type:** milestone
**Trigger:** Once the PAL C64 could boot real ROMs through the shared runtime, the next useful host-facing step was a thin runner and honest snapshot support. The runtime could not claim save states safely until the chip state serialization itself was complete.
**Result:** the fresh workspace now has a usable C64 automation path instead of just an internal runtime:
1. Added `emu198x-script-c64` as the first fresh-workspace C64 headless runner, with ROM directory resolution, PAL/NTSC model selection, boot waits, shared JSON script execution, PNG screenshots, and snapshot load/save.
2. Added real runtime snapshot import/export to `runtime-commodore-c64` using the same `postcard` envelope pattern as the Spectrum runtime.
3. Added machine-local `C64Snapshot` and `C64MemorySnapshot` state capture/restore in `machine-commodore-c64`, so the runtime is not trying to serialize board state ad hoc.
4. Tightened snapshot fidelity by serializing the 6502's in-flight cycle state and the VIC-II framebuffer instead of silently dropping them at snapshot boundaries.
5. Updated profile metadata and scripting docs so C64 now advertises runtime snapshot support honestly, while still leaving media support out of scope.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass. The new C64 runtime test exercises snapshot round-tripping mid-cycle, and the runner can boot or restore from the local ROM set at `~/.emu198x/roms/commodore-c64`.
**Next dependency:** the next honest C64 step is media and software workflows above this runner boundary, not more shell scaffolding.

---

## 2026-04-13 — C64 runtime now emits frames and detects the READY. boot state

**Type:** milestone
**Trigger:** Once the fresh C64 machine could boot real ROMs to `READY.` inside the machine crate, the next honest step was to push that proof through the shared shell boundary instead of leaving C64 as metadata plus chip tests.
**Result:** the fresh workspace now has its first real C64 runtime surface:
1. Added `C64Runtime` in `runtime-commodore-c64`, backed by the live `machine-commodore-c64` board and the real BASIC/KERNAL/CHARGEN firmware set.
2. `run_until()` now advances the C64 in authoritative `phi2` cycles and emits RGBA framebuffer packets to the shared host sinks, so the family is no longer “catalogue only.”
3. Added `C64SessionQueryProvider` with a minimal boot/query namespace: `boot.detected`, `boot.reason`, `boot.offset`, plus current raster/IRQ/BA state.
4. Added two ROM-backed proofs of the visible boot path:
   - an ignored machine test that boots the PAL C64 and finds `READY.` in screen RAM
   - an ignored runtime test that drives the same ROM set through `run_until()` and resolves `boot.detected = true`
5. Tightened the profile honesty: PAL now reports `SupportTier::Boots`; NTSC stays at `Research` until it is verified on the same footing.
**Verification:** `cargo test -p machine-commodore-c64 boots_kernal_to_ready_prompt -- --ignored --nocapture`, `cargo test -p runtime-commodore-c64 query_provider_detects_ready_on_real_pal_boot -- --ignored --nocapture`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` all pass locally.
**Next dependency:** the next C64 step is a thin runner above this runtime, then PRG/media support and SID if the booted machine path stays stable under real software.

---

## 2026-04-13 — Live VIC-II replaces the C64 board shadow registers

**Type:** milestone
**Trigger:** After the 6502 and both CIAs were live, the main remaining board-local fake on the C64 path was the VIC-II. Keeping register shadows any longer would have stalled the bring-up at exactly the point where BA, raster IRQs, and framebuffer ownership start to matter.
**Result:** the fresh-workspace C64 now owns a real VIC-II chip instead of a board-local placeholder:
1. Added `mos-vic-ii` with the archived raster state machine, badline detection, sprite BA lead-in, register bus, raster IRQs, light-pen latch, visible framebuffer, and text/bitmap/sprite render paths.
2. Implemented `mos_vic_ii::VicMemory` for `C64Memory`, so VIC-visible banked RAM, character ROM windows, and colour RAM now flow through the real machine memory router rather than through chip-local test hooks.
3. Reworked `machine-commodore-c64` to own a live `Vic`, route CIA2 bank selection into it, OR its IRQ into the CPU IRQ line, and gate CPU reads with `BA -> RDY` instead of assuming the CPU is always ready.
4. Kept the scope honest: SID is still shadowed, sprite and text rendering use the archived batch-fetch compromise, and the fresh workspace still does not claim a booted KERNAL. But the board no longer fakes the VIC side of the machine.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass, including the new `mos-vic-ii` unit suite and C64 board tests for VIC raster IRQ and badline BA read stalling.
**Next dependency:** the next C64 step is to keep wiring toward a real boot path: tighten the VIC/CPU interaction where the new board loop exposes gaps, then bring in SID and the first runtime shell once the machine can show meaningful KERNAL output.

---

## 2026-04-13 — Live CIA chips replace the C64 board shadows

**Type:** milestone
**Trigger:** The C64 board had reached real CPU-driven execution, but keyboard scan, bank selection, and interrupt sources were still hand-rolled shadows. The next honest step had to replace those with live chip behaviour instead of adding more board-local fakes.
**Result:** the fresh C64 path now owns real CIA chips:
1. Added `mos-cia-6526` with live ports, DDR masking, Timer A/B countdown, ICR mask/status handling, TOD divider logic, serial-shift completion tracking, and IRQ pin output.
2. Replaced `machine-commodore-c64`'s CIA shadow latches with two live `Cia6526` instances. CIA1 now owns keyboard-facing port state and IRQ generation; CIA2 now owns VIC bank-select port state and the NMI-side interrupt source.
3. Reworked the board tick order so keyboard scan feeds CIA1 before the chip ticks, both CIAs tick once per `phi2`, VIC bank selection is refreshed from CIA2's live PA pins, and the 6502 sees real CIA-driven `irq`/`nmi` levels.
4. Kept the scope honest: VIC-II and SID are still shadowed, but the machine no longer fakes the CIA side of the board.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass, including C64 board tests proving CIA keyboard scan, bank selection, and CPU interrupt-line routing.
**Next dependency:** the next C64 step is the VIC-II, because that is now the main missing live chip between this board loop and a real KERNAL boot path.

---

## 2026-04-13 — Fresh-workspace C64 now has a real 6502 and CPU-driven board loop

**Type:** milestone
**Trigger:** The C64 board substrate had real banking, keyboard scan state, and timing, but it was still only a timed board shell. The next honest step had to be a real processor on the bus.
**Result:** the fresh workspace now has the first CPU-driven C64 slice instead of only static board behaviour:
1. Added `mos-6502` as a standalone cycle-accurate pin-level CPU crate with opcode decode, per-cycle bus scheduling, decimal-mode support, and core execution tests.
2. Reworked `machine-commodore-c64` to own a real 6502, reset it through the KERNAL reset vector, and drive one actual CPU bus transaction per `phi2` cycle over the existing 6510-style memory banking and I/O shadows.
3. Kept the board scope honest: IRQ/NMI/RDY sources are still placeholders until live CIA/VIC models arrive, but the CPU is now executing through the real memory map rather than through synthetic state changes.
4. Added machine-level proofs that the board boots through the reset vector to the first opcode fetch and can execute `LDA`/`STA` through the board bus into RAM and I/O-visible register space.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass with the new `mos-6502` crate and the CPU-driven C64 board loop.
**Next dependency:** the next C64 step is to replace the current CIA/VIC shadows with live chip crates and start surfacing real interrupt, bank-select, and bad-line behaviour through the same board loop.

---

## 2026-04-13 — C64 machine substrate now has real banking, keyboard, and timing

**Type:** milestone
**Trigger:** The profile/timing bootstrap made the C64 family visible in the fresh workspace, but the next useful slice had to stop being metadata and start becoming board behaviour the future boot path can actually stand on.
**Result:** the new `machine-commodore-c64` crate now owns the first durable machine substrate for the fresh workspace:
1. Added a real 6510 memory subsystem with `$00`/`$01` port semantics, BASIC/KERNAL/character ROM visibility rules, colour RAM, and VIC-visible character ROM banking.
2. Added the 8×8 keyboard matrix as a pure active-low scan surface.
3. Added a minimal board loop with `phi2` cycle counting, raster/frame progression for PAL and NTSC, CIA-side keyboard scan latches, CIA2-driven VIC bank selection, and shadowed VIC/SID register storage for I/O-visible accesses.
4. Kept the scope honest: no 6502 core, no live VIC-II/CIA/SID behaviour, and no runtime yet. This is substrate only, not a runnable C64.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass with the new crate wired into the workspace.
**Next dependency:** the next real C64 step is the processor/chip wave above this substrate, starting with the 6502/6510 side and then replacing the current CIA/VIC register shadows with live chip models.

---

## 2026-04-13 — C64 bootstrap lands as timing and profile truth only

**Type:** milestone
**Trigger:** Spectrum 48K is now strong enough that the next architectural test should be a second family, but the repo also carries stale C64 status documents from older workspaces that would make it too easy to overstate progress.
**Result:** the fresh workspace now has an honest C64 bootstrap instead of another archive-shaped mirage:
1. Added `common-commodore-c64` with baseline PAL and NTSC breadbin timing constants: φ2 clock, raster geometry, CIA TOD dividers, and archived VIC-II capture window dimensions.
2. Added `runtime-commodore-c64` as the new family catalogue crate. It currently exposes PAL and NTSC breadbin research-tier profiles only, with three required ROMs and baseline tape/disk/cartridge slots.
3. Chose `phi2-cycle` as the authoritative profile clock for now instead of inventing a master-clock contract before the fresh workspace has a live VIC-II/6510 timing loop to anchor it.
4. Added a historical warning at the top of `wiki/systems/commodore-c64.md` so the old archived implementation-status text stops reading like current repo truth.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass once the new crates are wired in.
**Next dependency:** the next real C64 slice is no longer more metadata. It is the first machine-facing substrate: 6510/port banking boundaries, keyboard matrix state, and the minimal VIC-II/CIA-aware clock model that a future boot path can actually stand on.

---

## 2026-04-13 — Shared boolean query waits land, with a Spectrum tape-stop alias

**Type:** milestone
**Trigger:** After the UI got cycle-faithful tape turbo, the next practical automation gap was obvious: scripts and headless runs still had no clean way to block on a tape load finishing except by guessing a frame count.
**Result:** the shared shell now has a reusable boolean wait primitive, and the Spectrum headless runner exposes the tape-stop case directly:
1. Added `HeadlessSession::wait_for_query_bool(path, expected, max_frames)` plus a typed `QueryBoolWaitResult`.
2. Added `wait_for_query_bool` to the shared JSON script surface so automation can wait on boolean machine state without inventing ad hoc polling loops.
3. Added `--wait-for-tape-stop N` to `emu198x-script-spectrum` as a Spectrum-specific alias for waiting until `spectrum.tape.playing == false`.
4. Kept the alias ordered after autoload and script execution, so one command line can autoload a tape, wait for playback to finish, and then continue with any extra frame or capture steps.
**Verification:** `cargo test -p emu198x-shell -p emu198x-script-spectrum`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass.
**Next dependency:** if we want to push this pattern further, the next useful addition is a more general query-equality wait only when a concrete workflow needs it. Right now boolean state and text containment cover the real use cases.

---

## 2026-04-13 — Spectrum verifier shell now has cycle-faithful tape turbo

**Type:** milestone
**Trigger:** Once tape autoload existed, the next quality-of-life gap was purely host-side: the native Spectrum shell still ran at wall-clock speed during long tape loads, even though the project explicitly rejects fake instant-load shortcuts.
**Result:** `emu-spectrum` now has a tape-only turbo mode that preserves exact machine execution:
1. Added `--turbo-tape` at launch and `F8` at runtime to arm or toggle tape turbo in the native verifier shell.
2. Turbo mode only engages while the tape is actually playing. When active, the host loop stops sleeping and runs bounded batches of real Spectrum frames as fast as the machine can execute them.
3. The implementation does not skip pilot tones, alter TZX/TAP semantics, or bypass ROM/tape behaviour. It is only a host scheduling change over the same machine path.
4. The window title now reports when turbo is armed or active so manual verification stays legible.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p emu-spectrum`, and `cargo test --workspace` all pass.
**Next dependency:** if we want more tape QoL after this, the next honest step is a “wait until tape stops / wait until query matches” convenience above the current headless session surface, not any form of instant loader.

---

## 2026-04-13 — Spectrum tape autoload is now a real host workflow

**Type:** milestone
**Trigger:** After Manic Miner and Jet Set Willy were both loading end to end, the next obvious friction point was that every real tape workflow still depended on hand-authored `LOAD ""` key choreography in tests or manual typing in the UI.
**Result:** the fresh Spectrum path now has a reusable tape autoload helper above the runtime boundary instead of burying that sequence inside ignored tests:
1. Added `runtime-sinclair-zx-spectrum::autoload_basic_tape()`, which waits for the 48K boot banner, exposes the BASIC prompt if needed, types the real `LOAD ""` keyword sequence through the ROM editor, and then starts tape transport on `tape-1`.
2. Kept that helper honest: it operates through `HeadlessSession`, shared input events, and normal media transport commands. It does not patch ROM code, skip leader tones, or bypass tape decoding.
3. Added `HeadlessSession::into_machine()` so the native verifier shell can reuse the same startup workflow without introducing a parallel machine-control path.
4. Wired `--autoload-tape` into both `emu198x-script-spectrum` and `emu-spectrum`, and added clear runner-level errors for missing tape media or conflicting `--play-tape` usage.
5. Replaced the duplicated hand-typed `LOAD ""` sequence in the ignored Manic Miner and Jet Set Willy ROM-backed regressions with the new helper.
**Verification:** `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo test -p runtime-sinclair-zx-spectrum spectrum_boots_and_loads_manic_miner_from_zipped_tzx -- --ignored --nocapture`, and `cargo test -p runtime-sinclair-zx-spectrum spectrum_boots_and_loads_jet_set_willy_from_zipped_tzx -- --ignored --nocapture` all pass locally.
**Next dependency:** the next quality-of-life improvement worth doing is turbo loading as uncapped exact execution on the real machine path, not an instant-load shortcut.

---

## 2026-04-13 — Jet Set Willy joins the ROM-backed Spectrum software regressions

**Type:** milestone
**Trigger:** After Manic Miner became a real end-to-end tape regression, the next useful Spectrum software target needed to cover a different post-load path instead of just another early title-screen success.
**Result:** `runtime-sinclair-zx-spectrum` now has a second ignored ROM-backed tape regression for the original Jet Set Willy TZX:
1. Added a local fixture lookup for `Jet Set Willy (1984)(Software Projects).zip`.
2. Added an ignored test that boots the real 48K ROM, types `LOAD ""`, starts the tape, and waits for the copy-protection code screen text `Enter Code at grid location`.
3. Verified that this is a better automated target than the cracked image for now. The original build exposes a stable decoded text screen; the cracked build reaches a stopped post-load state, but its stylized title screen is not currently a strong text-query target.
**Verification:** `cargo test -p runtime-sinclair-zx-spectrum spectrum_boots_and_loads_jet_set_willy_from_zipped_tzx -- --ignored --nocapture`, `cargo test -p runtime-sinclair-zx-spectrum`, and `cargo clippy -p runtime-sinclair-zx-spectrum --all-targets -- -D warnings` all pass locally.
**Next dependency:** Knight Lore and Atic Atac are still good manual/UI targets, but they need either a later post-load assertion or a different observable than decoded text before they become strong headless regressions.

---

## 2026-04-13 — Spectrum verifier shell now plays live beeper and tape audio

**Type:** milestone
**Trigger:** Once the Spectrum UI shell could boot and load real tape software, the next gap was obvious during manual verification: the frontend was still discarding the runtime’s audio packets, so there was no beeper output and no tape screech while loading.
**Result:** `emu-spectrum` now owns a real host-side audio path over the existing machine contract:
1. Added a `cpal`-backed audio sink in the frontend crate, using the default host output device and a bounded host-side queue.
2. Threaded the live audio sink through `SpectrumRunner::run_frame` instead of `NullAudioSink`, without changing the runtime or machine audio contract.
3. Kept conversion policy in the host layer: mono Spectrum packets are duplicated across the device channel count, and output-rate mismatch is handled by a small frontend resampler rather than by touching machine timing.
4. Added frontend-local tests covering mono-to-stereo duplication and the simple downmix/resample path.
**Verification:** `cargo test -p emu-spectrum`, `cargo clippy -p emu-spectrum --all-targets -- -D warnings`, and `cargo fmt --all --check` all pass.
**Next dependency:** the next useful frontend follow-up is host-configurable audio/input policy, not more machine-facing work in this area.

---

## 2026-04-13 — Spectrum tape loading now handles TZX pauses as timing spans, and Manic Miner loads end to end

**Type:** milestone
**Trigger:** Manual Spectrum verification had reached the first real software path, and Manic Miner was failing late with `R Tape loading error, 20:6` after the striped loader phase. That ruled out trivial command-entry mistakes and pointed toward the real tape path.
**Result:** the fresh Spectrum tape stack now models the missing semantics cleanly enough to load the zipped local Manic Miner fixture under the real 48K ROM:
1. Replaced the old flat pulse-only tape representation with a shared timing-span stream in `common-sinclair-zx-spectrum`. The tape player now supports edge-delimited pulses, held-level spans, and explicit stop markers, which are needed for TZX pause and direct-recording semantics.
2. Updated `format-sinclair-zx-spectrum-tzx` to parse TZX blocks into that richer span stream, including direct recording, pause blocks, signal-level directives, and loop expansion.
3. Corrected the machine-level tape override on port `$FE` bit 6 to follow the repo’s own Spectrum docs when tape is connected: external EAR high now reads as bit 6 low, and EAR low reads as bit 6 high.
4. Re-ran the real 48K ROM + zipped Manic Miner path headlessly and confirmed that the title now loads end to end. The successful run reached `screen.text.lines` containing `MANIC MINER` on row 19 after `10,672` frames of tape playback.
5. Added a new ignored local regression in `runtime-sinclair-zx-spectrum` that boots the real 48K ROM, types `LOAD ""` with the timing the Spectrum editor actually accepts, starts tape transport, and waits for the Manic Miner title screen from the zipped TZX fixture.
**Verification:** `cargo test -p common-sinclair-zx-spectrum`, `cargo test -p format-sinclair-zx-spectrum-tzx`, `cargo test -p machine-sinclair-zx-spectrum-48k`, `cargo test -p runtime-sinclair-zx-spectrum`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo test -p runtime-sinclair-zx-spectrum spectrum_boots_and_loads_manic_miner_from_zipped_tzx -- --ignored --nocapture` all pass locally with the required ROM and tape assets present.
**Next dependency:** the next useful pass is to turn the brittle Spectrum command-entry sequence into a higher-level host helper for scripting and MCP, so media autoloading does not depend on hand-authored key choreography.

---

## 2026-04-13 — Minimal native Spectrum verifier shell is live

**Type:** milestone
**Trigger:** The headless Spectrum path had reached the point where the next bottleneck was not another query or script primitive. It was the lack of a human-verifiable native frontend for real-time manual checking. At the same time, zipped local assets were already becoming normal, so the first UI needed to speak that host-side asset boundary instead of bypassing it.
**Result:** the repo now has a first native UI runner in `crates/emu-spectrum`:
1. Added `emu-spectrum`, a thin `winit` + `pixels` desktop shell over the existing 48K runtime. It boots the same `Spectrum48kRuntime`, renders the real indexed framebuffer in a native window, and drives the machine at Spectrum frame cadence instead of inventing a separate execution path.
2. Wired the UI shell to the shared asset boundary. `--rom` and `--tape` now accept plain files or zip archives with one matching candidate, using the same host-side asset loading code as the headless path.
3. Added practical live controls: host keyboard mapping for the Spectrum matrix, `Esc` to quit, `F5` hard reset, `F6` tape start, `F7` tape stop, cursor-key combos via `Caps Shift`, and `Alt` as `Symbol Shift`.
4. Kept the earlier ZIP-media work but narrowed the unfinished Manic Miner experiment back to a safe local smoke test. The fresh runtime now has an ignored test that verifies the zipped Manic Miner TZX fixture loads into the tape slot, without pretending the full autoload path is solved yet.
5. Corrected the frontend docs so they no longer claim that all family frontends already exist, while also no longer pretending there is no native frontend at all.
**Verification:** `cargo run -p emu-spectrum -- --help`, `cargo test -p emu-spectrum`, `cargo test -p emu198x-shell`, `cargo test -p emu198x-script-spectrum`, `cargo test -p runtime-sinclair-zx-spectrum`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass.
**Next dependency:** the highest-value follow-up is to use this real windowed shell to debug the first honest software-loading path, especially the `LOAD ""` and Manic Miner tape workflow, rather than guessing from headless snapshots alone.

---

## 2026-04-13 — Boot detection is now a reusable headless workflow, not just a query

**Type:** milestone
**Trigger:** Once `boot.detected` and `screen.text.lines` were queryable, the next gap was obvious: host tooling still had to hand-roll frame polling to use them. That was enough for experimentation, but not enough for scripting, CLI automation, or ROM-backed workflow tests.
**Result:** the shared headless surface now has a first real boot-wait primitive, and Spectrum uses it for an honest keyboard-at-prompt system test:
1. Added `HeadlessSession::wait_for_boot(max_frames)` in `emu198x-shell`. It polls the generic `boot.detected` query once per native frame, returns a structured result (`frames`, `reached`, `reason`, `row`) on success, and fails with a typed timeout that carries the last `boot.reason`.
2. Added `wait_for_boot` as a shared JSON script step with a matching structured observation, so automation can block on boot semantically instead of hard-coding frame counts.
3. Added `--wait-for-boot N` to `emu198x-script-spectrum`, layered on the same shared helper. The runner now reports that wait in its JSON observations and fails explicitly when boot does not become visible within the requested frame budget.
4. Added shell and script tests around the new helper using a dummy query provider, plus a runner test proving zero-ROM boot waits time out instead of silently pretending success.
5. Added a new ignored ROM-backed Spectrum test that uses `wait_for_boot(250)`, then injects real key events at the BASIC prompt and verifies the decoded text screen changes from the prompt line to `NEW...` after pressing `A`. That is the first fresh-workspace proof that boot wait, keyboard injection, ROM code, and decoded screen text all work together on one real machine path.
**Verification:** `cargo test -p emu198x-shell`, `cargo test -p emu198x-script-spectrum`, `cargo test -p runtime-sinclair-zx-spectrum`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should all pass. The new ROM-backed prompt test also passes locally with `cargo test -p runtime-sinclair-zx-spectrum spectrum_boot_wait_and_prompt_input_change_decoded_text -- --ignored --nocapture` when the 48K ROM is present.
**Next dependency:** the next high-value Spectrum system check is the same pattern applied to tape: boot, optionally wait for boot, start tape transport, and verify one concrete loaded-software path instead of stopping at ROM prompt interaction.

---

## 2026-04-13 — Spectrum boot detection is now queryable through the shared headless surface

**Type:** milestone
**Trigger:** After the machine-level timing checks landed, the next useful system-facing hook was not more synthetic tracing. It was a real boot-detected state that scripting, MCP, and future autoloading can consume directly instead of hard-coding frame counts or relying on screenshots.
**Result:** the Spectrum runtime now exposes generic machine-semantic boot and screen-text queries above the shared shell surface:
1. Added `screen.text.rows`, `screen.text.cols`, and `screen.text.lines` to the Spectrum query provider. For the 48K machine today these are derived by decoding the bitmap screen against the resident ROM font at `$3D00`, which is accurate for ROM text screens such as the boot banner.
   The ROM copyright glyph is normalized to Unicode `©` so the decoded line remains one text cell wide.
2. Added `boot.detected`, `boot.reason`, and `boot.row` on top of that decoded text surface. The current 48K boot detector reports success when the ROM copyright banner `(C) 1982 Sinclair Research Ltd` is visible on the decoded text screen.
3. Kept the implementation in the family runtime rather than the shared shell contract. The shell still owns generic query transport, while the Spectrum runtime owns the machine-specific meaning of “booted” and “text screen”.
4. Added synthetic unit tests for the bitmap-to-text decoder and boot-status parser, plus a new ignored ROM-backed runtime test that boots the real 48K ROM for 200 frames and proves that `boot.detected` becomes `true` with the expected reason and decoded text row.
5. Updated the scripting, MCP, and observability docs so the current repo stops describing `boot_detected` and `get_screen_text` as only future dedicated tools. Spectrum can now answer those concepts today through `query`.
**Verification:** `cargo test -p runtime-sinclair-zx-spectrum`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should all pass. The ROM-backed proof also passes locally with `cargo test -p runtime-sinclair-zx-spectrum spectrum_query_provider_detects_booted_48k_rom -- --ignored --nocapture` when the 48K ROM is present at `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
**Next dependency:** the next useful step is to let host workflows consume this directly: a simple boot-wait helper in the Spectrum runner or shared session layer, followed by the same query contract being adopted by the next runnable family.

---

## 2026-04-13 — Spectrum 48K machine loop now has normal-CI contention proofs

**Type:** milestone
**Trigger:** After the Z80 branch/contention fix was verified against FUSE, Tom Harte, `zexdoc`, and `zexall`, the remaining risk moved back up one level: we still needed proof that the fresh Spectrum 48K machine loop was actually exposing those bus patterns through the ULA-driven clocking model, not just inside the CPU in isolation.
**Result:** `machine-sinclair-zx-spectrum-48k` now has deterministic timing and trace coverage at machine level:
1. Added exact stepping helpers on the concrete 48K machine for half-cycles, T-states, and current frame-local T-state position. This stays below the shared runtime contract, but gives the machine crate a reusable deterministic timing surface for verification work.
2. Added Spectrum machine-loop trace helpers in the test module that record the real bus state seen after each CPU half-cycle under the ULA-driven outer loop.
3. Added a contention integration test proving that active-display fetches from contended RAM insert real CPU-clock gaps, while the same fetches from uncontended RAM do not.
4. Added machine-level regression tests for not-taken `DJNZ` and not-taken `JR cc` showing the fresh Spectrum loop now exposes the correct fallthrough behaviour:
   - a contended `PC` cycle with `MREQ` active and no read strobe
   - no displacement-byte memory read on the not-taken path
5. Exposed `spectrum.machine.tstate_in_frame` through the Spectrum query provider so headless scripts and future tooling can observe the machine timing state directly instead of inferring it from half-cycles.
**Verification:** `cargo test -p machine-sinclair-zx-spectrum-48k`, `cargo test -p runtime-sinclair-zx-spectrum`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass.
**Next dependency:** the next useful step is a stronger ROM- or software-driven system check that uses this verified timing surface to validate real Spectrum execution under display contention, rather than only synthetic instruction traces.

---

## 2026-04-12 — FUSE exact-trace verification is now live, and relative-branch contention is corrected

**Type:** milestone
**Trigger:** The first FUSE harness established final-state compatibility, but the exact event list still diverged because the fresh-workspace Z80 was not yet modeling every control-flow contention path the way FUSE records them. The decisive mismatch was `DJNZ` not taken: we were reading the displacement byte, while FUSE correctly showed a contended `PC` cycle without a read strobe.
**Result:** the fresh-workspace Z80 now has a stronger timing model and the FUSE harness now checks the whole instruction trace instead of only the end state:
1. Added exact event capture in `crates/zilog-z80/src/z80_fuse_tests.rs` for `MR`, `MW`, `MC`, `PR`, `PW`, and `PC`, including internal contention and port-timing phases.
2. Kept FUSE-specific address-selection logic in the harness instead of teaching production code FUSE-only heuristics. That preserves the chip model boundary while still comparing against the full reference trace.
3. Fixed a real Z80 timing bug in the core: not-taken `JR cc,e` and `DJNZ e` now use a contended `PC` cycle without a read strobe, instead of incorrectly reading the displacement byte.
4. Added an explicit `ContendPc` M-step and corresponding Z80 phase so the machine-visible bus behaviour matches the reference timing instead of faking the cycle as generic internal delay.
5. Re-ran the full local verification stack after the fix:
   - **FUSE:** `1,350 / 1,356` exact, `6` accepted disagreements, `0` unexpected, now on full event trace plus final state
   - **Tom Harte:** `1,604,000 / 1,604,000`
   - **ZEXDOC:** `67 / 67` checkpoints, `0` errors
   - **ZEXALL:** `67 / 67` checkpoints, `0` errors
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo test -p zilog-z80 run_fuse_z80_reference_suite -- --ignored --nocapture`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zexdoc-after-branch cargo test --release -p zilog-z80 --test zex_tests run_zexdoc -- --ignored --nocapture`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zexall-after-branch cargo test --release -p zilog-z80 --test zex_tests run_zexall -- --ignored --nocapture`, and `cargo test -p zilog-z80 --test single_step_tests run_all -- --ignored --nocapture` all pass.
**Next dependency:** the CPU-side reference loop is now strong enough that the next high-value work is back at machine level: use the verified branch/contention behaviour under real Spectrum software and keep pulling timing bugs out of full-machine execution rather than synthetic CPU traces alone.

---

## 2026-04-12 — Fresh-workspace FUSE Z80 compatibility harness is established

**Type:** milestone
**Trigger:** After Tom Harte, `zexdoc`, and `zexall` were all passing in the fresh workspace, the next missing external verification pass was FUSE. The older repo claimed a five-failure FUSE result, but there was no current harness in this workspace and no reason to trust that old count without rerunning it here.
**Result:** the fresh workspace now has a local FUSE Z80 harness in `crates/zilog-z80/src/z80_fuse_tests.rs`:
1. Added a parser for the local FUSE `tests.in` and `tests.expected` fixture files, including register state, final T-state counts, expected memory deltas, and the event list for future use.
2. Added a chip-level runner that initializes the FUSE DEADBEEF memory background, applies fixture memory overlays, runs the half-cycle Z80 until the real post-instruction boundary, and compares final registers, memory, and T-state totals.
3. Established the current fresh-workspace FUSE baseline: **1,350 / 1,356 exact matches, 6 accepted disagreements, 0 unexpected**.
4. Made the six accepted disagreements explicit in the harness so any new FUSE drift or changed mismatch pattern fails the test immediately instead of hiding behind a generic percentage.
5. Corrected the stale repo narrative: the fresh workspace does not currently show the old "five failures" story. It shows six named disagreements, including an additional `INDR` `WZ` difference.
**Verification:** `cargo test -p zilog-z80 run_fuse_z80_reference_suite -- --ignored --nocapture` passes with `1,350 / 1,356 exact, 6 accepted disagreements, 0 unexpected`. `cargo clippy -p zilog-z80 --tests -- -D warnings` passes.
**Next dependency:** if we need FUSE-level event-trace comparison rather than final-state compatibility, the remaining work is not parser or fixture setup. It is trace instrumentation for internal `MC` / `PC` timing phases that are not fully visible on the public pin surface.

---

## 2026-04-12 — ZEX snapshots, cached resume, and full suite reruns are established

**Type:** milestone
**Trigger:** After adding checkpoint-targeted reruns, the remaining problem was practicality. Late checkpoint reruns still replayed the suite from reset, and the first full fresh-workspace `zexdoc` release run exposed two harness edge cases at real suite completion that the shorter tests had not covered.
**Result:** `crates/zilog-z80/tests/zex_tests.rs` now supports practical local ZEX iteration and has been proven against full suite runs:
1. Added a local snapshot format for the ZEX harness under `target/zex-snapshots` (or `EMU198X_ZEX_SNAPSHOT_DIR`) that stores the Z80 state, 64K CP/M memory image, completed checkpoint list, and cycle count.
2. Targeted checkpoint runs now resume from the highest cached checkpoint below the requested target instead of always restarting from reset. Full-suite runs also resume from the highest cached checkpoint when available.
3. Added fast harness tests covering snapshot round-trips, highest-checkpoint selection, completion-line handling, and extra summary output after the final checkpoint.
4. Fixed the two harness bugs discovered by real end-to-end runs:
   - `Tests complete` must count as completion even when it does not contain `OK`.
   - extra post-checkpoint summary output after checkpoint 67 must not be treated as a parser error.
5. Re-ran both exerciser suites end-to-end in release mode in the fresh workspace, and both now pass:
   - `zexdoc`: 67 checkpoints, 67 OK, 0 ERROR
   - `zexall`: 67 checkpoints, 67 OK, 0 ERROR
**Verification:** `cargo test -p zilog-z80 --test zex_tests`, `cargo clippy -p zilog-z80 --test zex_tests -- -D warnings`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zex-resume-proof EMU198X_ZEX_CHECKPOINT=1 cargo test -p zilog-z80 --test zex_tests run_zexdoc_checkpoint -- --ignored --nocapture`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zex-resume-proof EMU198X_ZEX_CHECKPOINT=2 cargo test -p zilog-z80 --test zex_tests run_zexdoc_checkpoint -- --ignored --nocapture`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zex-release-full cargo test --release -p zilog-z80 --test zex_tests run_zexdoc -- --ignored --nocapture`, and `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zexall-release-full cargo test --release -p zilog-z80 --test zex_tests run_zexall -- --ignored --nocapture` all pass.
**Performance note:** The release build matters here. The resumed checkpoint-2 `zexdoc` run took about `129s` in debug and `17.30s` in release from the same cached checkpoint.
**Next dependency:** FUSE is now the next external Z80 verification pass worth re-establishing in the fresh workspace, using the same “reference, not oracle” adjudication rule against Tom Harte and the now-passing ZEX suites.

---

## 2026-04-12 — ZEX harness now supports checkpoint-targeted reruns

**Type:** milestone
**Trigger:** After wiring the local ZEX binaries back into the fresh workspace, the remaining weakness in the harness was failure granularity. A failing `zexdoc` or `zexall` run still only told us that some point in a long exerciser program had gone wrong, not which labelled block had failed.
**Result:** `crates/zilog-z80/tests/zex_tests.rs` now treats the exerciser's own progress output as ordered checkpoints instead of raw console text:
1. Added the canonical 67 ZEX block labels as an explicit ordered checkpoint list, sourced from the local archived ZEX source files but now kept in-repo so the harness does not depend on those external source trees at runtime.
2. Reworked the CP/M console capture to preserve line structure from BDOS output, parse `OK` / `ERROR` status at line completion time, and record per-checkpoint metadata including index, label, and cycle count.
3. Kept the existing full-suite ignored tests for `run_zexdoc` and `run_zexall`, but added targeted ignored tests `run_zexdoc_checkpoint` and `run_zexall_checkpoint` driven by `EMU198X_ZEX_CHECKPOINT`, so a specific labelled block can be rerun intentionally.
4. Added fast parser-level tests so ordinary `cargo test` now verifies the checkpoint parser without needing local ZEX binaries or long exerciser runs.
**Verification:** `cargo test -p zilog-z80 --test zex_tests` passes. `cargo clippy -p zilog-z80 --test zex_tests -- -D warnings` passes. `EMU198X_ZEX_CHECKPOINT=1 cargo test -p zilog-z80 --test zex_tests run_zexdoc_checkpoint -- --ignored --nocapture` and the equivalent `run_zexall_checkpoint` both pass locally, each stopping cleanly after checkpoint 1 at `4,520,939,783` half-cycles and roughly `236s`.
**Next dependency:** checkpoint targeting improves diagnosis, but it does not make late-block reruns cheap because each targeted run still replays the prefix from reset. If we want practical routine use beyond early checkpoints, the next real improvement is save-state or resume support between checkpoints.

---

## 2026-04-12 — Z80 local verification corpora are wired back in; Tom Harte rerun passes cleanly

**Type:** milestone
**Trigger:** After the instruction-level integration coverage work, the next useful step was to stop treating `zexdoc`, `zexall`, and the Tom Harte corpus as aspirational references and make the fresh workspace actually discover and run the local verification assets that already exist on disk.
**Result:** the Z80 verification harnesses now use explicit local-corpus discovery instead of brittle hard-coded paths:
1. Added shared test-support lookup in `crates/zilog-z80/tests/support/mod.rs` for the Tom Harte Z80 corpus, ZEX binaries, and future FUSE fixtures. The harnesses now respect explicit environment variables first and then fall back to known local archive roots, including `~/Projects/Emu198x-Unclean/Reference/test-suites/...`.
2. Updated `single_step_tests.rs` to use that shared lookup path. The full Tom Harte run was then executed against the local `processor-tests/z80/v1` corpus and passed completely: **1,604,000 / 1,604,000 cases passing, 0 failed opcodes**.
3. Updated `zex_tests.rs` to discover local `zexdoc.com` / `zexall.com`, treat BDOS function 9 output as line-level progress rather than raw character spam, stop duplicating each BDOS call four times, and honor the exerciser's own `"complete"` message as the intended completion boundary instead of relying only on a final `HALT`.
4. Added an explicit reference-adjudication note to `wiki/concepts/test-methodology.md`: Tom Harte remains the primary per-instruction oracle, ZEX remains the program-level CPU regression suite, and FUSE stays a strong secondary reference for Spectrum-visible timing and bus behavior. Disagreements are to be recorded and resolved, not papered over.
**Verification:** `cargo test -p zilog-z80 --test single_step_tests run_opcode_00 -- --ignored --nocapture` passes against the local corpus (`1000/1000`). `cargo test -p zilog-z80 --test single_step_tests run_all -- --ignored --nocapture` passes with `1,604,000 / 1,604,000` cases. The improved `zexdoc` harness was exercised far enough to confirm correct local binary discovery and sane block-by-block progress reporting, but a full fresh-workspace ZEX rerun was not completed in this session.
**Next dependency:** if we want routine ZEX use rather than occasional long manual runs, the worthwhile next step is the per-block stop/resume or snapshot instrumentation Steve mentioned earlier, so a failing exerciser block can be isolated without replaying the entire program from the beginning.

---

## 2026-04-12 — Z80 ED edge cases and repeat variants narrow further

**Type:** milestone
**Trigger:** After the previous ED-prefixed pass, the remaining execute gaps were no longer broad instruction families. What stayed thin were the edge forms and line-distinct variants that matter to compatibility work: refresh-register transfers, undocumented `IN` / `OUT` register forms, the undocumented `IM 0` opcode alias, and the repeat or non-repeat variants whose implementation paths are separate in `execute.rs`.
**Result:** `crates/zilog-z80/tests/integration.rs` now covers another focused ED slice through real instruction streams:
1. Added direct coverage for `LD R,A` plus `LD A,R`, including the refresh-counter interaction across prefixed fetches and the resulting flag state from the loaded `R` value.
2. Added explicit coverage for the undocumented ED forms `IM 0` via `ED 4E`, `IN F,(C)` as flags-only input, and `OUT (C),0` as zero-valued output.
3. Added the remaining distinct block-opcode variants that were still separate execute arms: `LDDR`, `CPD`, `INIR`, and `OTIR`.
4. The tests continue to assert externally meaningful behavior rather than internal helper details: actual emitted I/O writes, preserved registers on flags-only input, `HL` / `DE` directionality, repeat termination when `B` or `BC` reaches zero, and refresh-register effects that real machine code can observe indirectly.
**Coverage note:** On the current local coverage run, `zilog-z80/src/execute.rs` improved from `74.17%` line coverage to `78.07%`, total workspace region coverage improved from `78.36%` to `79.00%`, and total workspace line coverage improved from `81.16%` to `81.80%`.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p zilog-z80 --test integration`, `cargo test --workspace`, and `./scripts/coverage.sh` all pass.
**Next dependency:** the next Z80-side work should stop being “cover every obvious ED arm” and shift toward the remaining genuinely thin machine-relevant behavior, likely interrupt sequencing, refresh-visible quirks, and any compatibility failures that show up once fuller machine software is driving the core.

---

## 2026-04-12 — Z80 ED-prefixed execute coverage expands through block, port, and 16-bit paths

**Type:** milestone
**Trigger:** After the previous execute-path passes, the biggest remaining holes in the Z80 core had shifted into the ED-prefixed space: interrupt-mode control, stack return paths, `IN` / `OUT` register forms, 16-bit ED arithmetic and indirect loads, nibble-rotate memory operations, and the backward or repeating variants of the block instructions.
**Result:** `crates/zilog-z80/tests/integration.rs` now drives another substantial ED-prefixed slice through real instruction streams:
1. Added direct integration coverage for `LD A,I`, `RETN`, `IM 1`, `IM 2`, `IM 0`, `IN r,(C)`, `OUT (C),r`, `ADC HL,rr`, `SBC HL,rr`, `LD (nn),rr`, `LD rr,(nn)`, `RLD`, `RRD`, and `LDD`.
2. Added backward and repeat-path coverage for the block families that were still thin after the earlier pass: `CPDR`, `IND`, and `OTDR`.
3. The new tests continue to verify machine-facing outcomes rather than internal helper state: stack-pop return addresses, restored interrupt flip-flops, `WZ` side effects, actual I/O bus writes, backward address movement, repeat termination when `B` reaches zero, and the flag behavior that real software depends on.
4. While landing the new `IN r,(C)` coverage, one test assumption had to be corrected: the parity flag for input value `0x81` is set, not clear, because the byte has even parity. The test now asserts the real flag result instead of the mistaken one.
**Coverage note:** On the current local coverage run, `zilog-z80/src/execute.rs` improved from `57.48%` line coverage to `74.17%`, `zilog-z80/src/alu.rs` improved from `68.63%` to `69.02%`, total workspace region coverage improved from `75.48%` to `78.36%`, and total workspace line coverage improved from `78.32%` to `81.16%`.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p zilog-z80 --test integration`, `cargo test --workspace`, and `./scripts/coverage.sh` all pass.
**Next dependency:** the next worthwhile Z80 pass is to keep narrowing the remaining ED-prefixed gaps, especially the refresh-register transfer path (`LD R,A`, `LD A,R`) and any still-thin repeat or interrupt-control behavior that only shows up under real machine software.

---

## 2026-04-12 — Z80 direct transfer, rotate, flag, exchange, and port paths expand

**Type:** milestone
**Trigger:** After the previous execute-path pass, the remaining obvious holes in the Z80 core were no longer mostly control-flow branches. The thinnest areas had shifted to direct memory-transfer instructions, 16-bit pair arithmetic, rotate and flag-manipulation opcodes, alternate-register exchanges, and the single-byte port-I/O path.
**Result:** `crates/zilog-z80/tests/integration.rs` now covers another substantial slice of the execute engine through real instruction streams:
1. Added direct bus-facing coverage for `LD A,(BC)`, `LD A,(DE)`, `LD (BC),A`, `LD (DE),A`, `INC (HL)`, `DEC (HL)`, `INC rr`, `DEC rr`, `ADD HL,rr`, `RLCA`, `RRCA`, `RLA`, `RRA`, `DAA`, `CPL`, `SCF`, `CCF`, `EX AF,AF'`, `EXX`, `IN A,(n)`, and `OUT (n),A`.
2. Added a dedicated I/O-write trace helper in the integration harness so single-byte port output is asserted at the transaction level instead of being inferred indirectly from internal state.
3. The new tests deliberately assert externally meaningful outcomes: memory bytes, register-pair values, carry/half-carry/sign behavior, alternate-register swaps, `WZ` updates where the core models them, and actual emitted I/O writes on the bus.
**Coverage note:** On the current local coverage run, `zilog-z80/src/execute.rs` improved from `49.76%` line coverage to `57.48%`, `zilog-z80/src/alu.rs` improved from `49.02%` to `68.63%`, total workspace region coverage improved from `72.08%` to `75.48%`, and total workspace line coverage improved from `75.29%` to `78.32%`.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p zilog-z80 --test integration`, `cargo test --workspace`, and `./scripts/coverage.sh` all pass.
**Next dependency:** the next worthwhile Z80 pass is to keep driving down the remaining execute gaps in ED-prefixed and block/port behavior, plus any still-thin unprefixed instructions whose timing or side effects matter to real machine software.

---

## 2026-04-12 — Z80 execute-path integration coverage expands

**Type:** milestone
**Trigger:** After landing workspace coverage reporting, the next sensible use of that data was not to chase percentages blindly but to target real weak points in core behavior. The Z80 execute path was an obvious candidate: important control-flow and memory-transfer branches were present, but several of them were not being exercised directly by integration tests.
**Result:** `crates/zilog-z80/tests/integration.rs` now covers a materially wider slice of unprefixed control-flow and data-movement behavior:
1. Added direct integration coverage for `JP cc,nn` taken and not taken paths, `JP (HL)`, `CALL cc,nn` taken and not taken paths, `RET cc` taken and not taken paths, `DJNZ` taken and not taken paths, `RST 38h`, `EX (SP),HL`, `LD A,(nn)` / `LD (nn),A`, and `LD HL,(nn)` / `LD (nn),HL`.
2. These tests are machine-facing rather than isolated ALU assertions: they execute real instruction streams through the half-cycle core, verify resulting register and memory state, and exercise the walker's staged read/write/push/pop flow through the normal bus-facing integration harness.
3. While adding the `DJNZ` tests, one assumption in the new test code turned out to be wrong: the core's reset A state is not zero. The control-flow path itself was correct; the test was fixed to assert the branch outcome directly (`PC`, `B`, `HALT`) instead of assuming a reset accumulator value.
**Coverage note:** On the current local coverage run, `zilog-z80/src/execute.rs` improved from `40.90%` line coverage to `49.76%`, and total workspace line coverage improved from `73.59%` to `75.29%`. That is useful as a sanity signal, but the real gain here is the direct verification of previously untested execute branches.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p zilog-z80 --test integration`, `cargo test --workspace`, and `./scripts/coverage.sh` all pass.
**Next dependency:** the next high-leverage CPU-side increment is to keep working through the execute path with source-backed tests for remaining unprefixed and ED-prefixed instruction families, especially where line coverage is still low in `execute.rs`, `alu.rs`, and the block/IO sequences.

---

## 2026-04-12 — Coverage workflow and local reporting path land

**Type:** milestone
**Trigger:** The workspace had strong fast-test discipline and strict CI, but it still lacked one quantitative signal for how much of the current Rust surface was actually exercised. That made it harder to spot shallow wrappers, newly added untested code paths, and where the verification audit should focus first.
**Result:** Coverage reporting now exists as a first-class repo workflow:
1. `rust-toolchain.toml` now includes `llvm-tools-preview`, so the local toolchain can support source-based coverage without a separate manual component install.
2. New local entry point `scripts/coverage.sh` runs `cargo llvm-cov` for the whole workspace and writes four durable outputs under `target/llvm-cov/`: text summary, JSON summary, LCOV export, and HTML report.
3. New GitHub Actions workflow `.github/workflows/coverage.yml` runs that same script on pushes, pull requests, and manual dispatch. It publishes the `TOTAL` coverage line in the GitHub job summary and uploads both summary artifacts and the HTML report for inspection.
4. `docs/testing-policy.md` now records the intended use of coverage in this project: a directional audit signal, not a substitute for spec-driven testing or the verification ladder.
**Policy note:** This intentionally does not turn coverage percentage into the primary gate. The repo still treats reference-backed behavior and timing tests as the real bar. Coverage is there to show where the test surface is thin, not to certify cycle accuracy by itself.
**Verification:** The new coverage path was exercised locally with `./scripts/coverage.sh`, alongside the normal `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` gates.
**Next dependency:** the next useful step is a crate-by-crate coverage audit against the testing policy, especially for thin runtime and runner crates where percentages can now be compared against the actual verification matrix.

---

## 2026-04-12 — Spectrum family query namespace lands without widening `MachineCore`

**Type:** milestone
**Trigger:** The shared shell query surface was useful, but it only exposed session-owned state. The next gap was family-specific observability. That needed to land without turning `MachineCore` into a debugger or chip-inspection dumping ground.
**Result:** The shell now supports family-owned query namespaces through a separate `SessionQueryProvider` hook:
1. `emu198x-shell` now distinguishes between shared session queries and optional machine-family query providers. `HeadlessSession` can be created with a provider, and it merges provider-owned paths into `query_paths()` while falling back to provider-owned `query()` resolution only when a path is not part of the shared shell surface.
2. `runtime-sinclair-zx-spectrum` now ships `SpectrumSessionQueryProvider`, which owns the initial `spectrum.*` namespace: board issue, current half-cycle within the frame, keyboard matrix rows, and tape loaded/playing state.
3. `emu198x-script-spectrum` now boots its session with that provider, so shared JSON scripts can resolve both generic shell paths and Spectrum family paths through the same `query` / `query_paths` actions.
**Boundary note:** This was kept intentionally out of `MachineCore`. The runtime opts into family observability explicitly, and the shell still owns only the generic session model. That keeps chip- and family-specific inspection narrow and composable instead of making it part of the mandatory runtime contract for every machine.
**Documentation note:** `docs/features/scripting.md` now records the current Spectrum-owned `spectrum.*` paths in addition to the shared shell query paths.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes a shell test for provider-backed query extension, runtime tests for Spectrum query-path discovery and value resolution, and a Spectrum runner test that executes a script querying `spectrum.machine.issue`.
**Next dependency:** the next honest step is to decide how far family observability should go before we start needing explicit debugger namespaces, memory views, or trace/capture query surfaces. The current structure supports that growth, but it should stay deliberately narrow unless a concrete workflow needs more.

---

## 2026-04-12 — Shared session query surface and script observations land

**Type:** milestone
**Trigger:** The shell layer could already boot machines, load media, run frames, save captures, and execute JSON scripts, but scripts still only drove side effects. There was no shared way to ask the live session what it knew or to get structured results back from the script path itself.
**Result:** `emu198x-shell` now owns the first reusable observability surface above one live machine runtime:
1. New `query` module defines stable generic session paths, typed `QueryResult` / `QueryPathsResult` responses, and path resolution for current shell-owned state such as session time, profile metadata, capture availability, and the most recent run result.
2. `HeadlessSession` now tracks `last_run_result` and exposes `query()` plus `query_paths()`, so host-side tools can inspect live state without downcasting into family runtimes.
3. `HeadlessScript` and `ScriptStep` now support `query` and `query_paths` actions, and they return structured `ScriptObservation` values for `run_frames`, `query`, and `query_paths` instead of acting as pure fire-and-forget control flow.
4. `emu198x-script-spectrum` now emits one JSON report on stdout when `--script PATH` is used. That report includes structured observations plus final machine state (`time`, `tape_loaded`, `tape_playing`), which gives automation and future MCP-style hosts a real machine-readable result boundary.
**Documentation note:** `docs/features/scripting.md` now describes the current fresh-workspace contract instead of the older JSON-RPC-style scripting proposal. It reflects the real `action`-based step format, current shared actions, the implemented generic query paths, and the current JSON report shape.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes shell tests for query-path filtering, run-state query resolution, session query access, script query observations, and a Spectrum runner test that executes a shared script and inspects the structured observation report.
**Next dependency:** the next useful increment is to widen observability carefully, likely with family-owned query namespaces above the same shell surface rather than by smuggling debugger or chip-inspection policy into `MachineCore`.

---

## 2026-04-12 — Shared headless session and JSON script runner land

**Type:** milestone
**Trigger:** The shell surface could already boot machines, load media, control transport, capture PNG/WAV, and save snapshots, but those operations were still being composed ad hoc inside the Spectrum CLI. The next gap was the reusable host-side workflow layer itself.
**Result:** `emu198x-shell` now owns that workflow layer:
1. `MachineCore` gained a `time()` accessor so host-side code can reason about authoritative machine progress without downcasting to family runtimes.
2. New `HeadlessSession` wraps one live machine runtime together with queued input events, frame capture, audio capture, and native-frame stepping. It owns the reusable operations a headless runner actually needs: prepare media/commands, run frames, save screenshots, save audio, save and restore snapshots, and queue host input.
3. New `HeadlessScript` / `ScriptStep` in `emu198x-shell` parse and execute shared JSON session scripts. The initial generic step set covers media loading, media transport, queued input events, frame execution, snapshot load/save, PNG screenshot export, and WAV audio export.
4. `emu198x-script-spectrum` now runs through that shared session layer for both direct CLI flags and `--script PATH`, instead of composing its own one-off host loop.
**Documentation note:** `docs/features/scripting.md` no longer claims that all four anchor families already have fresh-workspace script and MCP runners. Its top-level note now reflects the current truth: shared shell support exists, and Spectrum is the implemented runner on that path today.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes shell tests for session stepping, queued input delivery, file-writing helpers, JSON script parsing/execution, and an end-to-end Spectrum runner test that executes a shared JSON script file.
**Next dependency:** the next useful increment is to keep pushing host policy into the shell layer by adding a structured result/query surface on top of the same session model, so future script and MCP paths can share more than just control flow.

---

## 2026-04-12 — Shared PNG/WAV capture lands on the shell surface

**Type:** milestone
**Trigger:** The headless path could boot, load media, control tape transport, and save snapshots, but it still had no shared way to turn emitted frame/audio packets into durable artifacts. Capture remained only a documented intention.
**Result:** `emu198x-shell` now owns the first reusable capture layer:
1. New `capture` module stores the latest emitted frame or a whole audio stream through `LatestFrameCapture` and `AudioCapture`, both implementing the shared `FrameSink` / `AudioSink` traits directly.
2. The shell can now convert raw machine output into real artifacts without family-specific code: indexed or RGBA frames encode to PNG, and captured audio encodes to 16-bit PCM WAV.
3. `emu198x-script-spectrum` now exposes that shared path through `--screenshot PATH` and `--audio-capture PATH`. The runner still stays thin: it just selects the capture sinks, runs frames, and writes the encoded bytes returned by the shell helpers.
**Boundary note:** Capture remains strictly above the runtime boundary. The Spectrum runtime still only emits raw indexed video and float audio packets; PNG and WAV are host-side concerns owned by the shell layer.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes shell tests for indexed-frame PNG output and WAV output plus runner tests that boot a zero ROM, emit one frame, and write both artifact types.
**Next dependency:** the next logical step is to use the same shared shell surface for scripted headless workflows, so capture, boot, media control, and later MCP methods all compose around one host-side session model instead of ad hoc CLI glue.

---

## 2026-04-12 — Shared firmware bootstrap and media transport control land

**Type:** milestone
**Trigger:** The first Spectrum headless runner worked, but it still owned too much host policy itself: firmware was a hard-coded `--rom` path interpreted directly by the binary, and tape start/stop bypassed the shared control surface via `Spectrum48kRuntime` methods.
**Result:** `emu198x-shell` now owns the first reusable host-side bootstrap/control layer:
1. New shared firmware types `FirmwareImage` and `FirmwareSet` validate declared firmware ids against `MachineProfile` requirements, catching missing, duplicate, and unknown firmware before family runtimes try to boot.
2. `MachineCore` now accepts shared `ControlCommand`s, with the first concrete command family being media transport (`start` / `stop` on a named slot).
3. New `boot_machine()` and `prepare_machine()` helpers formalize the thin-runner path: construct from firmware or a blank runtime for snapshot restore, then apply media inserts plus shared control commands.
4. `runtime-sinclair-zx-spectrum` now implements that contract directly: `Spectrum48kRuntime::from_firmware()` resolves the declared 48K ROM id, and tape playback is driven through shared media-transport commands on slot `tape-1`.
5. `emu198x-script-spectrum` is now a genuinely thin adapter. It still supports the Spectrum-friendly aliases (`--rom`, `--tape`, `--play-tape`), but its real path is shared and profile-driven: `--firmware ID=PATH`, `--media SLOT:KIND=PATH`, `--start-slot`, `--stop-slot`, snapshot load/save, then frame execution.
**Boundary note:** This is intentionally still host-side policy. The runtime validates firmware and honors transport commands, but it does not gain any filesystem or CLI knowledge, and the machine core still owns only hardware state and timing.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes shell tests for firmware validation and bootstrap/control helpers, runtime tests for declared-firmware boot plus tape transport commands, and script-runner tests for both generic flags and Spectrum compatibility aliases.
**Next dependency:** the next useful step is to keep extracting headless policy out of one binary by building capture/scripting entry points on the same shared shell surface rather than teaching each family runner its own bespoke workflow.

---

## 2026-04-12 — Spectrum runtime snapshots and headless runner land

**Type:** milestone
**Trigger:** The fresh-workspace Spectrum path had an honest machine loop, media parsing, and a `MachineCore` runtime, but there was still no durable state handoff and no small headless entry point that could supply firmware, load tapes, drive playback, and save/restore execution state.
**Result:** Two connected boundaries landed together:
1. `runtime-sinclair-zx-spectrum` now owns versioned runtime snapshot import/export. `Spectrum48kRuntime::snapshot()` serializes machine time plus validated 48K machine state into a postcard envelope, and `restore()` rejects wrong profile/version payloads before rebuilding the live machine.
2. New crate `emu198x-script-spectrum` provides the first headless family runner in the fresh workspace. It cold-boots from a ROM, optionally restores a snapshot, loads `tape-1` media from TAP/TZX bytes, explicitly starts tape playback, runs an exact frame count on the native Spectrum cadence, and can write a new runtime snapshot on exit.
**Design note:** The machine snapshot boundary is explicit rather than deriving `Serialize` directly on the whole machine. Large ROM/RAM arrays are flattened into `Spectrum48kSnapshot`, which keeps restore validation local to the machine crate and avoids pretending that every internal type is part of a stable wire format.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes runtime snapshot round-trip tests and runner tests for CLI parsing plus ROM boot to snapshot output.
**Next dependency:** the next honest step is to make the headless Spectrum path less ad hoc by formalizing firmware/tape control policy above the runtime boundary instead of leaving it embedded in one family-specific script binary.

---

## 2026-04-12 — Spectrum media parsers, runtime wrapper, and beeper audio land

**Type:** milestone
**Trigger:** The 48K machine had a real frame loop and tape progression, but there was still no honest shell-facing media path and no machine-emitted audio packet path.
**Result:** Three connected changes landed together:
1. New crates `format-sinclair-zx-spectrum-tap` and `format-sinclair-zx-spectrum-tzx` now parse the two baseline Spectrum tape formats into machine-usable structures/pulse streams.
2. `common-sinclair-zx-spectrum` gained `BeeperAudio`, and `machine-sinclair-zx-spectrum-48k` now models the beeper/EAR speaker path at T-state precision, emitting one mono audio frame alongside each video frame.
3. `runtime-sinclair-zx-spectrum` now includes `Spectrum48kRuntime`, the first fresh-workspace `MachineCore` implementation: it owns a real 48K profile, validates ROM bytes at construction, accepts `MediaSet` tape loads on slot `tape-1`, forwards host key events into the keyboard matrix, and emits indexed video plus mono audio packets through the shell sinks.
**Accuracy note:** Tape EAR no longer keeps driving `$FE` after the virtual deck stops. Tightening that behavior fixed an earlier overreach in the machine tests; once playback ends, bit 6 falls back to the ULA/tape-input boundary rather than a stale tape level.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all pass. New coverage includes TAP/TZX parser tests, machine audio tests, and runtime tests for `MediaSet` loading plus frame/audio emission.
**Next dependency:** snapshot import/export on the new runtime boundary and then a real family product/runner layer that can supply firmware and drive tape control without smuggling policy into the machine core.

---

## 2026-04-12 — Spectrum tape progression lands; ROM-backed boot smoke test added

**Type:** milestone
**Trigger:** The 48K machine had a real ULA/Z80 frame loop but tape was still only a static EAR-line override. The next honest step was to make media advance on the real 3.5 MHz T-state cadence.
**Result:** `common-sinclair-zx-spectrum` now owns a shared pulse-driven `TapePlayer` plus standard ROM-speed block-to-pulse helpers. `machine-sinclair-zx-spectrum-48k` now wires that player into the live frame loop: the machine advances tape every T-state (`hc % 4 == 2`), exposes the current EAR level through `$FE`, keeps the external `TapeInput` override as a higher-priority boundary for non-player sources, and adds machine-local load/play/stop helpers for raw pulses and standard blocks.
**Boot-path note:** The 48K machine now also carries an ignored ROM-backed smoke test that loads `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`, runs 200 frames, and asserts that the ROM has populated both pixel RAM and attribute RAM. This is intentionally a smoke test, not a claim of completeness.
**Quality note:** The imported tape player was tightened while porting: `play()` now resumes a partially consumed pulse instead of rewinding it, and zero-length pulses are consumed without risking an infinite loop.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all pass. The workspace now includes shared tape unit tests, machine tests for T-state-driven tape progression, and an ignored ROM-backed boot test hook.
**Next dependency:** actual media format ingestion at the machine/runtime boundary (`.tap` / `.tzx`) and then the 48K beeper/EAR audio path, both driven from this same T-state cadence rather than from host-time shortcuts.

---

## 2026-04-12 — Spectrum 48K machine crate lands; firmware boundary gap noted

**Type:** milestone + design note
**Trigger:** First fresh-workspace machine-layer implementation for the Spectrum 48K.
**Result:** New crate `machine-sinclair-zx-spectrum-48k` owns the first honest machine-local state: 48K memory delegation, the 8 half-row keyboard matrix, shell `InputEvent::Key` translation, tape EAR input state, and board-issue-correct `$FE` read/write behaviour (Issue 2 vs Issue 3 bit-6 feedback, with tape override when connected).
**Source notes:** The matrix key encoding ports cleanly from the older runtime crate; the board-issue `$FE` behaviour ports from the old Ferranti ULA tests. The old bus loop and ULA timing code were deliberately *not* reused here.
**Design note for future sessions:** `emu198x-shell::MachineCore` still has media loading but no firmware-loading boundary. That means ROM-dependent machine crates should stay *below* the shell trait for now rather than faking firmware as media or inventing half-initialized constructors. Revisit the shell boundary after at least one real machine path proves what the firmware handoff actually needs to look like.

---

## 2026-04-12 — Z80 crate ported into the fresh workspace

**Type:** milestone
**Trigger:** The Spectrum path reached the point where another support crate would just defer the real dependency. The next honest move was the CPU.
**Result:** `zilog-z80` is now present in the fresh workspace as the real half-cycle, pin-level Z80 core from the fresh-start lineage: public bus pins (`addr`, `data`, `mreq`, `iorq`, `rd`, `wr`, `m1`, `rfsh`, `halt`), input pins (`data_in`, `wait`, `irq`, `nmi`), static M-step sequences, and the instruction walker/ALU/register file needed for real execution.
**Verification:** Workspace checks pass with the imported crate under the current repo lint bar: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. The port carries 19 unit tests and 31 integration tests locally, plus ignored Tom Harte and ZEX harnesses.
**Quality note:** The initial quick port temporarily allowed `clippy::unwrap_used` in the test harnesses. That was immediately removed and the harnesses were rewritten to use explicit control flow instead, so the crate now matches the repo policy cleanly rather than by exception.
**Next dependency:** the Ferranti 6C001E ULA wrapper and the first real 48K machine loop that wires ULA gating to the Z80 pins.

---

## 2026-04-12 — Ferranti ULA and first real 48K frame loop land

**Type:** milestone
**Trigger:** With the pin-level Z80 in place, the next honest step was to stop modeling `$FE` and contention in isolation and wire the real 48K video chip into the machine.
**Result:** Three linked changes landed together:
1. `common-sinclair-zx-spectrum` grew the shared ULA substrate: palette helpers, `FrameTiming`, the Spectrum `Ula` trait, and the shared `UlaEngine`.
2. New crate `ferranti-ula-6c001e` ports the 48K Ferranti wrapper, including board-issue-specific EAR feedback (`Issue2` MIC-or-EAR vs `Issue3` EAR-only).
3. `machine-sinclair-zx-spectrum-48k` now owns a real 48K frame loop: the Ferranti ULA ticks against the 48K memory map, gates the Z80 clock, feeds IRQ, performs bus reads/writes, exposes the rendered framebuffer, and uses the ULA's floating-bus behaviour for unattached odd-port reads.
**Quality note:** The temporary machine-local `$FE` latch model was retired from the machine path. Tape EAR override still lives at the machine boundary because that line is external to the ULA core; border/beeper/keyboard feedback now come from the actual ULA implementation.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all pass. New local coverage includes palette tests, Ferranti board-issue tests, and a `run_frame()` smoke test for the integrated 48K machine.
**Next dependency:** honest media/tape progression and then ROM-backed boot-path tests against the new machine loop, rather than the former state-only machine shell.

---

## 2026-04-10 — Amiga boot screen debugging: root cause narrowed

**Type:** investigation
**Trigger:** Kickstart 1.3 shows white screen despite all OS inits completing.
**Key finding:** Compared chip RAM and register state against FS-UAE running the same Kickstart 1.3 with same 512K config. Chip RAM at $000-$600 is **byte-for-byte identical**. CPU instructions produce correct results. JMP table at $400+ matches exactly.

**Ruled out:** CPU instruction bugs, CPU speed (4× still white), copper corruption (COPJMP2 disabled still white), memory detection, init sequence (all residents run), chip RAM aliasing, autoconfig, byte-write merging, DMA contention, CIA init, TAS.

**Root cause:** Graphics.library never builds the display copper list. The COP2 display list that FS-UAE has at $10450 (WAIT→colors→DIWSTRT→2-plane bitplanes→END) does not exist anywhere in our chip RAM. COP2LC address also differs ($2408 ours vs $10450 FS-UAE).

**Top lead for next session:** The archive used **ECS Agnus/Denise wrappers** that provide BEAMCON0=$0020 (PAL) and other ECS registers. FS-UAE also returns BEAMCON0=$0020. Our pure OCS Agnus returns 0 for BEAMCON0. If graphics.library or the strap task reads BEAMCON0 to determine PAL mode and gets 0, it may skip display creation.

**Fixes applied this session:** Byte-write merging for custom registers, chip RAM DMA bus contention, CIA-A external_a=$EB, CPU reset_to(), autoconfig bus float, VPOSR v9/v10 bits, Gary slow_ram config, 13→19 pin-level 68000 tests.

**Workspace:** 730 tests, 0 failures, 62 crates. 12 commits this session.

---

## 2026-04-10 — Amiga Phase 8: runtime + CLI + screenshot

**Type:** milestone
**Trigger:** Phases 1-7 complete (all chips + machine + peripherals). Needed visible output.
**Result:** Two crates:
1. `runtime-commodore-amiga` (4 tests) — RGBA framebuffer conversion from Denise's ARGB32 raster buffer, cropped to 724×568 visible display area.
2. `emu198x-script-amiga` — headless CLI: `emu198x-script-amiga kick.rom --frames N --screenshot out.png [--adf disk.adf]`
**Kickstart 1.3 boot status:**
- CPU executes from ROM, clears overlay, sets up exception vectors ✓
- Keyboard power-up init ($FD/$FE handshake) completes ✓
- VERTB interrupt fires every frame, CPU handles via autovector ✓
- exec.library scheduler reached (STOP #$2000 idle loop) ✓
- DMA enabled (bitplane + copper + blitter + sprite) ✓
- Copper list at $2368 runs for 3 frames before being replaced ✓
- **Not yet working:** boot animation (hand/insert-disk screen). The graphics.library task that maintains the persistent copper list isn't setting COP1LC after exec init. This is a CPU/scheduler interaction issue — the 68000's instruction execution is correct (7 pin-level tests + 200 frames of successful Kickstart init), but the OS-level task scheduling needs further debugging. Same class of issue as the NES port where the machine wiring was the bottleneck, not the chip logic.
**Workspace totals:** 724 tests passing, 0 failing, 18 ignored (62 crates).

---

## 2026-04-10 — Amiga Phase 7: floppy + keyboard + ADF

**Type:** milestone
**Trigger:** Phase 6 (machine wiring) complete — peripherals needed for Kickstart to proceed past early init.
**Result:** Three crates ported from archive as clean lifts:
1. `format-commodore-amiga-adf` (139 lines, 6 tests) — ADF image parser, DD/HD support, sector read/write.
2. `peripheral-commodore-amiga-floppy` (480 lines + 492 MFM, 24 tests) — drive mechanism: head positioning, motor spin-up, disk change, MFM track encode/decode with Amiga odd/even bit-split format, sector write-back via DiskImage trait.
3. `peripheral-commodore-amiga-keyboard` (357 lines, 8 tests) — power-up init sequence ($FD/$FE with handshake), rotated keycode transmission, timeout/resend.
All three wired into `machine-commodore-amiga`:
- Keyboard ticks on E-clock, injects serial bytes into CIA-A SDR, handshake on CIA-A CRA bit 6 falling edge
- Floppy ticks on E-clock (motor spin-up), status feeds CIA-A PRA (DSKCHANGE/DSKPROT/DSKTRACK0/DSKRDY), control from CIA-B PRB (step/dir/side/sel/motor)
- Disk DMA now encodes from real floppy track data instead of dummy stream
**Workspace totals:** 720 tests passing, 0 failing, 18 ignored (60 crates).
**Next:** Phase 8 — runtime + headless CLI + PNG screenshot. Validation target: Kickstart 1.3 hand/insert-disk screen.

---

## 2026-04-10 — Amiga Phase 6: machine-commodore-amiga wiring

**Type:** milestone
**Trigger:** Continuation from phases 1-5 (all OCS chips ported). Phase 6 is the machine wiring — the "moment of truth" where the clock tree drives everything.
**Result:** `machine-commodore-amiga` crate landed with 16 tests (10 machine + 6 memory). Master-clock-driven tick loop implements the amiga-port-plan.md pseudocode exactly:
- CCK every 8 master clocks: Agnus beam advance + DMA slot allocation (bitplane, sprite, disk, copper, audio, blitter) + Denise pixel output + Paula audio DMA + audio downsampling
- CPU every 4 master clocks: 68000 State enum inspection for bus servicing via Gary address decode → chip RAM / Kickstart ROM / slow RAM / CIA-A / CIA-B / custom registers. Interrupt ack returns autovector. Paula IPL → CPU ipl pin routing.
- E-clock every 40 master clocks: CIA-A/CIA-B tick, CIA IRQ → Paula interrupt routing
- Full custom register read/write dispatch (Agnus/Denise/Paula/Copper), including BPLCON0/DDFSTRT/DDFSTOP/color register pipelining (2-CCK Agnus→Denise delay)
- Full synchronous blitter (area + line mode) ported from archive
- Disk DMA with WORDSYNC, sprite DMA phase state machine, bitplane DMA with vertical enable flip-flop and modulo application
- Serial port minimal model (TBE always initially set for Kickstart boot)
- run_frame() advances by one PAL frame, stereo audio with RC low-pass filter
**New crates:** `machine-commodore-amiga` (16 tests)
**Workspace totals:** 682 tests passing, 0 failing, 18 ignored (57 crates).
**Next:** Phase 7 (floppy + keyboard) for Kickstart to proceed past init, then Phase 8 (runtime + CLI + screenshot) for visible output. The validation target is booting Kickstart 1.3 to the hand/insert-disk screen.

---

## 2026-04-10 — Amiga phases 1-5: 68000 + all OCS chips ported

**Type:** milestone
**Trigger:** User chose to push through to Amiga after NES was complete.
**Result:** Six Amiga crates landed:
1. `motorola-68000` (14,167 lines) — pin-level bus conversion from archive's `M68kBus` trait. tick() reads `bus_status` and `ipl` pin fields. 7 pin-level tests (MOVEQ, MOVE, ADD, JSR/RTS, memory read/write, DBRA loop, supervisor mode). 68020+ synchronous bus ops stubbed.
2. `mos-cia-8520` (634 lines, 18 tests) — clean lift, Amiga CIA variant.
3. `commodore-gary` (687 lines, 37 tests) — clean lift, address decoder.
4. `commodore-agnus-ocs` (1,706 lines, 30 tests) — clean lift, beam + DMA + copper.
5. `commodore-denise-ocs` (2,319 lines, 18 tests) — clean lift, pixel pipeline.
6. `commodore-paula-8364` (1,394 lines, 8 tests) — clean lift, audio + interrupts.
**Workspace totals:** 666 tests passing, 0 failing, 18 ignored.
**Next:** Phase 6 — machine-commodore-amiga wiring (master clock → Agnus → DMA → CPU + Denise + Paula). This is the largest remaining piece (~6,600 lines in the archive). Start by reading the archive's `machine-commodore-amiga/src/lib.rs` tick loop and rewriting for pin-level CPU bus. Target: boot Kickstart 1.3.

---

## 2026-04-10 — APU ported + System trait + archive cleanup

**Type:** milestone
**Trigger:** Continuation after nestest + SMB screenshot. APU was the last missing chip; System trait was needed for shell integration.
**Result:** Three deliverables:
1. `ricoh-apu-2a03` crate — clean lift from archive, 21 tests pass unchanged. Wired into machine-nintendo-nes: ticks per CPU cycle, registers routed, IRQ OR'd into cpu.irq, DMC DMA bytes fed from mapper.
2. `runtime-nintendo-nes` System trait impl — family=Nes, model=nintendo-nes-ntsc, RGBA framebuffer, 48 kHz mono audio, controller input via inject_input, register read/write. 5 runtime tests.
3. Archive cleanup — `ricoh-apu-2a03` deleted from archive (commit `3d8c51d60b`). Remaining NES crates in archive: `format-nintendo-nes-ines` (47 mappers), `emu-nintendo-nes{,-wasm}` (frontend/WASM).
**New crates:** `ricoh-apu-2a03` (21 tests)
**Pages created:** `chips/ricoh-apu-2a03.md`
**Pages updated:** `index.md`, `decisions/archives-as-source.md` (NES source map + cleanup history)
**Workspace totals:** 455 tests passing, 0 failing, 18 ignored.

---

## 2026-04-10 — nestest: 8991/8991 instructions matched

**Type:** milestone
**Trigger:** Machine wiring landed — natural next step was running real NES code.
**Result:** nestest.nes (Kevin Horton's CPU instruction exerciser) passes with **8,991 / 8,991 instructions matching** the golden log. Register state (PC, A, X, Y, P, SP) compared at every instruction fetch. Test result codes: `$02` (official opcodes) = `0x00`, `$03` (unofficial opcodes) = `0x00` — all official and unofficial opcodes pass.
**New test file:** `crates/machine-nintendo-nes/tests/nestest.rs` — smoke test (first 100 instructions, runs on every `cargo test`), full suite (`#[ignore]`'d, ~0.01s in release).
**What this validates:** the complete NES chip stack working together under real code — tick loop timing, address space routing, PPU register bus, CPU instruction correctness, 2A03 BCD-disabled behaviour. This is the NES equivalent of the C64 boot-to-READY test.
**Workspace totals:** 429 tests passing, 0 failing, 18 ignored.

---

## 2026-04-10 — machine-nintendo-nes: NES machine wiring

**Type:** milestone
**Trigger:** Continuation after PPU port — all three NES chip crates were ready (2A03 CPU, 2C02 PPU, iNES parser + NROM mapper), so the machine wiring was the natural next step.
**Result:** `machine-nintendo-nes` crate landed with 12 tests. Master-clock tick loop implements the nes-clock-topology decision doc exactly: PPU every dot, CPU every 3rd dot, NMI/IRQ routed between ticks. OAMDMA stalls CPU for 514 cycles. Controller 1 serial shift register. Full NES address space (2 KiB RAM mirrored, PPU registers, APU stubs, mapper). `run_frame()` runs until pre-render → scanline 0 transition.
**New crates:** `machine-nintendo-nes` (12 tests)
**Pages created:** `systems/nintendo-nes.md`
**Pages updated:** `index.md` (NES system overview added)
**Workspace totals:** 428 tests passing, 0 failing, 17 ignored.
**Next:** nestest.nes validation (golden log comparison), then runtime + headless CLI.

---

## 2026-04-10 — Ricoh 2C02 PPU: dot-level rendering ported

**Type:** milestone
**Trigger:** Continuation of NES port after phase 1 (2A03 + iNES parser + clock topology). Steve confirmed that the machine wiring was the architectural issue, not the PPU's rendering logic itself — so the port was scoped as an interface rewrite rather than a logic rewrite.
**Result:** `ricoh-ppu-2c02` crate ported from archive with 20 tests. Internal rendering logic (background tile fetch pipeline, sprite evaluation with hardware overflow bug, pixel composition, loopy scroll registers, NMI timing, odd-frame skip) lifts intact. Interface changes: `tick()` takes `&mut dyn Mapper` instead of closures; `nmi` is a public active-high field instead of active-low with edge-detection helpers; A12 transitions call `mapper.notify_a12_rendering()` directly from inside `tick()` instead of deferring for the machine to poll.
**New crates:** `ricoh-ppu-2c02` (20 tests)
**Pages created:** `chips/ricoh-ppu-2c02.md`
**Pages updated:** `index.md` (PPU chip line added)
**Workspace totals:** 416 tests passing, 0 failing, 17 ignored.

---

## 2026-04-10 — NES phase 1: 2A03 variant, iNES parser, clock topology

**Type:** milestone
**Trigger:** Steve chose tight NES scope (Option B) — 2A03 CPU variant + Tom Harte validation, iNES/NES 2.0 parser with NROM only, clock topology decision doc. No PPU scaffolding this session.
**Result:** Three deliverables landed:

1. **2A03 CPU variant** — `M6502::new_2a03()` constructor sets `decimal_disabled: true`, gating the BCD paths in `alu_adc` and `alu_sbc`. Validated against the Tom Harte NES fixture (`nes6502/v1/`): **2 470 000 / 2 470 000 stable opcodes passing, zero regressions.** Same 9 unstable undocumented opcodes excluded. Bonus: `6b` (ARR #imm) passes 10 000/10 000 on NES because BCD disabled makes it deterministic.
2. **`format-nintendo-nes-ines` crate** — iNES 1.0 + NES 2.0 header parser, `Mapper` trait, NROM (mapper 0) implementation. 17 tests covering 16 KiB/32 KiB PRG, CHR ROM/RAM, mirroring modes, PRG RAM, battery flag, NES 2.0 12-bit mapper numbers, error cases. The other 47 mappers from the archive are deferred until the PPU crate is online.
3. **`wiki/decisions/nes-clock-topology.md`** — formal decision doc for the NES master-clock-driven tick loop (RULES.md item 1), PPU every dot, CPU every 3rd dot, pin contracts for PPU/CPU/Mapper, drift triggers, OAMDMA/DMC DMA stall shapes.

**New crates:** `format-nintendo-nes-ines` (17 tests)
**Pages created:** `decisions/nes-clock-topology.md`
**Pages updated:** `chips/mos-6502.md` (Variants section added, test coverage expanded to cover both suites), `index.md` (NES section added, crate + decision listed)
**Workspace totals post-session:** 396 tests passing, 0 failing, 17 ignored.

---

## 2026-04-09 — Tom Harte 6502 regression suite: 2.47M / 2.47M stable

**Type:** milestone
**Trigger:** Option B from the post-READY-screenshot planning — Tom Harte validation of the mos-6502 port against <https://github.com/SingleStepTests/65x02>. Fixture found on disk at `~/Projects/Emu198x-archive/test-data/65x02/6502/v1/` (1 GiB, 256 JSON files, 2.56 M test cases).
**Result:** **2 470 000 / 2 470 000 stable opcodes passing, zero regressions.** Every one of the 151 documented 6502 opcodes passes every Tom Harte test case (register state, memory state, cycle counts). 96 of the 105 undocumented opcodes also pass cleanly. The 9 that don't match (`6b`, `8b`, `93`, `9b`, `9c`, `9e`, `9f`, `ab`, `bb`) are exactly the famously-unstable opcodes whose behaviour varies between chip revisions and ambient temperature on real hardware — the port stubs them as `NopRead` per the archive's "Unstable undocumented" comment block, and the regression suite excludes them via a hardcoded allow-list.
**New test file:** `crates/mos-6502/tests/tom_harte.rs` (~430 lines) — pin-level test harness with fixture resolution (`MOS_6502_TEST_DATA` env var / known-good archive path / in-tree `test-data/`), 4 smoke tests that run on every `cargo test` (~150 ms for 40 000 cases), and the `#[ignore]`'d `run_all` regression suite (~6 s in release mode for 2.56 M cases).
**Pages updated:** `chips/mos-6502.md` (Test coverage section expanded, "Known gaps" section reshaped — Tom Harte gap closed, AbsX cycle-count quirk confirmed as not-a-bug, unstable undocumented opcodes documented as deliberate and accepted).
**What this validates:** the mos-6502 port is now validated to gold-standard level for every opcode that any real software uses. The pin-level pipelined tick model, every addressing mode, every flag update, every cycle count, BCD arithmetic in both ADC and SBC, indirect-Y with page cross, BRK/IRQ/NMI stack pushes, RMW three-write-phase cycles — all correct. The only remaining 6502 gap that matters is the unimplemented unstable undocumented opcodes, and those are consciously excluded.
**A small bonus finding:** the previously-flagged `tick_absolute` AbsX no-cross "1-extra cycle" concern was actually **not a bug** — the Tom Harte tests on `bd.json` (LDA abs,X) pass cleanly, proving the cycle count is correct. The concern came from a misreading of the archive's code during the port. Updated the wiki to reflect this.
**Runtime:** 6.75 seconds for 2.56 M test cases in release mode on this machine. Cheap enough to run on every PR that touches mos-6502 source.

---

## 2026-04-09 — C64 runtime + CLI: first visible READY. screenshot

**Type:** milestone
**Trigger:** Immediately after the machine-wiring boot test confirmed the KERNAL ran end-to-end in RAM, Steve picked Option A (runtime + frontend) from the next-step planning. The session built `runtime-commodore-c64` (wrapper + `System` trait impl + RGBA conversion + file loader), `emu198x-script-c64` (headless CLI with fast-path flags), and ran it against the real ROMs to produce a PNG of the booted BASIC prompt.
**Result:** **Rendered.** 120-frame boot run produced a 416×312 8-bit RGBA PNG at `/tmp/c64-screenshots/boot-120frames.png` showing the classic `**** COMMODORE 64 BASIC V2 ****` / `64K RAM SYSTEM  38911 BASIC BYTES FREE` / `READY.` banner, in the light-blue-on-blue C64 palette, with the cursor after `READY.`.
**Pages updated:** `systems/commodore-c64.md` (phase 4 section added), `index.md` (not yet — will follow in a doc pass when runtime-commodore-c64 and emu198x-script-c64 are public).
**New crates:** `runtime-commodore-c64` (12 tests), `emu198x-script-c64` (headless CLI, no tests — it's wrapper code over the runtime).
**What this validates:** the remaining unknowns after phase 3 — the `System` trait implementation, RGBA conversion from the VIC-II's `Vec<u32>` framebuffer, integration with `emu198x-shell::encode_png_to_file`, the CLI fast-path for headless captures. All clean, all first-try. The RGBA re-pack runs per frame (O(width × height) shift-and-mask on each pixel) and the resulting byte order matches the shell's `PixelFormat::Rgba8888` expectation (R G B A byte order, not the BGRA that a naive little-endian slice cast would have produced).
**Workspace totals post-session:** 372 tests passing, 0 failing, 15 ignored (the 15 are the boot-to-READY test, a Tom Harte fixture test awaiting vendoring, and a handful of long-running integration tests in other crates).

---

## 2026-04-09 — C64 machine wiring boots the KERNAL end-to-end

**Type:** milestone
**Trigger:** Steve located the C64 ROMs (`basic.rom`, `chargen.rom`, `kernal.rom`) in `Emu198x-archive-april2026/roms/c64/` and the `#[ignore]`'d boot-to-READY integration test in `machine-commodore-c64` was run against them.
**Result:** **Booted first try.** `Found READY. at frame 108, offset $00C8` — the KERNAL reached the BASIC `READY.` prompt at frame 108 (~2.16 s of emulated C64 time, matching real hardware's ~2.5 s cold-boot timing). Test runtime: ~2.35 s.
**Pages updated:** `systems/commodore-c64.md` (implementation-status section now records the validation milestone, boot-test section documents the known-good ROM location and command).
**What this validates:** every architectural decision from the chip wave + machine wiring — pin-level CPU bus (RULES.md item 6), the `VicMemory` trait, one-op-per-tick discipline, tick ordering, IRQ routing (VIC∪CIA1→irq, CIA2→nmi), RDY-only-gates-reads semantics, `$00`/`$01` port banking, the 6510 I/O-port routing at `$D000-$DFFF`, and the CIA1 keyboard scan hand-off through `pb_in`. ~2 million real KERNAL opcodes executed without an illegal-instruction trap, a stack overflow, or a stuck-on-BRK loop. The bad-line BA assertion stalled the CPU correctly without deadlocking. Memory banking put BASIC and KERNAL in the right places for the KERNAL's own boot-time reads.
**Known-good ROMs:** `~/Projects/Emu198x-archive-april2026/roms/c64/{basic,chargen,kernal}.rom` (8192/4096/8192 bytes).
**Why this is a big deal:** before this run, every chip had been tested in isolation with hand-written fixtures; the machine had been tested with stub ROMs containing sentinel byte patterns. No evidence existed that the chips *worked together* under real code. This test is the first end-to-end proof that the whole port chain is correct enough to run the actual operating system.

---

## 2026-04-09 — C64 chip port wave + archive cleanup

**Type:** ingest + lint
**Source:** Four-chip C64 port wave (`mos-6502`, `mos-cia-6526`, `mos-sid-6581`, `mos-vic-ii`) followed by the archive cleanup commit once every chip had a verified replacement with a passing test suite.
**Pages created:** `chips/mos-sid-6581.md`, `chips/mos-vic-ii.md`
**Pages updated:** `chips/mos-cia-6526.md` (flipped from "planned" to "ported"), `decisions/archives-as-source.md` (per-subsystem table now marks every C64 chip as ported with commit hashes; added the second cleanup-history row; added a "how to read deleted paths" note pointing at `git show`), `index.md` (three chip pages added to the Chips section)
**Key decisions:** Each C64 chip went through the same "port → verify → stub wiki → commit" loop. The cleanup is a deliberate second pass that happens after *all* the replacements are landing and verified, not interleaved with the ports themselves — this keeps the audit trail coherent (one cleanup commit in each archive, one doc commit in the Emu198x wiki, all referencing each other). Emu198x-backup was consulted as a cross-reference during every chip port but not deleted — it remains the second-opinion reference for future chip work.
**Emu198x commits:** `2d42f8b` (mos-6502 tick), `cf7d0e7` (mos-cia), `49128bf` (mos-sid), `7ac5a65` (mos-vic-ii), plus the cleanup doc commit that bookends this entry.
**Archive commits:** `Emu198x-archive` `6bdc617d3a` (removed 5 crates); `Emu198x-archive-april2026` `bd942d9` (removed cpu-6502).

---

## 2026-04-09 — Archive source correction + C64 chip source map

**Type:** lint + ingest
**Trigger:** Post-`mos-6502` session planning — grepped the wiki for "which archive should the CIA come from" and nothing surfaced, because per-chip sourcing was never written down. Then the `archives-as-source.md` decision record said `Emu198x-backup` was *"probably nothing useful"*, which turned out to be wrong — the backup has functional `cia.rs` / `sid.rs` / `vic_ii.rs` / `c64.rs` implementations in `systems/c64/src/`.
**Pages updated:** `decisions/archives-as-source.md` (added per-subsystem source map, corrected the Emu198x-backup table row and "Best for" column, left an audit-trail note recording the correction), `index.md`
**Pages created:** `chips/mos-cia-6526.md` (stub with pin-contract sketch, port sources, subsystems, test plan — to be fleshed out during the port session)
**Key decisions:** For each C64 chip, the primary source is whichever archive has the most complete implementation (March archive for CIA / SID / VIC-II; April archive for CPU, already ported); the backup is a second reference for chip-level code that wasn't acknowledged before. Future sessions should consult the per-subsystem table before porting any chip.

---

## 2026-04-08 — Phase 0.6 / 0.7 architectural decisions

**Type:** ingest
**Source:** Phase 0 refactor wave — `SpectrumDriver` trait (0.6, commits `fc657b5` + `a3c1e48`) and `Peripheral` trait (0.7, commit `8cfdee1`).
**Pages created:** decisions/spectrum-driver.md, decisions/peripheral-trait.md
**Pages updated:** index.md
**Key decisions:** Within the Spectrum family, one shared run loop via a provided-method trait with `#[inline(always)]` hooks — a measured requirement, not a stylistic preference. Peripheral integration uses static dispatch (typed fields per machine), not a `Vec<Box<dyn Peripheral>>`, because the hot path is inliner-sensitive and every peripheral is known at compile time. Memory-bus intercepts (Beta disk TR-DOS ROM, Interface 1 shadow ROM, Multiface banking) deliberately stay machine-side until a second consumer justifies adding `read_memory` to the trait.

---

## 2026-04-05 — Wiki audit and sync

**Type:** lint
**Findings:** Wiki was behind by ~6 commits. Missing: nec-upd765a chip crate (22nd crate), .SNA snapshot format, WD1793 now functional (was stub), .TRD/.DSK disk support, ZIP archive loading, Timex SCLD hi-res modes (704px framebuffer). Serde partially applied (chips yes, Z80/machines not yet).
**Pages created:** chips/nec-upd765a.md
**Pages updated:** systems/spectrum/overview.md, tests/spectrum.md, decisions/save-state-format.md, index.md

---

## 2026-04-05 — Infrastructure decisions (GUI, serialisation, run loops)

**Type:** ingest
**Source:** Brainstorm Q&A — open questions for Phase 1
**Pages created:** decisions/native-ui-strategy.md, decisions/save-state-format.md, decisions/system-specific-run-loops.md
**Key decisions:** Platform-native frontends long-term, SDL2+native menus for October. serde/bincode for save states. No universal run loop — each system matches its hardware. run_frame() is the system trait boundary.

---

## 2026-04-05 — Long-term system coverage brainstorm

**Type:** ingest
**Source:** Brainstorm continuation — beyond October
**Pages updated:** decisions/product-roadmap.md, index.md
**Key decisions:** Rebuild all 35+ systems at new accuracy bar. Per-system standalones + unified launcher. Wave 2 by historical significance (Atari 2600, BBC Micro, MSX, Master System). All CPU cores cycle-perfect. Chip reuse map documented.

---

## 2026-04-05 — Product roadmap brainstorm

**Type:** ingest
**Source:** Brainstorm session — bridging accuracy to product
**Pages created:** decisions/product-roadmap.md
**Pages updated:** index.md
**Key decisions:** Four Code198x platforms (Spectrum→C64→NES→Amiga), same accuracy bar as Spectrum for all, capture pipeline + CRT as must-haves for October, WASM post-launch.

---

## 2026-04-05 — Initial seed

**Type:** ingest
**Sources:** Emu198x memory files, ARCHITECTURE.md, RULES.md, SPECTRUM-VARIANTS.md, brainstorm docs
**Pages created:** 20 pages across chips/, systems/spectrum/, concepts/, decisions/, tests/, references/
**Notes:** Migrated accumulated knowledge from flat memory files into cross-referenced wiki structure. All content verified against current codebase state (12 crates, 11,500 lines, all 6 Spectrum variants booting).
