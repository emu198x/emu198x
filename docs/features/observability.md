# Observability

## Overview

Observability is a core design goal, not an afterthought. Every emulator exposes internal state for education and debugging.

## Principles

1. **State is always queryable.** At any tick, you can inspect any component.
2. **State is coherent.** Because we tick at crystal frequency, there's no "between states" ambiguity.
3. **Queries don't affect emulation.** Reading state never changes state.
4. **History is available.** Trace buffers record what happened.

## Observable Components

### CPU

| Property | Description |
|----------|-------------|
| `pc` | Program counter |
| `sp` | Stack pointer |
| `registers` | All CPU registers |
| `flags` | Status flags |
| `current_instruction` | Opcode being executed |
| `cycle_in_instruction` | Which cycle of multi-cycle op |
| `halted` | CPU halted/waiting |
| `interrupt_pending` | Pending interrupt state |

### Memory

| Property | Description |
|----------|-------------|
| `read(address)` | Read byte |
| `read_range(address, length)` | Read range |
| `bank_state` | Current bank configuration |
| `last_access` | Most recent read/write |

### Video Chip

| Property | Description |
|----------|-------------|
| `raster_line` | Current scanline |
| `raster_cycle` | Cycle within scanline |
| `frame_number` | Frame counter |
| `registers` | All video registers |
| `palette` | Current colour palette |
| `sprite_state` | Per-sprite state |
| `framebuffer` | Current pixel buffer |

### Audio Chip

| Property | Description |
|----------|-------------|
| `channels` | Per-channel state |
| `registers` | All audio registers |
| `output_buffer` | Recent audio samples |
| `waveform` | Current waveform data |

### Timing

| Property | Description |
|----------|-------------|
| `master_clock` | Crystal tick counter |
| `cpu_cycle` | CPU cycle counter |
| `frame_cycle` | Cycle within frame |

## Query Interface

```rust
pub trait Observable {
    /// Get all state as structured data
    fn snapshot(&self) -> StateSnapshot;
    
    /// Query specific property by path
    fn query(&self, path: &str) -> Option<Value>;
    
    /// List available query paths
    fn query_paths(&self) -> Vec<&'static str>;
}
```

### Query Paths

Hierarchical paths for specific values:

```
cpu.pc
cpu.a
cpu.flags.z
memory.0x0400
memory.0xD020
video.raster_line
video.border_colour
video.sprite.0.x
audio.voice.0.frequency
timing.master_clock
```

### Snapshot Format

```rust
pub struct StateSnapshot {
    pub master_clock: u64,
    pub cpu: CpuState,
    pub memory: MemorySnapshot,
    pub video: VideoState,
    pub audio: AudioState,
    pub custom: HashMap<String, Value>,
}
```

## Trace Recording

### Trace Mode

When enabled, emulator records events:

```rust
pub struct TraceEvent {
    pub tick: u64,
    pub event_type: TraceEventType,
    pub data: TraceData,
}

pub enum TraceEventType {
    InstructionStart,
    InstructionEnd,
    MemoryRead,
    MemoryWrite,
    RegisterChange,
    InterruptRaised,
    InterruptServiced,
    RasterLine,
    Custom(String),
}
```

### Trace Buffer

```rust
pub struct TraceBuffer {
    events: VecDeque<TraceEvent>,
    capacity: usize,
}

impl TraceBuffer {
    pub fn record(&mut self, event: TraceEvent);
    pub fn query(&self, filter: TraceFilter) -> Vec<&TraceEvent>;
    pub fn last_n(&self, n: usize) -> &[TraceEvent];
    pub fn since_tick(&self, tick: u64) -> &[TraceEvent];
}
```

### Trace Filter

```rust
pub struct TraceFilter {
    pub event_types: Option<Vec<TraceEventType>>,
    pub address_range: Option<(u16, u16)>,
    pub tick_range: Option<(u64, u64)>,
}
```

## Breakpoints

### Types

| Type | Trigger |
|------|---------|
| Execution | PC reaches address |
| Read | Memory read from address |
| Write | Memory write to address |
| Change | Value at address changes |
| Condition | Custom expression evaluates true |

### Breakpoint Structure

```rust
pub struct Breakpoint {
    pub id: u32,
    pub breakpoint_type: BreakpointType,
    pub address: Option<u16>,
    pub condition: Option<String>,
    pub enabled: bool,
    pub hit_count: u32,
}
```

### Condition Language

Simple expression language for conditional breakpoints:

```
a == 0            // Register equals value
memory[0xD020] == 14   // Memory equals value
x > 100           // Register comparison
pc >= 0xC000 && pc < 0xD000  // Range check
```

## Disassembly

### Interface

```rust
pub trait Disassembler {
    fn disassemble(&self, memory: &[u8], address: u16) -> DisassembledInstruction;
    fn disassemble_range(&self, memory: &[u8], start: u16, count: usize) -> Vec<DisassembledInstruction>;
}

pub struct DisassembledInstruction {
    pub address: u16,
    pub bytes: Vec<u8>,
    pub mnemonic: String,
    pub operand: String,
    pub cycles: u8,
    pub affects_flags: String,
}
```

### Output Format

```
C000  A9 00     LDA #$00      ; 2 cycles, affects NZ
C002  8D 20 D0  STA $D020     ; 4 cycles
C005  60        RTS           ; 6 cycles
```

## Visual Debugging

### Memory View

Display memory as:
- Hex dump with ASCII
- Disassembly
- Bitmap (for screen memory)
- Character set (for character ROM)

### Video State View

- Current raster position on frame
- Sprite positions and states
- Colour palette
- Register values

### Audio State View

- Waveform visualisation
- Per-channel state
- Envelope state
- Filter state

### Timing View

- CPU cycles vs video cycles
- DMA/contention visualisation
- Interrupt timing

## Educational Annotations

### Labelled Memory

Provide symbolic names for well-known addresses:

```rust
pub struct MemoryLabel {
    pub address: u16,
    pub name: &'static str,
    pub description: &'static str,
}

// C64 example
const LABELS: &[MemoryLabel] = &[
    MemoryLabel { address: 0xD020, name: "BORDER", description: "Border colour" },
    MemoryLabel { address: 0xD021, name: "BGCOL0", description: "Background colour 0" },
    // ...
];
```

### Instruction Explanations

When stepping, provide human-readable explanation:

```
LDA #$00
  Load the value 0 into the accumulator (A register).
  After this instruction:
  - A = 0
  - Zero flag (Z) = 1 (because A is zero)
  - Negative flag (N) = 0 (because bit 7 of A is 0)
```

### Hardware Explanations

Explain what happens when accessing hardware registers:

```
STA $D020
  Write to the VIC-II border colour register.
  The border colour will change to the value in A.
  Value 0 = black, 1 = white, 2 = red, ...
```

## Performance Considerations

- Observability is cheap when not used
- Trace recording has overhead; disabled by default
- Breakpoint checking happens per-tick; minimal impact with few breakpoints
- Snapshots allocate; don't snapshot every tick in hot loops
