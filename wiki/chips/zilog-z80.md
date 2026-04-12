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

### MStep types (14)

`FetchByte`, `FetchByteHi`, `FetchDisp`, `ReadAddr`, `ReadAddrHi`, `WriteAddr`, `WriteAddrHi`, `PushHi`, `PushLo`, `PopLo`, `PopHi`, `IoRead`, `IoWrite`, `Internal(n)`, `IntAck`, `Execute`

### Key design decisions

- **Execute is 0 T-states** — processed immediately by `try_complete_step`
- **Staged data** — `cs.data_lo`/`data_hi`/`addr` populated by MSteps, consumed by Execute
- **Conditional branches** (JR cc, DJNZ, CALL cc): use truncation (`cs.done`) for not-taken path
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

As of 2026-04-12 in the fresh Rust workspace, the Tom Harte corpus has been rerun locally and passes in full. The ZEX harness now supports checkpoint-targeted reruns keyed to the exerciser's own labelled blocks, but full fresh-workspace end-to-end reruns for ZEX and FUSE are still outstanding.

| Suite | Result |
|-------|--------|
| Tom Harte | 1,604,000 / 1,604,000 (100%) — rerun locally on 2026-04-12 |
| ZEXDOC | Harness wired to local binaries with checkpoint-targeted reruns; full rerun pending |
| ZEXALL | Harness wired to local binaries with checkpoint-targeted reruns; full rerun pending |
| FUSE | Reference target; fresh-workspace rerun pending |
