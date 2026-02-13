# cpu-m68k — 68000 CPU Core from Scratch

## Context

The `emu-m68k` crate has fundamental engine-level bugs that can't be fixed incrementally:

1. **Broken extension word pipeline.** `recipe_commit()` resets `ext_count` and `ext_idx`, destroying pre-loaded extension words from `setup_prefetch`. `FetchExtWords` then re-reads from memory at the wrong position. The extension word lifecycle is unfixable without modelling the 68000 prefetch pipeline (IR + IRC) explicitly.
2. **Wasted ticks.** `queue_fetch()` followed by `return` burns a tick doing nothing. `Internal(0)` takes 1 tick instead of 0. These are symptoms of a tick loop that wasn't tested incrementally.
3. **Dual execution paths.** A "legacy" path and a "recipe" path exist side-by-side. The recipe layer adds indirection without solving the prefetch problem.

The fix is a clean rewrite (`cpu-m68k`) that follows the Z80 crate's proven architecture — per-cycle ticking, explicit micro-op queue, instant Execute — adapted for the 68000's 4-cycle bus and 2-word prefetch pipeline. The Z80 passes 100% of 1,604,000 single-step tests using this pattern.

Naming: `cpu-m68k` not `emu-m68k` — this is a CPU core, not a full emulator.

---

## Architecture

### Prefetch pipeline: IR + IRC

The 68000 has two prefetch registers:

- **IR** (Instruction Register): the opcode being executed
- **IRC** (Instruction Register Cache): the next prefetched word

At any point, the CPU has already fetched two words ahead. When an instruction consumes IRC (as an extension word or as the next opcode), a 4-cycle bus fetch replaces it from memory at PC.

**Key operations:**
- **Consume IRC**: read IRC value (instant), queue `FetchIRC` (4 cycles) to refill from PC
- **Start next instruction**: IR <- IRC (instant), queue `FetchIRC` (4 cycles) to refill IRC, then `Execute`

### Tick model

One `tick()` = one CPU clock cycle (7.09 MHz on PAL Amiga). Each tick:

1. Burn wait cycles from bus contention (if any)
2. Process all leading instant ops (Execute, Internal(0))
3. If queue is empty, call `start_next_instruction()` and loop back to step 2
4. Process one cycle of the current timed op (FetchIRC, ReadWord, etc.)
5. If the timed op completed (final cycle), process trailing instant ops

