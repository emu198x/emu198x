# decode.rs Rewrite Plan

## Problem

`crates/emu-m68k/src/m68000/decode.rs` was written in one session (~1,400 lines) without incremental verification. Each new bug found during KS boot requires a full debugging session to trace. Known bugs: SkipExt double-advance in MOVE (fixed), unknown instruction bug causing wrong D1 value in memory detection loop. Unknown count of latent bugs.

The recipe/micro-op engine underneath is sound. The `M68kBus` trait, `tick()` method, micro-op execution in `mod.rs`, and recipe expansion in `recipe.rs` all work correctly. The rot is isolated to the decode layer.

## Approach

Rewrite decode.rs one instruction group at a time. After each group, run the corresponding single-step tests (`test-data/m68000-dl/v1/`) and fix until 100% pass. Never move to the next group with failures outstanding.

127 test files exist covering every 68000 instruction. Currently 77,972/317,500 pass (24.6%). Target: close to 100% (the remaining ~75% are mostly instructions that trigger illegal-instruction exceptions because decode.rs didn't implement them, plus genuine timing/flag bugs).

## What stays

- `recipe.rs` — RecipeOp execution, micro-op expansion. Tested and working.
- `mod.rs` — tick engine, micro-op dispatch, wait cycle consumption. Working.
- `ea.rs` — EA calculation, `next_ext_word()`. Working.
- `execute_shift.rs`, `microcode.rs`, `timing.rs`, `exceptions.rs` — keep.
- `observable.rs` — keep.

## What gets rewritten

- `decode.rs` — delete and rebuild from scratch, group by group.
- Debug traces — remove the HashSet/TRACE machinery. Add only targeted traces when debugging specific issues.

## Verification command

Run a specific instruction's tests:
```
cargo test -p emu-m68k --test single_step_tests -- SWAP --nocapture 2>&1 | tail -5
```

Run all tests (slow, ~10min):
```
cargo test -p emu-m68k --test single_step_tests run_all_single_step_tests -- --ignored --nocapture
```

Run a batch of related tests:
```
for f in MOVE.b MOVE.w MOVE.l MOVEA.w MOVEA.l MOVE.q; do
  echo "=== $f ==="
  cargo test -p emu-m68k --test single_step_tests -- "$f" --nocapture 2>&1 | grep -E "passed|failed"
done
```

## Phases

### Phase 0: Clean slate

Delete the body of `decode_and_execute()`. Replace with a single `self.illegal_instruction()` fallback. Every instruction triggers illegal exception. Run full test suite to confirm baseline (should be ~0 passes since every opcode goes to illegal).

Commit: "decode.rs: strip to skeleton for incremental rebuild"

### Phase 1: Data movement basics

**Instructions:** MOVE.b/w/l, MOVEA.w/l, MOVEQ, LEA
**Test files:** MOVE.b, MOVE.w, MOVE.l, MOVEA.w, MOVEA.l, MOVE.q, LEA
**Tests:** ~17,500 (7 files x 2,500)
**Why first:** Most-executed instructions. MOVE exercises the full EA machinery (all addressing modes as src and dst). Getting MOVE right validates the extension word flow, CalcEa, ReadEa, WriteEa, and FetchExtWords.
**Key risk:** Extension word indexing for MOVE with both src and dst ext words. The SkipExt bug lived here. Build carefully: start with register-to-register, then add addressing modes one at a time.

### Phase 2: Branches and flow control

**Instructions:** BRA, Bcc, BSR, JMP, JSR, RTS, NOP, DBcc
**Test files:** Bcc, BSR, JMP, JSR, RTS, NOP, DBcc
**Tests:** ~17,500
**Why second:** KS init uses these constantly. DBcc is the fill loop instruction. Getting these right means the CPU can execute sequential code and loops.

### Phase 3: Arithmetic and logic (register)

**Instructions:** ADD.b/w/l, SUB.b/w/l, CMP.b/w/l, AND.b/w/l, OR.b/w/l, EOR.b/w/l
**Test files:** 18 files
**Tests:** ~45,000
**Why third:** Core ALU operations. These exercise the two-operand recipe pattern (read src EA, read dst EA or reg, compute, write result). Flags must be correct.

### Phase 4: Address register arithmetic

**Instructions:** ADDA.w/l, SUBA.w/l, CMPA.w/l
**Test files:** 6 files
**Tests:** ~15,000
**Why here:** Needed for LEA-like patterns. ADDA/SUBA are long-only internally (word sign-extends). No flag changes.

### Phase 5: Immediate operations

**Instructions:** ORI/ANDI/SUBI/ADDI/EORI/CMPI (to EA), ORI/ANDI/EORI to CCR/SR
**Test files:** ANDItoCCR, ANDItoSR, ORItoCCR, ORItoSR, EORItoCCR, EORItoSR (+ the EA forms are tested via the ALU test files implicitly, but also have dedicated opcodes in group 0)
**Tests:** ~15,000
**Why here:** Group 0 immediate ops share the same pattern: read immediate from ext words, read EA, compute, write EA. The ext word handling is critical to get right.

### Phase 6: Quick operations and unary

**Instructions:** ADDQ, SUBQ, CLR.b/w/l, NEG.b/w/l, NEGX.b/w/l, NOT.b/w/l, TST.b/w/l, Scc, SWAP, EXT.w/l
**Test files:** CLR.b/w/l, NEG.b/w/l, NEGX.b/w/l, NOT.b/w/l, TST.b/w/l, Scc, SWAP, EXT.w/l (+ ADDQ/SUBQ via group 5)
**Tests:** ~30,000
**Why here:** These are simpler single-operand instructions. SWAP is the suspected current bug. Getting it verified here prevents cascading failures.

### Phase 7: Bit operations

**Instructions:** BTST, BCHG, BCLR, BSET (register and immediate forms)
**Test files:** BTST, BCHG, BCLR, BSET
**Tests:** ~10,000
**Why here:** Group 0 bit ops have tricky encoding (immediate vs register source). Needed for KS hardware detection.

### Phase 8: Stack and control flow

**Instructions:** PEA, LINK, UNLK (UNLINK), TRAP, TRAPV, RTE, RTR, MOVE from/to SR/CCR/USP
**Test files:** PEA, LINK, UNLINK, TRAP, TRAPV, RTE, RTR, MOVEfromSR, MOVEtoCCR, MOVEtoSR, MOVEfromUSP, MOVEtoUSP
**Tests:** ~30,000
**Why here:** Exception handling, privilege mode changes. KS uses RTE for interrupt returns, TRAP for system calls.

### Phase 9: MOVEM, MOVEP

**Instructions:** MOVEM.w/l (register-to-memory, memory-to-register), MOVEP.w/l
**Test files:** MOVEM.w, MOVEM.l, MOVEP.w, MOVEP.l
**Tests:** ~10,000
**Why here:** MOVEM is complex (register mask, predec mode reverses order). MOVEP is CIA access pattern (alternate bytes). Both are heavily used by KS.

### Phase 10: Multiply, divide, BCD

**Instructions:** MULU, MULS, DIVU, DIVS, ABCD, SBCD, NBCD
**Test files:** MULU, MULS, DIVU, DIVS, ABCD, SBCD, NBCD
**Tests:** ~17,500
**Why here:** Least common in early boot. DIVU/DIVS have complex timing (Cwik's algorithm already in execute.rs). BCD ops are niche.

### Phase 11: Extended arithmetic, exchange, misc

**Instructions:** ADDX.b/w/l, SUBX.b/w/l, EXG, TAS, CHK, STOP, RESET
**Test files:** ADDX.b/w/l, SUBX.b/w/l, EXG, TAS, CHK, STOP, RESET
**Tests:** ~20,000
**Why here:** Less common, but ADDX/SUBX needed for multi-precision math. TAS is atomic test-and-set.

### Phase 12: Shifts and rotates

**Instructions:** ASL/ASR/LSL/LSR/ROL/ROR/ROXL/ROXR (.b/w/l register, .w memory)
**Test files:** 24 files
**Tests:** ~60,000
**Why last:** Many test cases (3 sizes x 8 operations = 24 files). Shift timing depends on shift count. Memory shifts are word-only. Already have execute_shift.rs for the ALU.

## After all phases pass

1. Run full single-step suite. Target: 317,500/317,500 (or document remaining failures with root causes).
2. Run KS 1.3 headless boot. Verify fill loop completes in ~28 frames, not 3000.
3. Take screenshot at frame 200. Verify display output.
4. Strip all debug traces from bus.rs, memory.rs, mod.rs, decode.rs.
5. Commit: "emu-m68k: decode.rs rewrite complete, N/317500 single-step tests pass"

## Rules

1. **Never write more than one phase without testing.** If tests fail, fix before moving on.
2. **No debug trace in committed code.** Add traces locally when debugging, remove before commit.
3. **Each phase gets its own commit.** Makes bisecting easy.
4. **If a test file has >0 failures, investigate until 0.** Don't move on with "close enough".
5. **Extension word flow is the #1 risk.** For every instruction that uses ext words, trace through the ext_idx advancement manually.
