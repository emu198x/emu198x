# CONSTRAINTS

**Read this before writing any emulation code. Read it again when you think "I'll simplify this for now."**

---

## There Is No "For Now"

"Simplified for now" means "broken forever." The timing model is not an optimisation pass. It's not a cleanup task. It's the foundation.

If you build instruction-level stepping "for now," you will:
1. Pass early tests (ZEXALL, Klaus Dormann)
2. Get games booting
3. Feel like you're making progress
4. Hit a wall when anything timing-sensitive fails
5. Realise the fix requires rewriting everything
6. Not rewrite everything because "it mostly works"
7. Have a broken emulator that "mostly works"

There is no refactoring path from instruction-level to cycle-accurate. It's not a matter of "adding precision later." The entire execution model is different.

**Do it right the first time or accept that it will never be right.**

---

## Forbidden Patterns

### ❌ Instruction-Level CPU Stepping

```rust
// WRONG - DO NOT USE
fn run_frame(&mut self) {
    while !self.frame_complete {
        self.cpu.execute_instruction();  // ← NO
        self.ppu.catch_up(self.cpu.cycles);
    }
}
```

Why it's wrong:
- Memory accesses happen on specific cycles within an instruction
- Interrupts can be sampled mid-instruction
- Video chip steals cycles from CPU during instructions
- "Catch up" logic accumulates timing errors

### ❌ Variable Time Steps

```rust
// WRONG - DO NOT USE
fn tick(&mut self, delta: f64) {  // ← NO
    self.accumulated += delta;
    while self.accumulated >= CYCLE_TIME {
        self.step();
        self.accumulated -= CYCLE_TIME;
    }
}
```

Why it's wrong:
- Floating-point drift
- Phase relationships become undefined
- "Approximately correct" is incorrect

### ❌ CPU As Timing Master

```rust
// WRONG - DO NOT USE
fn tick(&mut self) {
    self.cpu.tick();
    if self.cpu.cycle % 3 == 0 {  // ← NO
        self.ppu.tick();
    }
}
```

Why it's wrong:
- The crystal is the timing master, not the CPU
- Video chip often runs independently and steals from CPU
- This inverts the actual hardware relationship

### ❌ Separate Clocks Per Component

```rust
// WRONG - DO NOT USE
struct Emulator {
    cpu_clock: u64,   // ← NO
    ppu_clock: u64,   // ← NO
    apu_clock: u64,   // ← NO
}
```

Why it's wrong:
- Clocks drift relative to each other
- No single point of truth for "what time is it"
- Phase relationships become impossible to maintain

### ❌ "Good Enough" Timing

```rust
// WRONG - DO NOT USE
// "Close enough for games, we can fix demos later"
const CYCLES_PER_LINE: u32 = 63;  // Actually 63.5 on some lines
```

Why it's wrong:
- There is no "fix demos later"
- The fix requires architectural changes
- Half-cycle errors accumulate into full-cycle errors into visible glitches

---

## Required Pattern

One master clock. Everything derives from it.

```rust
struct Emulator {
    master_clock: u64,  // Crystal ticks. THE source of truth.
}

impl Emulator {
    fn tick(&mut self) {
        self.master_clock += 1;
        
        // Components check if this is their tick
        if self.master_clock % 4 == 0 {
            self.ppu.tick(&mut self.bus);
        }
        if self.master_clock % 12 == 0 {
            self.cpu.tick(&mut self.bus);
        }
    }
}
```

The CPU does NOT "run an instruction." The CPU ticks. On each tick, it does whatever work that cycle requires — which might be "continue the memory access I started 2 ticks ago."

---

## How To Know You're Doing It Wrong

You're doing it wrong if:

- [ ] You have a function called `execute_instruction()` or `run_instruction()`
- [ ] You have multiple clock counters that aren't derived from one master
- [ ] You use floating-point anywhere in timing calculations
- [ ] You have a "catch up" or "sync" function between components
- [ ] You're calculating "cycles per frame" as a single integer
- [ ] Your CPU doesn't know what cycle of an instruction it's on
- [ ] You think "I'll add cycle accuracy later"

---

## "But The Tests Pass"

Yes. They will.

- ZEXALL passes with instruction-level stepping
- Klaus Dormann passes with instruction-level stepping
- Many games boot with instruction-level stepping
- Some games even play correctly with instruction-level stepping

And then:

- Timing-sensitive loaders fail
- Raster effects glitch
- Demos look wrong
- FDS doesn't load
- Copy protection triggers
- Sprite 0 hit timing breaks split-screen games
- Music plays at wrong speed

The tests that exist test instruction correctness, not timing correctness. Passing them proves nothing about your timing model.

---

## The Correct Response To "Simplify For Now"

No.

---

## If You've Already Built It Wrong

Stop. Do not continue. Do not add features. Do not "see how far it gets."

The timing model is in `docs/architecture.md`. Read it. Start again with `master_clock` as the foundation.

Time spent building on a broken foundation is time wasted.
