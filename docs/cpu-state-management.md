# CPU State Management: Prevention Strategies

**This document addresses a class of bugs that emerge from state variable misuse in cycle-accurate CPU emulators.**

---

## The Bug Pattern

In a cycle-accurate CPU, instructions execute across multiple `tick()` calls. Temporary state must persist between ticks within an instruction but must NOT bleed into subsequent instructions.

### What Happened

```rust
struct Cpu {
    addr: u16,  // Temporary address register
    // ...
}
```

The `addr` field was used for two distinct purposes:

1. **Temporary storage** during addressing mode calculations (zero page, absolute, indexed)
2. **Interrupt vector signaling** to distinguish NMI ($FFFA) / IRQ ($FFFE) from software BRK

The bug:
- An addressing mode instruction (e.g., `LDA $1234`) set `self.addr = 0x1234`
- The instruction completed but `addr` was not cleared
- A subsequent BRK instruction checked `if self.addr != 0` to decide the vector address
- BRK used the stale address instead of $FFFE
- The CPU jumped to garbage instead of the BRK vector

### Why It Was Hard To Catch

- BRK worked correctly when preceded by instructions that happened to leave `addr == 0`
- Most BRK test cases used minimal preambles
- The bug was instruction-sequence-dependent, not instruction-specific
- Klaus Dormann test passed because its BRK sequences happened to work

---

## Prevention Strategies

### 1. Clear Temporary State on Instruction Completion

The `finish()` function is called at the end of every instruction. Clear all temporary state here:

```rust
fn finish(&mut self) {
    self.state = State::FetchOpcode;
    self.cycle = 0;

    // Clear ALL temporary registers
    self.addr = 0;
    self.data = 0;
    self.pointer = 0;
}
```

**Pros:** Simple, foolproof, catches all stale state bugs.

**Cons:** Slight overhead (negligible in practice).

**Verdict:** Do this. Always. The cost is nothing compared to debugging stale state.

### 2. Clear Temporary State at Instruction Start

Alternative: clear state in cycle 1 of every instruction, before use:

```rust
fn addr_abs<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
    match self.cycle {
        1 => {
            // Clear at start
            self.addr = 0;
            self.data = 0;

            self.addr = u16::from(bus.read(self.regs.pc).data);
            // ...
        }
        // ...
    }
}
```

**Pros:** Makes each instruction self-contained.

**Cons:** Requires discipline in every addressing mode function. Easy to forget.

**Verdict:** Less reliable than `finish()` cleanup. Use as a backup, not primary strategy.

### 3. Separate Variables for Separate Purposes

The root cause was overloading meaning. `addr` meant both "temporary calculation space" and "interrupt vector selector."

**Bad:**
```rust
struct Cpu {
    addr: u16,  // Used for addressing AND interrupt vector selection
}

fn begin_nmi(&mut self) {
    self.addr = 0xFFFA;  // Signals "use NMI vector"
}

fn op_brk(&mut self) {
    let vector = if self.addr != 0 { self.addr } else { 0xFFFE };
    // ...
}
```

**Good:**
```rust
struct Cpu {
    addr: u16,                    // Temporary address for addressing modes
    interrupt_vector: Option<u16>, // Explicit interrupt vector, None = use BRK
}

fn begin_nmi(&mut self) {
    self.interrupt_vector = Some(0xFFFA);
}

fn op_brk(&mut self) {
    let vector = self.interrupt_vector.unwrap_or(0xFFFE);
    // ...
}
```

**Better: Use an enum:**
```rust
enum InterruptSource {
    Software,      // BRK instruction
    Irq,           // IRQ line
    Nmi,           // NMI line
}

struct Cpu {
    addr: u16,
    interrupt_source: InterruptSource,
}

fn begin_nmi(&mut self) {
    self.interrupt_source = InterruptSource::Nmi;
}

fn op_brk(&mut self) {
    let vector = match self.interrupt_source {
        InterruptSource::Software => 0xFFFE,
        InterruptSource::Irq => 0xFFFE,
        InterruptSource::Nmi => 0xFFFA,
    };
    // ...
}
```

**Verdict:** This is the right solution. Use types to make illegal states unrepresentable.

### 4. Debug Assertions for Stale State

Add assertions to catch assumptions about state:

```rust
fn op_brk(&mut self, bus: &mut B) {
    match self.cycle {
        1 => {
            // BRK should start with clean temp registers if it's software BRK
            debug_assert!(
                self.addr == 0 || self.addr == 0xFFFA || self.addr == 0xFFFE,
                "BRK started with unexpected addr: {:#06X}",
                self.addr
            );
            // ...
        }
        // ...
    }
}
```

**Or assert in `finish()`:**
```rust
fn finish(&mut self) {
    // In debug builds, verify state is clean before clearing
    // (catches places that left dirty state)
    debug_assert_eq!(self.addr, 0, "Instruction left stale addr");

    self.state = State::FetchOpcode;
    self.cycle = 0;
    self.addr = 0;
    self.data = 0;
    self.pointer = 0;
}
```

Wait, that doesn't work - we need to clear state, but we want to know if it wasn't already clean. Better:

```rust
#[cfg(debug_assertions)]
fn finish(&mut self) {
    // Log if state was dirty (not an error, just info for debugging)
    if self.addr != 0 {
        log::trace!("finish(): clearing stale addr {:#06X}", self.addr);
    }

    self.state = State::FetchOpcode;
    self.cycle = 0;
    self.addr = 0;
    self.data = 0;
    self.pointer = 0;
}
```

**Verdict:** Useful for development. Log stale state to catch potential issues early.

---

## Test Case Recommendations

