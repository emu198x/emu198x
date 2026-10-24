# emu198x-wdc-65c816

WDC 65C816.

The 16-bit 6502 inside the Super Nintendo's Ricoh 5A22, the Apple IIGS and
the SA-1. It is cycle-stepped rather than instruction-stepped: each `tick` is
one CPU cycle, internal cycles included, and the bus is a set of public pins
the machine services between ticks.

```rust
use emu198x_wdc_65c816::Wdc65c816;

let mut cpu = Wdc65c816::new();
let mut memory = vec![0u8; 1 << 24];
for _ in 0..1_000 {
    if cpu.bus_active() {
        if cpu.rw {
            cpu.data_in = memory[cpu.addr as usize];
        } else {
            memory[cpu.addr as usize] = cpu.data;
        }
    }
    cpu.tick();
}
```

`vda` and `vpa` say whether a cycle is an opcode fetch, an operand fetch, a
data access or an internal operation, which is what a 65C816 system needs to
time its bus. `vpb` and `mlb` mark vector reads and read-modify-write cycles.

The core is the CPU alone. The 5A22's DMA, multiply and divide units and
timers belong to the machine around it.

## Testing

Every cycle of every case in the
[SingleStepTests 65816](https://github.com/SingleStepTests/65816) corpus: the
address, read or write, the data written, VDA, VPA, VPB, E, M, X and MLB, and
the registers and memory afterwards. The corpus is 2.8 GB, so it is fetched
rather than committed; see `tests/single_step_tests.rs`.

## Provenance

The instruction sequences and the ALU are re-expressed from
[ares](https://github.com/ares-emulator/ares)'s `wdc65816` component (ISC
licence). Part of [Emu198x](https://github.com/emu198x/emu198x), a family of
cycle-accurate retro-computing emulator cores; versioned independently of the
suite.

Licensed GPL-2.0-or-later.
