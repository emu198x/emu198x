# Zilog Z80

Half-cycle signal-level state machine. Each `tick()` advances one phase (e.g. `M1_T1_Rise` → `M1_T1_Fall`). Used by: [ZX Spectrum](../systems/spectrum/overview.md), MSX (planned), Amstrad CPC (planned).

## Crate

`zilog-z80` — standalone, no system dependencies.

## Signal interface

Output signals: `addr`, `data`, `mreq`, `iorq`, `rd`, `wr`, `m1`, `rfsh`, `halt`.
Input signals: `data_in`, `wait`, `irq`, `nmi`.

No Bus trait. The machine inspects signals directly and performs bus transactions. See [No Bus Trait](../decisions/no-bus-trait.md).

## MStep walker

Instructions decompose into sequences of MStep operations (~50 static arrays in `mcycle.rs`). The walker processes one T-state per `tick()` call.

### MStep types (17)

`FetchByte`, `FetchByteHi`, `FetchDisp`, `ReadAddr`, `ReadAddrHi`, `WriteAddr`, `WriteAddrHi`, `PushHi`, `PushLo`, `PopLo`, `PopHi`, `ContendPc`, `IoRead`, `IoWrite`, `Internal(n)`, `IntAck`, `Execute`

### Key design decisions

- **Execute is 0 T-states** — processed immediately by `try_complete_step`
- **Staged data** — `cs.data_lo`/`data_hi`/`addr` populated by MSteps, consumed by Execute
- **Conditional relative branches** (`JR cc`, `DJNZ`): decode chooses taken vs not-taken timing up front so the not-taken path can model the real contended `PC` cycle without a false displacement read
- **Conditional calls / returns** (`CALL cc`, `RET cc`): still use the staged conditional path machinery
- **RET cc**: sequence switching after `Internal(1)` condition check
- **Block repeat ops** (LDIR etc.): switch to non-repeat sequence when done (not `cs.done`, which would skip WriteAddr)
- **DD/FD pass-through**: unprefixed opcodes walk their unprefixed sequence (prefix reset to 0)
- **DDCB/FDCB**: post-step hooks save sub-opcode from `data_lo` after FetchByte, compute indexed address after FetchDisp

### M1 cycle (opcode fetch)

4 T-states: contend at T1, `m1_read` at T2, refresh+contend at T3, decode at T4.

### Prefix handling

CB/DD/ED/FD trigger a second M1 fetch. DD/FD chains. DD+ED overrides. DD+CB enters DDCB mode.

### Interrupts

- **INT (IM 1)**: `IntAck(7T)` + `Execute` + `PushHi` + `PushLo`
- **INT (IM 2)**: `IntAck(7T)` + `Execute` + `PushHi` + `PushLo` + `ReadAddr` + `ReadAddrHi` + `Execute`
- **NMI**: `Internal(5)` + `Execute` + `PushHi` + `PushLo`

## Block I/O flag formulas

The fix for the final 1,996 Tom Harte failures:

- INI: `k = data + ((C+1) & 0xFF)`
- IND: `k = data + ((C-1) & 0xFF)`
- OUTI/OUTD: `k = data + L_after` (L after HL adjustment)
- Flags: S/Z/bits 3,5 from `B_after`. N from `data` bit 7. H=C from `k > 0xFF`. P = parity of `(k & 7) ^ B_after`.

## Test results

As of 2026-04-12 in the fresh Rust workspace, the Tom Harte corpus passes in full, `zexdoc` and `zexall` both pass end-to-end, and the local ZEX runner supports checkpoint-targeted reruns plus cached resume. FUSE is now rerun as an exact-trace compatibility harness over events, final state, memory effects, and final T-state counts.

| Suite | Result |
|-------|--------|
| Tom Harte | 1,604,000 / 1,604,000 (100%) — rerun locally on 2026-04-12 |
| ZEXDOC | Pass — end-to-end rerun locally on 2026-04-12 |
| ZEXALL | Pass — end-to-end rerun locally on 2026-04-12 |
| FUSE | 1,350 / 1,356 exact matches, 6 accepted disagreements, 0 unexpected — exact event trace rerun locally on 2026-04-12 |

## Sources

**Primary documentation**
- *Z80 CPU User Manual* (Zilog, UM008011-0816) — opcode set, signal protocol, M-cycle/T-state timing diagrams.
- *The Undocumented Z80 Documented* — Sean Young — undocumented opcodes, block I/O flag behaviour, MEMPTR (WZ register).

**Test corpora**
- **Tom Harte single-step tests** — JSON corpus at `~/Projects/Emu198x-Unclean/ProcessorTests/z80/v1`.
- **ZEXDOC / ZEXALL** — Frank Cringle's documented/all flag exerciser.
- **FUSE test suite** — `tests.in` / `tests.expected` event traces from the FUSE source tree.

**Reference emulators consulted**
- **SpecIde** (C++) — half-cycle ULA/Z80 interleaving model, closest in shape to ours. See [references/emulators.md](../references/emulators.md).
- **FUSE** (C) — authoritative timing data; event-driven architecture, not our approach.
- **z80cpp** (C++) — clean standalone core, sanity-check on instruction decode.

Block I/O flag formulas (§ above) derived from Sean Young's notes, validated against Tom Harte. See [decisions/half-cycle-signals.md](../decisions/half-cycle-signals.md) for why we chose the signal-level model over event-driven.