### Test BRK After Various Addressing Modes

The bug manifested when BRK followed instructions that set `addr`. Test BRK after every addressing mode:

```rust
#[test]
fn test_brk_after_absolute() {
    // LDA $1234; BRK
    // The LDA sets addr = $1234, BRK must NOT use that
    let mut cpu = Mos6502::new();
    let mut bus = SimpleBus::new();

    setup_brk_vector(&mut bus, 0x0300);
    setup_stack(&mut cpu);

    bus.load(0x0200, &[
        0xAD, 0x34, 0x12,  // LDA $1234
        0x00,              // BRK
        0xEA,              // NOP (padding)
    ]);
    cpu.regs.pc = 0x0200;

    run_to_address(&mut cpu, &mut bus, 0x0300);

    assert_eq!(cpu.pc(), 0x0300, "BRK should jump to $0300, not $1234");
}

#[test]
fn test_brk_after_indexed() {
    // LDA $1000,X; BRK (with X = $FF, causes page cross)
}

#[test]
fn test_brk_after_indirect_indexed() {
    // LDA ($80),Y; BRK
}

#[test]
fn test_brk_after_rmw() {
    // INC $1000; BRK
}
```

### Test BRK in Complex Sequences

Real programs don't execute BRK immediately after reset. Test realistic sequences:

```rust
#[test]
fn test_brk_after_dormann_preamble() {
    // Exact sequence from Dormann test: LDX #$FF; TXS; LDA #$00; PHA; PLP; BRK
}

#[test]
fn test_brk_in_loop() {
    // loop: LDA table,X; BEQ done; DEX; BNE loop; done: BRK
}

#[test]
fn test_brk_after_jsr_rts() {
    // JSR sub; BRK; sub: LDA $1234; RTS
}
```

### Test Interrupt Interleaving

```rust
#[test]
fn test_nmi_during_absolute_addressing() {
    // LDA $1234 with NMI triggered mid-instruction
    // addr should be used for LDA, then NMI vector, not confused
}

#[test]
fn test_irq_then_brk() {
    // Trigger IRQ, handle it, return, execute BRK
    // Verify vectors are correct for each
}
```

### Property-Based Testing

If using proptest or quickcheck:

```rust
proptest! {
    #[test]
    fn brk_vector_is_correct_regardless_of_preamble(
        preamble in any_valid_instruction_sequence()
    ) {
        let mut cpu = Mos6502::new();
        let mut bus = SimpleBus::new();

        setup_brk_vector(&mut bus, 0x0300);
        setup_stack(&mut cpu);

        // Load preamble followed by BRK
        let program = [preamble.as_slice(), &[0x00, 0xEA]].concat();
        bus.load(0x0200, &program);
        cpu.regs.pc = 0x0200;

        run_until_brk_vector(&mut cpu, &mut bus);

        assert_eq!(cpu.pc(), 0x0300, "BRK vector corrupted after preamble");
    }
}
```

---

## Prevention Checklist

Before merging CPU changes, verify:

- [ ] **`finish()` clears all temporary state** (`addr`, `data`, `pointer`, etc.)
- [ ] **Separate variables for separate purposes** (no overloaded meanings)
- [ ] **State semantics are documented** (what each field is for, when it's valid)
- [ ] **BRK tested after multiple addressing modes** (absolute, indexed, indirect)
- [ ] **BRK tested after complex sequences** (Dormann-like preambles)
- [ ] **Interrupts tested in combination** (NMI then BRK, IRQ then BRK)
- [ ] **Debug assertions verify state invariants** (optional but recommended)

---

## Code Patterns to Avoid

### Overloaded Sentinel Values

```rust
// BAD: 0 means "default" but also means "result of calculation"
let vector = if self.addr != 0 { self.addr } else { DEFAULT_VECTOR };
```

The number 0 is a valid calculation result. Using it as a sentinel causes ambiguity.

### Implicit State Communication

```rust
// BAD: begin_irq() sets up state that op_brk() reads
fn begin_irq(&mut self) {
    self.opcode = 0x00;  // "Pretend it's BRK"
    self.addr = 0xFFFE;  // "But use this vector"
    // ...
}
```

This creates an invisible contract between `begin_irq()` and `op_brk()`. Document it or eliminate it.

### Assuming Clean Initial State

```rust
// BAD: Assumes addr is 0 when instruction starts
fn op_foo(&mut self) {
    if self.cycle == 1 {
        self.addr |= bus.read(self.regs.pc).data as u16;  // Oops, OR with stale value
    }
}
```

Either clear at instruction start or clear in `finish()`.

### Mixing Instruction and Inter-Instruction State

```rust
// BAD: Same struct, no clear boundary
struct Cpu {
    // Architectural state (persists)
    pc: u16,
    a: u8,

    // Instruction-local state (should be cleared)
    addr: u16,
    data: u8,

    // Inter-instruction state (persists)
    nmi_pending: bool,
}
```

Consider grouping:

```rust
struct Cpu {
    regs: Registers,        // Architectural state
    temp: TempState,        // Cleared each instruction
    pending: PendingEvents, // Persists across instructions
}

struct TempState {
    addr: u16,
    data: u8,
    pointer: u8,
}

impl TempState {
    fn clear(&mut self) {
        *self = Self::default();
    }
}
```

---

## Summary

1. **Clear temp state in `finish()`** - simplest and most reliable
2. **Use separate variables for separate purposes** - prevents semantic confusion
3. **Test BRK after every addressing mode** - catches stale state bugs
4. **Add debug assertions** - catches issues during development
5. **Document state semantics** - makes contracts explicit

The 6502 has relatively simple state. The 68000 will have more. Build good habits now.
