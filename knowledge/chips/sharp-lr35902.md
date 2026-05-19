# Sharp LR35902 (SM83)

> Status as of 2026-04-23: **`sharp-lr35902` crate ported and
> validated.** Pin-level m-cycle CPU; 92 unit tests + 49,600 Adam
> Tennant single-step tests passing (every opcode covered by the
> corpus, plus 25,600 CB sub-table permutations). HALT, STOP, DI,
> EI, and 11 illegal opcodes covered by unit tests since the corpus
> intentionally omits them. Source of truth for hardware behaviour
> while porting was the Zig implementation at
> `~/Projects/Emu198x-Zig/src/sm83.zig` (1858 LOC).

The CPU inside the Nintendo Game Boy. Commonly called the "SM83"
after its internal codename; the die is the Sharp LR35902, a
system-on-chip that also carries the APU, timer, DMA, serial,
interrupt controller, and joypad registers alongside the core.

## Crate

`sharp-lr35902` — pin-level m-cycle state machine. Standalone, no
system dependencies. Used by: [Nintendo Game Boy](../systems/nintendo-game-boy/overview.md)
(DMG + CGB).

## Instruction set

Not a Z80. Not an 8080. A custom 8-bit core that sits between them:

- **Z80-style register pairs** (`BC`, `DE`, `HL`, `AF`) with 8-bit
  halves, 16-bit SP and PC.
- **No IX/IY index registers.** The Z80's `DD`/`FD` prefixes don't
  exist.
- **No shadow registers.** The Z80's `EX AF, AF'` and `EXX` don't
  exist.
- **No `IN`/`OUT` opcodes.** Memory-mapped I/O via the `$FFxx` HRAM
  page, reached through `LDH` / `LD (C), A` style ops.
- **Added opcodes** over 8080: `LDH`, `LD HL, SP+r8`, `ADD SP, r8`,
  `SWAP` (CB-prefixed), `STOP`, the `RETI` return-and-enable-IRQs.
- **Removed opcodes:** Z80's block moves, bit-set-by-register,
  relative jumps with the full condition set.
- **CB prefix:** 256 bit-manipulation opcodes (`BIT`, `SET`, `RES`,
  `RL`, `RR`, `RLC`, `RRC`, `SLA`, `SRA`, `SRL`, `SWAP`) over every
  register and `(HL)`.

The F register has four flags: **Z** (zero), **N** (add/sub), **H**
(half-carry), **C** (carry). The low 4 bits are hardwired to zero;
any write to F masks them off.

## Abstraction level

**M-cycle, not T-cycle.** See [the decision](../decisions/sm83-abstraction-level.md).
One `tick()` advances one 4-T-state machine cycle. The machine
inspects pin state between ticks.

## Signal Interface

Output pins: `addr` (u16), `data` (u8), `rd` (bool), `wr` (bool),
`mreq` (bool; high for bus-active m-cycles, low for internal-only).
Input pins: `data_in` (u8), `irq` (u8 bitfield of IE & IF), `halt`
(read-only mirror for the machine to know the CPU is halted).

No Bus trait. See [CPU bus interface](../decisions/cpu-bus-interface.md).

## State Machine Shape

Lifted from `sm83.zig`:

- `m_cycle: u3` — position within the current instruction (0 for
  opcode fetch).
- `opcode: u8` — latched during the fetch m-cycle; gates all later
  m-cycles for the instruction.
- `cb_prefix: bool` — set by `CB` prefix, cleared after the next
  opcode.
- `z: u8`, `w: u8` — internal scratch bytes (the Z80 analogue is the
  `MEMPTR` register; SM83 uses similar internal staging).
- `ime: bool` + `ime_pending: bool` — interrupt-master-enable with
  the one-instruction delay between `EI` and the enable taking
  effect.
- `halted: bool`, `stopped: bool` — HALT bug and STOP-mode state.

## Interrupts

5 interrupt sources, prioritised high-to-low:

1. VBlank (bit 0) — PPU mode-1 entry.
2. LCD STAT (bit 1) — PPU mode/LYC match.
3. Timer (bit 2) — TIMA overflow.
4. Serial (bit 3) — SIO transfer complete.
5. Joypad (bit 4) — input register edge.

IF ($FF0F) latches, IE ($FFFF) masks. Dispatch takes 5 m-cycles:
internal-wait, internal-wait, push-PCH, push-PCL, set-PC-to-vector.

The "HALT bug" (HALT with IME=0 and a pending interrupt causing the
next opcode to be read twice) is an m-cycle-visible edge case and is
implemented in the CPU crate, with system-level coverage from Blargg
and mooneye-gb.

## Test coverage

- **Crate unit tests:** 92 tests covering reset, every opcode group,
  HALT (with halt-bug latch), STOP, DI/EI (with the one-instruction
  IME delay), and the full interrupt dispatch sequence (priority,
  cancelled-IRQ, IME-clear no-op).
- **Adam Tennant single-step corpus** (Tom Harte format) at
  `~/Projects/Emu198x-Unclean/GameboyCPUTests/v2/` — 49,600 tests
  total: 100 per top-level opcode + 25,600 for the CB sub-table.
  100% pass. The corpus omits HALT/STOP/DI/EI plus the 11 illegal
  opcodes; those gaps are filled by the unit tests above. Harness
  lives at `crates/sharp-lr35902/tests/single_step_tests.rs` (run
  with `--ignored`); the pipelined-model adapter is documented in
  the test file's header comment.

System-level coverage now lives in
`crates/runtime-nintendo-game-boy/tests/phase2_verification.rs`:

- Blargg `cpu_instrs` — full 11-sub-test ROM (overlaps Tennant),
  passing locally.
- Blargg `instr_timing` — m-cycle count per instruction, passing
  locally.
- Blargg `mem_timing` v1 + v2 — bus-access timing, passing locally.
- mooneye-gb acceptance suite — IME delay, HALT bug, TIMA-reload
  behaviour, and other edge cases at m-cycle precision, passing
  locally.

## Related

- [SM83 abstraction level](../decisions/sm83-abstraction-level.md) —
  why m-cycle.
- [CPU bus interface](../decisions/cpu-bus-interface.md) — pin-level
  rule.
- [Nintendo Game Boy](../systems/nintendo-game-boy/overview.md) —
  the family this chip serves.
- [Game Boy timing](../systems/nintendo-game-boy/timing.md) — master
  clock, m-cycle constants.
