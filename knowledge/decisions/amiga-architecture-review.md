# Decision: Amiga architecture review — tighten the seams, not the spine

**Date:** 2026-04-19
**Status:** Implemented (closed 2026-05-21). Superseded by
[`amiga-full-family-architecture-review.md`](amiga-full-family-architecture-review.md)
for forward-looking scope.

## Closeout (2026-05-21)

All five seams landed between 2026-04-19 and 2026-05-21:

- **Seam 1** (`service_cpu_bus` → `BusTransaction` / `BusResponse`) —
  commit `8b37751`. Machine file 3099 → 2016 lines; dispatcher
  splits per chip-select.
- **Seam 2** (disk DMA path into Paula) — commit `fa49c61`.
  `Paula8364::tick_disk_dma_slot` owns WORDSYNC + arm flip-flop;
  machine layer is thin glue.
- **Seam 3** (byte-write merge locality) — solved differently than
  proposed: per-arm byte-write handling in each `dispatch_*` function
  rather than chip-side `read_register_word`. Same locality outcome.
- **Seam 4** (byte-lane response conventions) — folded into Seam 1.
  `BusResponse` enum (`Byte` / `Word` / `WriteAck` / `Float`) is the
  canonical surface.
- **Seam 5** (boot invariants) — commit `85d962b`.
  `runtime-commodore-amiga/tests/boot_invariants.rs` (420 lines)
  covers all four anchor families.

Done-criteria status from the original review: Workbench 1.3 boots
to desktop ✅, boot invariants in CI ✅, `service_cpu_bus` thinned ✅,
Paula owns the DSKLEN/WORDSYNC/countdown state machine ✅, byte-write
merge solved via per-arm locality ✅. The 2026-07-31 accuracy audit found
that the board-level floppy transfer still advances from an independent
track pacer rather than consuming Agnus's disk-DMA grants. That later
arbitration gap is tracked by the
[Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md);
it does not reopen the ownership/locality result this review established.

The follow-on review at
[`amiga-full-family-architecture-review.md`](amiga-full-family-architecture-review.md)
tackles seams that have emerged since this one landed — covering
the full Amiga family (OCS / ECS / AGA / CDTV / CD32 / future).

---

## What this is

A targeted review of the Amiga implementation against the load-bearing decisions already in place ([CPU bus interface](cpu-bus-interface.md), [No Bus trait](no-bus-trait.md), [ULA-drives model](ula-drives-model.md), [System-specific run loops](system-specific-run-loops.md), [Amiga port plan](amiga-port-plan.md)). The Amiga is the first system in the roadmap that exercises the architecture under realistic stress: multi-master bus arbitration, DMA-driven I/O, six chips collaborating per CCK. This review names the friction points that have produced repeated bugs during the Kickstart-boot push, and proposes scoped fixes for each — without disturbing the spine.

The spine stays. The seams need work.

## What we are *not* changing

These decisions are load-bearing and have proven themselves on the Spectrum, NES, and C64. Nothing in this review revisits them:

- Master oscillator drives the loop (`tick_cck` for Amiga, equivalent for each system).
- Pin-level CPU bus interface for every CPU. No Bus trait, ever.
- Chip-as-trait/struct-with-pins. One implementation per variant.
- Manufacturer-chipname crate naming.
- Per-system run loops; no universal tick pattern.
- Half-cycle Z80; one clock period per step for the 6502 and current 68000
  core. The 68000 minimum bus cycle spans four steps.

If the review below appears to require revisiting any of these, the review is wrong and the decision wins.

## The five seams

### Seam 1 — `service_cpu_bus` in `machine-commodore-amiga`

**Current state.** A 3099-line `lib.rs` whose `service_cpu_bus` function is the central traffic cop between the 68000 pin state and every chip on the bus. It reads `cpu.state`, decodes the address through Gary, and dispatches reads/writes per chip-select. CIA-A, CIA-B, custom registers (Agnus/Denise/Paula), chip RAM, slow RAM, ROM, autoconfig, and unmapped space each have their own arms.

**Friction observed.** Two of the three boot-blocker bugs this sprint lived here:

- The CIA register double-read (commit `3ab4329`) — the function was re-entered between `cycle_count >= 2` (DTACK sample) and `cycle_count >= 4` (CPU latches), repeating the side-effecting CIA read.
- Byte-vs-word lane handling for CIAs and custom registers, with four different conventions in flight (`0xFF00`, `0x00FF`, "always low byte", per-chip merge).

**Diagnosis.** The seam between "CPU exposes pins" and "machine performs the bus transaction" is doing too much in one function, with too many ad-hoc conventions. The architecture is correct; the implementation is brittle.