This ensures:
- Instant ops (Execute, StartNextInstr) never waste a tick
- Only one bus cycle per tick
- The trailing-instant-ops pattern lets Execute run within the final cycle of a bus operation (matching the Z80's proven approach)

### MicroOp enum

```rust
enum MicroOp {
    // Prefetch (4 cycles)
    FetchIRC,           // Read word at PC -> IRC, PC += 2

    // Data reads (4 cycles each)
    ReadByte,           // Read byte from self.addr
    ReadWord,           // Read word from self.addr -> self.data
    ReadLongHi,         // Read word from self.addr -> self.data (high)
    ReadLongLo,         // Read word from self.addr+2 -> self.data (low)

    // Data writes (4 cycles each)
    WriteByte,          // Write byte from self.data to self.addr
    WriteWord,          // Write word from self.data to self.addr
    WriteLongHi,        // Write high word of self.data to self.addr
    WriteLongLo,        // Write low word of self.data to self.addr+2

    // Stack operations (4 cycles each)
    PushWord,           // SP -= 2, write word
    PushLongHi,         // SP -= 4, write high word
    PushLongLo,         // Write low word to SP+2
    PopWord,            // Read from SP, SP += 2
    PopLongHi,          // Read high word from SP
    PopLongLo,          // Read low word from SP+2, SP += 4

    // Internal processing
    Internal(u8),       // n cycles (0 = instant)

    // Instant operations (0 cycles)
    Execute,            // Decode IR and execute instruction
}
```

No `CalcEA` micro-op — EA calculation is instant, done inside `Execute`.
No `RecipeStep` — no recipe layer at all.
No `StartNextInstr` — handled by `tick()` when queue empties.

### Instruction lifecycle example

**MOVE.W d16(A0), D1** (12 cycles = 3 bus reads x 4):

```
Tick 1:  Execute(0) — decode MOVE, displacement = IRC, calc EA.
         Queue: [FetchIRC, ReadWord, Execute]
         FetchIRC cycle 0 starts.
Tick 2:  FetchIRC cycle 1
Tick 3:  FetchIRC cycle 2
Tick 4:  FetchIRC cycle 3 — IRC refilled.
Tick 5:  ReadWord cycle 0
Tick 6:  ReadWord cycle 1
Tick 7:  ReadWord cycle 2
Tick 8:  ReadWord cycle 3 — data read.
         Execute(0) — write to D1, set flags. Queue empty.
Tick 9:  start_next_instruction: IR <- IRC.
         Queue: [FetchIRC, Execute]
         FetchIRC cycle 0 starts.
Tick 10: FetchIRC cycle 1
Tick 11: FetchIRC cycle 2
Tick 12: FetchIRC cycle 3 — IRC refilled.
         Execute(0) — decode next opcode.
```

### Multi-stage decode (followup pattern)

Instructions needing multiple extension words use staged decode, like the Z80's `in_followup`:

```
MOVE.W d16(A0), d16(A1) — needs 2 extension words:

Execute (stage 1): src_disp = IRC. Push [FetchIRC, Execute(stage 2)].
FetchIRC (4): IRC refilled with dst displacement.
Execute (stage 2): dst_disp = IRC. Push [FetchIRC, ReadWord, WriteWord].
FetchIRC (4): IRC refilled.
ReadWord (4): read src data.
WriteWord (4): write to dst.
[Queue empty -> start_next_instruction -> FetchIRC(4) + Execute]
Total: 20 cycles (5 bus accesses x 4)
```

### PC-relative addressing

IRC tracks where it was fetched from (`irc_addr` field). When IRC is consumed for a PC-relative EA (d16(PC), d8(PC,Xn)), the base PC for the displacement is `irc_addr`, not the current runtime PC.

### setup_prefetch (for tests)

```rust
pub fn setup_prefetch(&mut self, opcode: u16, irc: u16) {
    self.ir = opcode;
    self.irc = irc;
    self.irc_addr = self.regs.pc.wrapping_sub(2);
    self.instr_start_pc = self.regs.pc.wrapping_sub(4);
    self.micro_ops.clear();
    self.micro_ops.push(MicroOp::Execute);
    self.cycle = 0;
    self.in_followup = false;
}
```

No `ext_words` array. No `ext_count`/`ext_idx`. Extension words come from IRC at decode time. This eliminates the fundamental bug in emu-m68k.

---

## Crate structure

```
crates/cpu-m68k/
├── Cargo.toml              (depends on emu-core only)
├── src/
│   ├── lib.rs              # Public API: Cpu68000, M68kBus, BusResult, FunctionCode
│   ├── bus.rs              # M68kBus trait, BusResult, FunctionCode (port from emu-m68k)
│   ├── cpu.rs              # Cpu68000 struct, tick(), micro-op dispatch
│   ├── microcode.rs        # MicroOp enum, MicroOpQueue (fixed-size 32)
│   ├── decode.rs           # Instruction decode (staged, from IR + IRC)
│   ├── execute.rs          # Instruction execution (ALU ops, data movement)
│   ├── ea.rs               # Effective address calculation (instant, no micro-ops)
│   ├── exceptions.rs       # Exception handling (group 0/1/2)
│   ├── registers.rs        # Registers struct (port from emu-m68k/common)
│   ├── alu.rs              # Size, add/sub/addx/subx/neg/negx (port from emu-m68k/common)
│   ├── flags.rs            # Flag constants, Status helpers (port from emu-m68k/common)
│   ├── addressing.rs       # AddrMode enum (port from emu-m68k/common)
│   ├── timing.rs           # DIVU/DIVS cycles, BCD add/sub/nbcd (port from emu-m68k)
│   └── shifts.rs           # Shift/rotate operations (port from emu-m68k)
└── tests/
    └── single_step_tests.rs  # Test harness (adapted from emu-m68k)
```

## Code reuse from emu-m68k

These files are ported with minimal changes (remove `Cpu68000` method wrapping, adjust imports):

| Source file | Destination | Changes |
|---|---|---|
| `common/registers.rs` | `registers.rs` | None |
| `common/flags.rs` | `flags.rs` | None |
| `common/addressing.rs` | `addressing.rs` | None |
| `common/alu.rs` | `alu.rs` | None |
| `bus.rs` | `bus.rs` | None |
| `m68000/timing.rs` | `timing.rs` | Change `Cpu68000` method to free functions or keep as methods |
| `m68000/execute_shift.rs` | `shifts.rs` | Adapt to new Cpu68000 struct fields |

**What's NOT reused:**
- `m68000/mod.rs` — tick loop is completely different (IR/IRC pipeline)
- `m68000/microcode.rs` — new MicroOp enum (no CalcEA, no RecipeStep, no compound ops)
- `m68000/decode.rs` — staged decode with IRC consumption, not recipe building
- `m68000/execute.rs` — no legacy path, no env var traces, no recipe dispatch
- `m68000/recipe.rs` — eliminated entirely
- `m68000/ea.rs` — `calc_ea` rewritten to use `consume_irc()` instead of `next_ext_word()`

---

## Implementation phases

Each phase adds instructions, verifies against single-step tests, and only proceeds when 100% pass. Test files are in `test-data/m68000-dl/v1/`.

### Phase 0: Infrastructure ✅

Create the crate and tick engine. No instructions — just the tick loop, MicroOp dispatch, FetchIRC handler, and test harness.

**Files:** All source files created. Port reusable code (registers, flags, addressing, alu, bus, timing, shifts). Write `cpu.rs` with tick loop, `microcode.rs` with MicroOp/queue, test harness.

**Verification:** `cargo build -p cpu-m68k` compiles. Test harness loads test files and runs them (all fail with illegal instruction — that's expected).

### Phase 1: MOVE / MOVEA / MOVEQ / LEA

The most important phase — exercises the full EA system, extension word consumption via IRC, FetchIRC refill, and multi-stage decode for two-EA instructions.

**Instructions:** MOVE.b/w/l, MOVEA.w/l, MOVEQ, LEA

**Test files:** `MOVE.b.json.bin`, `MOVE.w.json.bin`, `MOVE.l.json.bin`, `MOVEA.w.json.bin`, `MOVEA.l.json.bin`, `MOVEQ.json.bin`, `LEA.json.bin`

**Key challenges:**
- All 12 addressing modes for source
- All data-alterable modes + address register for destination
- Two-EA modes need multi-stage decode (src ext words then dst ext words)
- Long-word transfers need ReadLongHi + ReadLongLo / WriteLongHi + WriteLongLo
- MOVEA sign-extends word to long, doesn't set flags

### Phase 2: Arithmetic (ADD/SUB/CMP/ADDQ/SUBQ/ADDA/SUBA/CMPA)

**Instructions:** ADD.b/w/l, SUB.b/w/l, CMP.b/w/l, ADDA.w/l, SUBA.w/l, CMPA.w/l, ADDQ, SUBQ

**Key:** Register-to-register ALU ops need Internal(4) for long size. Reuse `alu::add` and `alu::sub`.

### Phase 3: Logic + immediates (AND/OR/EOR/NOT/ADDI/SUBI/CMPI/ANDI/ORI/EORI)

**Instructions:** AND.b/w/l, OR.b/w/l, EOR.b/w/l, NOT.b/w/l, ADDI, SUBI, CMPI, ANDI, ORI, EORI, ANDI to CCR, ANDI to SR, ORI to CCR, ORI to SR, EORI to CCR, EORI to SR

**Key:** Immediate values consumed from IRC (1 word for byte/word, 2 words for long). Memory destinations use read-modify-write.

### Phase 4: Branches and jumps

**Instructions:** Bcc, BRA, BSR, JMP, JSR, RTS, RTE, RTR, DBcc, Scc, NOP

**Key:** Control flow, stack push/pop for JSR/RTS/RTE. Conditional evaluation via `Status::condition()`.

### Phase 5: Shifts and rotates

**Instructions:** ASL/ASR/LSL/LSR/ROL/ROR/ROXL/ROXR (register and memory variants)

**Key:** Port from `execute_shift.rs`. Memory variants operate on words, shift by 1. Register variants have variable count and Internal(6+2n) or Internal(8+2n) timing.

### Phase 6: Bit operations

**Instructions:** BTST/BCHG/BCLR/BSET (register bit number and immediate bit number variants)

### Phase 7: Misc data movement and control

**Instructions:** MOVEM, EXG, SWAP, EXT, CLR, TAS, LINK, UNLK, PEA, MOVE USP, MOVE from SR, MOVE to SR, MOVE to CCR

**Key:** MOVEM is the most complex — register mask in ext word, multi-register transfer with per-register bus cycles.

### Phase 8: Multiply/divide

**Instructions:** MULU, MULS, DIVU, DIVS

**Key:** Variable timing. Port `divu_cycles` and `divs_cycles` from `timing.rs`.

### Phase 9: BCD arithmetic

**Instructions:** ABCD, SBCD, NBCD

**Key:** Port `bcd_add`, `bcd_sub`, `nbcd` from `timing.rs`. Register-to-register and memory-to-memory (-(An)) variants.

### Phase 10: Multi-precision

**Instructions:** ADDX, SUBX, CMPM

**Key:** Extended arithmetic with -(An)/+(An) addressing. Z flag only cleared, never set.

### Phase 11: Exceptions

**Instructions:** TRAP, TRAPV, CHK, illegal instruction, address error, privilege violation

**Key:** Exception frame building (6 bytes for normal, 14 bytes for group 0). Address error needs fault address, access info word, and instruction register in the frame.

### Phase 12: System instructions

**Instructions:** STOP, RESET, MOVE from/to SR/CCR/USP, ANDI/ORI/EORI to SR/CCR

**Key:** Privilege checking (supervisor-only instructions trap from user mode).

---

## Test harness design

Adapted from `crates/emu-m68k/tests/single_step_tests.rs`:

```rust
fn setup_cpu(cpu: &mut Cpu68000, mem: &mut TestBus, state: &CpuState) {
    mem.load_ram(&state.ram);
    // Set registers
    cpu.regs = /* from state */;
    // Set up prefetch — just IR and IRC, no ext_words array
    cpu.setup_prefetch(state.prefetch[0] as u16, state.prefetch[1] as u16);
}

fn run_test(test: &TestCase) -> Result<(), Vec<String>> {
    let mut cpu = Cpu68000::new();
    let mut mem = TestBus::new();
    setup_cpu(&mut cpu, &mut mem, &test.initial);
    for _ in 0..test.cycles {
        cpu.tick(&mut mem);
    }
    compare_state(&cpu, &mem, &test.final_state, &test.name)
}
```

Key difference from emu-m68k: no `ext_words` array construction. Just `setup_prefetch(opcode, irc)`.

The `compare_state` function checks: D0-D7, A0-A6, USP, SSP, SR, PC, and RAM.

Individual test functions per instruction (like `test_movea_w`) plus `run_all_single_step_tests` (`#[ignore]`).

---

## Downstream: emu-amiga2

After cpu-m68k passes single-step tests, rewire `emu-amiga2` to depend on `cpu-m68k` instead of `emu-m68k`:

- `Cargo.toml`: `cpu-m68k = { path = "../cpu-m68k" }`
- Imports: `use cpu_m68k::{Cpu68000, M68kBus, BusResult, FunctionCode}`
- The `AmigaBus` trait implementation stays the same — `M68kBus` is identical.

---

## Verification

- **Per-phase:** Each instruction group reaches 100% pass on its test files before the next phase starts.
- **Full suite:** `cargo test -p cpu-m68k --test single_step_tests run_all_single_step_tests -- --ignored --nocapture` — target: 317,500 / 317,500.
- **Integration:** After full suite passes, boot KS 1.3 via emu-amiga2 to "insert disk" screen.
