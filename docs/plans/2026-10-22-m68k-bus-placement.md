# 68000 bus placement within instructions

Lane: engineering frontier (680x0 accuracy). Steve asked for this on
2026-10-22, after the instruction clock totals were fixed.

## Problem

Every 68000 instruction took the right total clocks (`harte_bus_totals`),
but where the clocks fell inside an instruction was often wrong. On the
Amiga that matters: the CPU shares chip RAM with DMA, so an access that
lands a few clocks early can take a different slot, and timing-critical
code (demos, copy protection) can see it.

SingleStepTests records each row's bus transactions, and
`harte_bus_sequence_report` compares ours clock by clock. Before this work
345,432 of the 821,973 non-address-error rows matched exactly.

## What the corpus showed

1. **Trailing idle after the final prefetch.** The 68000 prefetches the
   next word, then spends the instruction's last internal clocks (`r n4`
   for ADDA, `r n16` for shifts, `r n2` for EXG). The core spent them first.
   Exceptions spend them first on hardware too: Bcc not taken, DBcc,
   DIVU/DIVS, RESET, and the last two clocks of an indexed LEA or PEA.
2. **The final prefetch before the last write.** Read-modify-write
   instructions, MOVE to -(An), PEA (except from an absolute address),
   ABCD/SBCD/ADDX/SUBX to memory prefetch before their last write.
   MOVE to other destinations, JSR, MOVEM and TAS write first.
3. **Word order.** Read-modify-write longs, MOVE.l and MOVEM.l to -(An)
   write the low word first. ADDX/SUBX.l -(An) read each operand low word
   first and prefetch between the two writes. RTE and RTR read the PC's
   high word, then SR or CCR, then the PC's low word.
4. **Smaller orderings.** LINK takes its displacement before pushing.
   MOVE from memory to an absolute long address refills past the
   address's second word after its write. The BTST/BCHG/BCLR/BSET #n,Dn
   and BTST Dn,#imm clocks follow their prefetch.

## Design

An **early final prefetch**:

- `MicroOp::PrefetchAhead` (appended to the shared enum in
  `motorola-68k-common`, so saved queues keep their discriminants) reads the
  word after the next opcode into `prefetch_ahead` and advances
  `next_fetch_addr`. IRC still holds the next opcode, so an exception
  before promotion stacks the right PC; exception entry discards the word.
- Promotion installs the held word in IRC instead of fetching it.
- An instruction that has made its final prefetch is promoted at the end of
  its last clock (`retire_fetched_ahead_instruction`), and the next
  instruction decodes on the following tick, its own first clock. Promoting
  and decoding in one tick would leave no boundary between them, and the
  debugger, CPU trace and tests that stop at an instruction boundary would
  see the next instruction's effects.
- Where the prefetch goes: `place_final_prefetch` puts it ahead of an
  instruction's trailing clocks once nothing else is left (with the
  hardware's clocks-first exceptions), `push_trailing_clocks` does it where
  a register writeback step follows the clocks, and `queue_final_write_ops`
  puts it ahead of a last write.
- An odd long access faults at its base address whichever half goes first.
- Interrupts are still recognised at promotion. Whether the 68000 does so
  before or after trailing clocks is not settled by either corpus.
- 68020+ timing is unchanged: everything is gated on `mc68000_timing()`.

`prefetch_ahead` is live state between the early prefetch and promotion,
so it is saved. The Amiga runtime envelope advances from 64 to 65 and the
savestate decision record has an amendment.

## Result

- `harte_bus_sequences` (was `harte_bus_totals`) now gates exact
  agreement: every access, its address, and the idle clocks between. All
  819,977 rows outside the two known corpus-error families match; those
  1,996 rows stay pinned.
- SingleStepTests final state stays at 1,000,058/1,000,058, and the MAME
  agreement and address-error sweeps pass.
- Unit tests cover promotion from the held word (one bus cycle for
  ADDA.W An,An, and a clean boundary) and its snapshot round trip.