**Proposed change.** Extract a small `BusTransaction` struct that captures `(addr, fc, is_read, is_word, data)` and represents the *intent* of the bus cycle exactly once per cycle. `service_cpu_bus` becomes a guard that produces at most one `BusTransaction` per cycle and dispatches it. Per-chip-select handlers take a `BusTransaction` and return a `BusResponse` (the value to drive on D0-D15, or a side-effect-only acknowledgement). One canonical helper (`response_byte_low(value: u8)`, `response_byte_high(value: u8)`, `response_word(value: u16)`, `response_float()`) replaces the per-arm `0xFF00`/`0x00FF` placeholders.

**Scope.** Restructuring within `machine-commodore-amiga`. No public API change. ~400 lines moved, no new crate.

**Why this matters for other systems.** Every multi-master system (Atari ST, Acorn Archimedes, Amiga 1200, possibly BBC Micro with the 6845) will have the same seam. Fixing the pattern here means copy-pasting a known-good shape, not re-discovering the same bugs.

### Seam 2 — Disk DMA path straddling four crates

**Current state.** The disk DMA read path touches four crates per word:

- `peripheral-commodore-amiga-floppy` produces the encoded MFM track bytes.
- `commodore-agnus-ocs` allocates slots 0x04-0x06 to disk DMA when DMACON bit 4 is set.
- `commodore-paula-8364` owns DSKLEN/DSKPT/DSKSYNC/ADKCON state, the arming flip-flop, and DSKBYTR.
- `machine-commodore-amiga::service_disk_dma_slot` (lib.rs:1126-1213) actually performs the byte writes to chip RAM, drives WORDSYNC suppression, and fires DSKSYN/DSKBLK interrupts.

**Friction observed.** WORDSYNC handling is the residual blocker for Workbench boot (six wrong cooked-longs per bootblock, two identical `$0000021C` values at sec0[0] and sec1[0] — alignment-class symptom). The suppression logic lives in the machine layer, not in Paula, even though it is fundamentally a Paula behaviour. The arming flip-flop lives in Paula; the DMA byte transfer lives in machine. The MFM-decoded bytes flow from floppy → machine → chip RAM, never touching Paula's DSKBYTR/DSKDATR latches except for the diagnostic `note_disk_read_word` call.

**Diagnosis.** The responsibility split is wrong. Paula is the disk controller — it owns the read/write/sync state machine on real silicon. In our code, the machine layer owns half of it. That split is what makes the WORDSYNC bug hard to localise: you have to read Paula's state, the machine's `runtime`, and the floppy's encoder side by side.

**Proposed change.** Move the disk read state machine into `commodore-paula-8364`. Paula gains a `tick_disk_dma_slot(&mut self, fetch: impl FnOnce() -> Option<u16>) -> Option<DiskDmaWrite>` API that takes the next encoded word from floppy and returns either a chip-RAM write (with address derived from DSKPT) or a sync-only acknowledgement. The machine layer becomes a four-line glue: ask Agnus if the slot is granted, ask Paula to handle it, perform the write to chip RAM if Paula returned one. WORDSYNC, sync-stripping, DSKBYTR/DSKDATR updates, and DSKBLK interrupt all live in Paula.

**Scope.** ~150 lines moved from `machine-commodore-amiga` into `commodore-paula-8364`. Paula gets a small new test surface (already-implemented MFM bits become testable in isolation). Public Paula API gains one method.

**Why this matters for other systems.** No system other than the Amiga uses DMA-driven floppy in the current roadmap, but the *pattern* — "chipset chip owns the I/O state machine; machine layer is glue" — applies to NES MMC mappers, C64 1541 IEC bus, and any system where a peripheral controller does its own DMA. Getting Paula right sets the template.

### Seam 3 — Custom register byte-write merge latch

**Current state.** `byte_merge_latch` (lib.rs:735-…) is a hand-maintained match arm listing every custom register that needs read-back-and-merge on byte writes. When a byte write to BPLCON0 arrives, the function returns the current `agnus.bplcon0` so the unwritten byte can be preserved.

**Friction observed.** Adding a new chip means remembering to update this list, in a different file from the chip itself. Forgetting it produces silent half-word corruption that only shows up when something writes a byte to a register normally written as a word. Easy to miss in review.

**Diagnosis.** Locality. The chip knows its own registers; the machine should not need to.

**Proposed change.** Each chip exposes `fn read_register_word(&self, offset: u16) -> Option<u16>` covering its register range. The machine asks the right chip when it needs the merge value. The chip-to-offset routing already exists for reads (`read_custom_reg`); merge is the same routing in inverse.

**Scope.** ~30 lines deleted from machine; ~10 lines added per chip (Agnus, Denise, Paula). Net reduction. No public API churn — the new method is additive.

### Seam 4 — Byte-lane response conventions

**Current state.** Four conventions in flight for "what u16 to put in `BusStatus::Ready` for a sub-word access":

