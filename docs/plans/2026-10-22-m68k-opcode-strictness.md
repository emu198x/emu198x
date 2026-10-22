# 68000 opcode strictness

Lane: engineering frontier (680x0 accuracy), found while fixing #1966.

## Problem

Run across all 65,536 words, the 68000 core executed 6,672 opcodes that the
disassembler rejects, and 2,160 of them panicked. The disassembler is
checked against the Isa198x spec over the whole opcode space
(`tests/isa_conformance.rs`), so it is the oracle here. The 68020 shares the
decoder and panicked on 2,166. SingleStepTests covers only legal
instructions, so nothing caught it.

ILLEGAL itself ran as TAS; that was fixed in #1975.

## Decision

Steve, 2026-10-22: per-instruction checks in the decoder, as TAS and Scc
already do, rather than a legality table built from the disassembler. The
executor stays independent of the disassembler, which is only the test
oracle. The 68020 widens some 68000 addressing modes, so each check allows
the wider form where the variant enables it.

## Inventory

A throwaway probe ran every word the disassembler rejects on both CPUs:

| 68000 | 68020 | Words | What it means |
|---|---|---|---|
| panic | panic | 2,160 | undefined mode 7, register 5-7 |
| ran | ran | 4,498 | 68000 checks missing; some are legal 68020 widenings |
| vector 4 | ran | 889 | 68020 hook claims them (MULL, DIVL, bit fields, CAS, CHK2, PACK, TRAPcc, ...) |
| vector 4 | panic | 6 | undefined mode in the 68020 hook |
| vector 10/11 | same | 8,192 | line A and line F, correct |

Base-decoder widenings the 68020 needs (MC68020 user's manual): TST with
An, PC-relative and immediate operands; CMPI with PC-relative operands;
CHK.L; LINK.L. The 68020-only instructions live in the hook.

## Steps

1. **No panics.** Every effective-address decode goes through `ea_mode`,
   which takes the exception for the undefined mode 7, register 5-7 (line F
   for $Fxxx, otherwise illegal, as WinUAE's op_illg does). Tests: no word
   panics on the 68000 or the 68020. Verify both SingleStepTests corpora,
   the 680x0 and Amiga suites.
2. **68000 legality, one instruction group per PR.** Add the PRM's mode
   restrictions to each group's decode (immediate group, MOVE, misc $4xxx,
   ADDQ/SUBQ/Scc, the ALU groups $8xxx-$Dxxx, memory shifts), with the
   68020 widenings behind a variant flag. Widen `illegal_opcodes.rs` to
   that group's opcodes as each lands, so the whole space is covered at the
   end.
3. **68020 regression set.** The 889 words the 68020 hook claims, and the
   widenings above, must keep running on the 68020. Pin them in the 68020
   crate's tests.

Each step must keep SingleStepTests at 1,000,058/1,000,058 and the MAME
agreement sweep green.
