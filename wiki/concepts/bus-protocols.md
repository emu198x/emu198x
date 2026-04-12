# Bus Protocols

How the CPU communicates with memory and I/O devices. Each system has its own bus architecture, but the emulator follows a common principle: the CPU exposes signals, the machine handles transactions.

## Signal-level approach

The [Z80](../chips/zilog-z80.md) exposes output signals (`addr`, `data`, `mreq`, `iorq`, `rd`, `wr`, `m1`, `rfsh`, `halt`) and accepts input signals (`data_in`, `wait`, `irq`, `nmi`). The machine loop inspects these signals and performs the appropriate bus transaction each half-cycle.

See [No Bus Trait](../decisions/no-bus-trait.md) for why we chose this over a trait-based abstraction.

## Spectrum bus methods

For convenience, the machine's bus handling is expressed as method calls that map to specific T-state positions within M-cycles:

| Method | When | Purpose |
|--------|------|---------|
| `contend(addr)` | T1 of memory M-cycle | Apply contention delay if address is contended |
| `contend_no_mreq(addr)` | Each T-state of internal ops | IR on bus, MREQ not active |
| `contend_io(port, t)` | Each T-state of I/O M-cycle | Per-T-state I/O contention |
| `m1_read(addr)` | T2 of M1 | Opcode fetch (distinct from data read) |
| `refresh(addr)` | T3 of M1 | IR on bus with RFSH, also contended |
| `read(addr)` / `write(addr, val)` | T2 of memory M-cycles | Data transfer |
| `io_read(port)` / `io_write(port, val)` | T3 of I/O M-cycles | Device communication |
| `interrupt_data()` | T7 of IntAck | Read interrupt vector from bus |

## Cross-system patterns

Each system will define its own bus protocol section as it's implemented. Common patterns:
- CPUs expose signals, machines handle transactions
- Contention/wait states are a property of the system, not the CPU
- Bus arbitration (Amiga) will be documented in `systems/amiga/`