- CIA-A byte read at odd address: low byte of u16.
- CIA-A even-address (or word) read: hard-coded `0xFF00`, ignoring the CIA.
- CIA-B byte read at even address: low byte of u16 (NOT high byte, despite CIA-B physically being on D8-D15).
- Custom register byte read at even address: high byte (matches physical lane).
- Chip RAM byte read: low byte.

The CPU model always extracts byte values from `data & 0xFF` regardless of address parity, so the "always low byte" convention is internally consistent — but the per-chip-select code does not state this convention anywhere, and one of the arms (custom registers) actually does it the other way.

**Friction observed.** The CIA-A even-address case skips the CIA read entirely. If anything in Kickstart ever does a `MOVE.W $BFE000.L, ...` (autoconfig probes do peek into CIA space during reset), an ICR-pending bit could go uncleared. Low probability today, but a landmine.

**Diagnosis.** No canonical helper. Every arm reinvents lane handling.

**Proposed change.** Define `BusResponse` as enum: `Byte(u8)`, `Word(u16)`, `Float`. The dispatcher in `service_cpu_bus` converts to the u16 the CPU expects, applying one rule consistently: byte responses go in low byte (matching CPU convention), `Float` returns `0xFFFF`. Chip-select arms produce `BusResponse`, never raw u16.

**Scope.** Folds into Seam 1. Same restructuring pass.

### Seam 5 — No standing per-system boot-invariant suite

**Current state.** Each Amiga boot bug this sprint produced a hand-rolled diagnostic example: `bootblock_check`, `paula_dma_buffer`, `signal_watch`, `microhz_handler_trace`, `cia_a_timer_b_trace`, etc. These are excellent for debugging. None of them are run automatically.

**Friction observed.** When a fix lands, there is no green-bar to confirm we did not regress an earlier waypoint. The diagnostic example for the now-fixed CIA double-read still exists, but nothing prevents the bug from coming back. Future Amiga work risks re-finding bugs we have already fixed.

**Diagnosis.** Diagnostic examples are append-only. They are not promoted into a regression suite when the bug they found is fixed.

**Proposed change.** Add `tests/boot_invariants.rs` to `runtime-commodore-amiga` and equivalent in C64/NES/Spectrum runtimes. One test per known waypoint:

- `boots_kickstart_to_insert_disk_within_n_frames`
- `accepts_workbench_disk_insertion`
- `seven_disk_dma_arms_within_3000_frames` (catches the CIA double-read regression)
- `bootblock_decodes_to_dos_magic` (catches the WORDSYNC fix regression, once landed)

Each test uses the existing headless runner and asserts on the existing query surface. No new infrastructure.

**Scope.** ~50 lines per system. Promoted from existing diagnostic examples one waypoint at a time, as bugs are fixed.

## Order of work

In order of leverage for unblocking the Amiga and protecting future systems:

1. **Seam 2 (disk DMA path)** — the actual boot blocker. Move the disk read state machine into Paula, fix WORDSYNC in the same pass, validate Workbench loads.
2. **Seam 1 + Seam 4 (CPU bus seam, byte-lane conventions)** — fold together since they touch the same file. `BusTransaction`/`BusResponse` types, refactor `service_cpu_bus` arms to use them.
3. **Seam 5 (boot invariants)** — promote the bootblock-decode and seven-disk-DMA-arms diagnostics to automated tests. Add the existing Kickstart-to-insert-disk test if not already present.
4. **Seam 3 (merge latch locality)** — chip-owned `read_register_word`. Lowest urgency since the current code is correct; this is a future-proofing pass.

## Done criteria

- Workbench 1.3 boots to the desktop in the headless runner.
- Boot-invariant tests run in CI and fail loudly on regression.
- `service_cpu_bus` is under 200 lines and all chip-select arms produce `BusResponse`.
- Paula owns the disk-controller state machine; the machine layer retains only
  memory, media-stream and bus-grant glue.
- Each chip owns its own register-word read for byte-write merging.
- This document is updated with implementation status and links to commits.

## Non-goals

- Refactoring the 68000 internals. Tom Harte 1,000,058/1,000,058 is the bar; nothing here changes the CPU.
- Touching Agnus's slot allocation. The slot table is correct against HRM Table 6-1.
- Splitting `machine-commodore-amiga` into more crates. The single crate is fine; the seam-tightening is internal.
- Anything in the runtime/headless-runner layers. Those are working.

## Related

- [Amiga port plan](amiga-port-plan.md) — the plan that produced the current baseline
- [CPU bus interface](cpu-bus-interface.md) — the load-bearing decision this review preserves
- [No Bus trait](no-bus-trait.md) — the rule the seam fixes uphold
- [System-specific run loops](system-specific-run-loops.md) — the pattern the seam fixes inherit
