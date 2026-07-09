# Emu198x Architecture

> Archived document. Do not treat status claims here as current. Current state lives in `../../status/` and binding rules/decisions.


## 1. Core ambition

Build a cycle-accurate multi-system emulator covering every 8-bit and 16-bit platform, including all model variants, regional variants, period hardware extensions, accelerator cards, and modern recreations (Spectrum Next, MEGA65, Commander X16), with:

- accuracy to the master oscillator of each system, not merely CPU-clock accuracy
- configuration-driven machine construction where variants are parameters, not separate codebases
- a composable extension system for period add-ons and modern enhancements
- a canonical media subsystem with signal-level tape, flux-level disk, sector-level optical, and lightweight ROM pathways
- a debugger and observability layer that serves interactive debugging, scripted MCP queries, automated regression testing, and real-time audio/video visualisation from the same infrastructure
- system-specific debug views (tile banks, sprite viewers, register decoders) producing renderer-agnostic output that any shell can display
- a display pipeline with correct pixel aspect ratio, system-specific signal processing, and GPU-accelerated CRT simulation
- first-class asset export: tiles, sprites, palettes, audio channels, and register logs individually extractable
- a capture pipeline producing screenshots, video, GIF, and audio at any display pipeline stage
- input mapping (symbolic/positional keyboard, gamepad, mouse, paddle, light gun), peripheral emulation (printers, serial/parallel), and networking (Econet, modem/telnet, Ethernet)
- an integrated development environment with per-CPU assemblers, shared symbol tables, source-level debugging, and BASIC text file loading for every BASIC-equipped system
- a multi-window panel UI with OS-native window chrome, where every tool panel is an independent, freely positionable window
- transport features, persistence, and tooling that make the emulator useful for preservation, education, and development

The central architectural principles:

**Do not build file-format loaders.** Build a media subsystem with canonical internal models, transport devices, machine-facing interfaces, and a persistence layer.

**Do not build a debugger as an afterthought.** Build an observation layer into the machine model from the start, with zero-cost hooks when no observer is attached.

**Do not tick at CPU frequency.** Tick at the master oscillator. Let component interleaving emerge from integer clock division, not from batching heuristics.

**Views interpret, shells render.** System-specific view models produce renderer-agnostic output. Shells never interpret hardware state. Adding a system never touches shell code. Improving the shell benefits every system.

**Every observable thing is exportable.** If you can see it in a debug view, you can export it as a file. If you can hear it in a channel, you can capture it as audio.

**Variants are configurations, not codebases.** A Spectrum 128K is a 48K with extensions pre-attached. An accelerated Amiga is a stock Amiga with a different CPU config and fast RAM. The machine builds itself from a config; the code doesn't fork.

**Panels are native windows.** Every tool and debug panel is an independent OS window with native chrome. The user arranges them freely across monitors. The emulator doesn't fight the OS window manager — it uses it.

**Every implementation cites its source.** Hardware behaviour traces back to datasheets, die analyses, or empirical hardware tests. Reference material is cached on disk, catalogued in a manifest, and cited in code comments. Accuracy is verifiable, not anecdotal.

---

## 2. Clock architecture

### 2.1 Why master oscillator, not CPU clock

Crystal frequencies commonly cited for systems (3.5MHz Spectrum, 1.79MHz NES) are derived clocks, not the actual oscillator. The real timing hierarchy has a master oscillator from which all component clocks derive via integer division.

| System | True master | CPU divisor | PPU/ULA divisor | Notes |
|--------|-------------|-------------|-----------------|-------|
| Spectrum 48K PAL | 14.000 MHz | ÷4 (3.5 MHz) | ÷2 (7 MHz) | ULA contention is at 7MHz |
| Spectrum 128K PAL | 17.734475 MHz | ÷5 (3.547 MHz) | ÷... | Different crystal from 48K |
| NES NTSC | 21.477272 MHz | ÷12 (1.790 MHz) | ÷4 (5.369 MHz) | CPU/PPU alignment matters for NMI |
| NES PAL | 26.601712 MHz | ÷16 (1.663 MHz) | ÷5 (5.320 MHz) | Different ratios from NTSC |
| C64 PAL | 17.734475 MHz | ÷18 (0.985 MHz) | ÷8 (dot clock) | VIC-II badlines at dot clock rate |
| Amiga PAL | 28.37516 MHz | ÷4 (7.094 MHz) | ÷... | Chipset DMA at colour clock ÷8 |
| Mega Drive | 53.693175 MHz | ÷7 (7.671 MHz 68K) | ÷... | Z80 also present at different divisor |
| Game Boy | 4.194304 MHz | ÷4 (1.049 MHz) | same crystal | PPU and CPU share master |

Ticking at CPU frequency and batching PPU/APU cycles loses the sub-CPU-cycle interleaving that determines race conditions (NES NMI suppression), clock-stealing (C64 badlines, Amiga DMA), and contention (Spectrum ULA).

### 2.2 Clock tree model

```rust
pub struct ClockFrequency {
    pub numerator: u64,    // Hz (e.g., 21_477_272)
    pub denominator: u64,  // usually 1
}

pub struct ClockDivisor {
    pub divisor: u32,      // ticks every N master cycles
    pub phase: u32,        // offset within divisor period (0..divisor-1)
}

pub struct ClockTree {
    pub master: ClockFrequency,
    pub components: Vec<(&'static str, ClockDivisor)>,
}
```

Phase offset captures the fixed alignment between components. For NES, randomising CPU phase at power-on reproduces the real hardware's non-deterministic NMI race behaviour.

The clock tree supports runtime reconfiguration for systems with selectable CPU speeds (Spectrum Next: 3.5/7/14/28 MHz, MEGA65: 1/40 MHz, Amiga accelerators). When a component's divisor changes at runtime, the scheduler reschedules that component's pending events. The rest of the system continues at its original rate — changing the CPU speed doesn't affect the video or audio clock.

### 2.3 Half-cycle awareness

Two-phase CPUs (Z80, 68000) perform different operations on rising vs falling clock edges. The Z80 samples WAIT on the falling edge of T2, samples data on the falling edge of T3, and samples BUSREQ on the rising edge of the last T-state. The 68000's bus cycle is driven by two non-overlapping phases.

Model this with a 2x divisor — the Z80 ticks at half-T-state rate, receiving a clock edge indicator:

```rust
pub enum ClockEdge { Rising, Falling }

impl Z80 {
    pub fn tick(&mut self, edge: ClockEdge, bus: &mut dyn Z80Bus) {
        match (self.current_t_state, edge) {
            (1, ClockEdge::Falling) => {
                // Assert MREQ, drive address bus
            }
            (2, ClockEdge::Falling) => {
                // Sample WAIT line
            }
            (3, ClockEdge::Falling) => {
                // Sample data bus
            }
            // ...
        }
    }
}
```

### 2.4 Nanosecond boundary

Internal timing is always master cycles. Nanoseconds are used at the boundary where media, audio output, UI, and cross-system concerns live.

```rust
impl ClockFrequency {
    pub fn cycles_to_ns(&self, cycles: u64) -> u64 {
        ((cycles as u128 * 1_000_000_000u128 * self.denominator as u128)
            / self.numerator as u128) as u64
    }

    pub fn ns_to_cycles(&self, ns: u64) -> u64 {
        ((ns as u128 * self.numerator as u128)
            / (1_000_000_000u128 * self.denominator as u128)) as u64
    }
}
```

---

## 3. Scheduling and performance

### 3.1 Next-event scheduler

Instead of ticking every component every master cycle, maintain a priority queue of next-needed-tick per component. Jump directly to the soonest event.

```rust
pub struct Scheduler {
    /// Priority queue keyed by master cycle.
    events: BinaryHeap<Reverse<ScheduledEvent>>,
    /// Current master cycle.
    pub cycle: u64,
}

pub struct ScheduledEvent {
    pub cycle: u64,
    pub component: ComponentId,
}

impl Scheduler {
    pub fn run_until(&mut self, target: u64, machine: &mut Machine) {
        while let Some(event) = self.events.peek() {
            if event.0.cycle > target { break; }
            let event = self.events.pop().unwrap().0;
            self.cycle = event.cycle;
            machine.dispatch(event.component, self);
        }
        self.cycle = target;
    }
}
```

The scheduler remains available for systems that benefit from event-driven scheduling (e.g., systems with multiple asynchronous processors). However, the standard run loop for single-CPU systems uses a **per-cycle loop with a CPU debt model**:

```rust
for _ in 0..cycles_per_frame {
    video.tick();      // 1 cycle: render pixels
    audio.tick();      // 1 cycle: accumulate sample
    tape.advance(ns);  // advance tape transport

    if cpu_debt > 0 {
        cpu_debt -= 1;
    } else {
        let t = cpu.step(&mut bus);
        cpu_debt = (t as i32) - 1;
    }
}
```

This pattern is used consistently across all three current systems (Spectrum, C64, Dragon). Each per-cycle iteration ticks all components by one cycle. The CPU executes whole instructions; its `debt` counter prevents the next instruction from starting until the current one's cycle cost has elapsed. For systems with bus stealing (C64 VIC-II BA line), the CPU execution is additionally gated by the video chip's bus-available signal.

The `else` between the debt decrement and the CPU execution is critical — without it, the CPU fires on the same iteration that the debt reaches zero, running 33% too fast.

### 3.2 Performance strategy (in priority order)

**1. Next-event scheduler** — eliminates empty ticks. The biggest single win.

**2. Function-pointer opcode dispatch** — replaces match/switch dispatch with a 256-entry (or 512 for prefixed opcodes) function pointer table. Indirect call is well-predicted by modern branch predictors for typical instruction working sets.

```rust
type OpcodeHandler = fn(&mut Z80, &mut dyn Z80Bus);
const OPCODE_TABLE: [OpcodeHandler; 256] = [ /* ... */ ];
```

**3. Page-table memory dispatch** — the single biggest interpreter cost is often the address-range match in `bus.read()`. Replace with a page table:

```rust
pub struct MemoryMap {
    read_pages: [PageEntry; 256],   // 256 pages × 256 bytes = 64KB
    write_pages: [PageEntry; 256],
}

enum PageEntry {
    Direct(*const [u8; 256]),  // fast: single pointer deref
    Dispatch,                   // slow: full I/O handler
}
```

ROM and uncontested RAM become direct pointer dereferences. I/O still goes through full dispatch. Typically 2-3x speedup on memory-heavy workloads.

**4. Catch-up batching for slow components** — APU, serial ports, and other components that don't interact with the CPU every cycle can lag behind and catch up when queried. Keep CPU-PPU tightly coupled.

**5. JIT (deferred)** — not appropriate for emu198x's current targets. Cycle-accurate Z80/6502/68000 with contention, clock-stealing, and sub-instruction bus timing leaves effective JIT block sizes of 3-5 instructions — too small for compilation to pay for itself. JIT becomes relevant at PS1/N64 scale. The scheduler and memory map infrastructure built now is exactly what a JIT would plug into later.

---

## 4. Debugger and observability

### 4.1 Design philosophy

The debugger is not a separate application bolted onto the emulator. It is an observation layer woven into the machine model, with four consumers:

1. **Interactive debugger** — breakpoints, stepping, register/memory inspection
2. **MCP server** — scripted queries from external tools and Claude Code
3. **CI regression harness** — automated trace comparison against baselines
4. **Real-time visualisers** — per-channel audio waveforms, PPU state overlays, bus activity displays

All four consume the same infrastructure. Designing for one designs for all.

### 4.2 Zero-cost observation hooks

Observation must be zero or near-zero cost when no observer is attached. The model: every observable operation checks a flag or calls through a trait object that defaults to a no-op.

```rust
/// Trait for observing machine bus activity.
/// Default implementation is no-ops — zero cost when not debugging.
pub trait BusObserver {
    fn on_read(&mut self, _addr: u16, _value: u8, _cycle: u64) {}
    fn on_write(&mut self, _addr: u16, _value: u8, _cycle: u64) {}
    fn on_io_read(&mut self, _port: u16, _value: u8, _cycle: u64) {}
    fn on_io_write(&mut self, _port: u16, _value: u8, _cycle: u64) {}
    fn on_interrupt(&mut self, _kind: InterruptKind, _cycle: u64) {}
    fn on_dma(&mut self, _channel: u8, _addr: u16, _cycle: u64) {}
}

/// Fast check — avoid virtual dispatch when no observer is present.
pub struct ObservationState {
    /// Bitflags for which observation categories are active.
    active: u32,
    observer: Option<Box<dyn BusObserver>>,
}

impl ObservationState {
    #[inline(always)]
    pub fn is_active(&self) -> bool {
        self.active != 0
    }
}
```

In the hot path:

```rust
fn bus_read(&mut self, addr: u16) -> u8 {
    let value = self.memory_map.read(addr);
    if self.observation.is_active() {
        if let Some(obs) = &mut self.observation.observer {
            obs.on_read(addr, value, self.scheduler.cycle);
        }
    }
    value
}
```

The `is_active()` check is a single branch that's almost always not-taken when debugging is off. The branch predictor handles this with essentially zero overhead.

### 4.3 Observation categories

Active observation is controlled by category bitflags so you can enable exactly what you need:

```rust
bitflags! {
    pub struct ObservationFlags: u32 {
        const BUS_READS       = 0b0000_0001;
        const BUS_WRITES      = 0b0000_0010;
        const IO_ACCESS       = 0b0000_0100;
        const INTERRUPTS      = 0b0000_1000;
        const DMA             = 0b0001_0000;
        const CPU_TRACE       = 0b0010_0000;
        const BREAKPOINTS     = 0b0100_0000;
        const SIGNAL_TRACE    = 0b1000_0000;
        const AUDIO_CHANNELS  = 0b0001_0000_0000;
        const VIDEO_STATE     = 0b0010_0000_0000;
        const MEDIA_ACTIVITY  = 0b0100_0000_0000;
    }
}
```

Enabling `BREAKPOINTS` alone adds only the breakpoint check to the hot path. Enabling `CPU_TRACE` adds full instruction logging. Enabling everything gives you a complete system trace at significant performance cost — but that's expected and chosen.

### 4.4 CPU tracer

Records every instruction execution with full context:

```rust
pub struct CpuTraceEntry {
    /// Master cycle at instruction start.
    pub cycle: u64,
    /// Program counter at instruction start.
    pub pc: u16,
    /// Raw opcode bytes (up to 4 for multi-byte instructions).
    pub opcode_bytes: [u8; 4],
    pub opcode_len: u8,
    /// Full register state before execution.
    pub registers: CpuRegisters,
    /// Disassembled instruction text (optional, can be derived).
    pub disasm: Option<String>,
}
```

The trace can be:
- **Ring buffer** — keeps last N entries, constant memory, good for interactive debugging ("what happened in the last 1000 instructions before this crash?")
- **Streaming** — writes to disk/pipe continuously, used for regression baselines and post-hoc analysis
- **Filtered** — only records entries matching a condition (address range, register value, etc.)

```rust
pub enum TraceMode {
    RingBuffer { capacity: usize },
    Stream { sink: Box<dyn TraceSink> },
    Filtered { filter: Box<dyn TraceFilter>, inner: Box<TraceMode> },
}
```

### 4.5 Breakpoint engine

Breakpoints are evaluated in the observation layer, not in the CPU core. The CPU core doesn't know breakpoints exist — it just has the `BusObserver` hook.

```rust
pub enum Breakpoint {
    /// Break when PC reaches this address.
    Execution {
        address: u16,
        condition: Option<BreakCondition>,
    },
    /// Break when this memory address is read.
    MemoryRead {
        address: u16,
        condition: Option<BreakCondition>,
    },
    /// Break when this memory address is written.
    MemoryWrite {
        address: u16,
        value_match: Option<u8>,
        condition: Option<BreakCondition>,
    },
    /// Break when this I/O port is accessed.
    IoAccess {
        port: u16,
        direction: IoDirection,
        condition: Option<BreakCondition>,
    },
    /// Break when an interrupt fires.
    Interrupt {
        kind: Option<InterruptKind>,
    },
    /// Break when master cycle count reaches this value.
    CycleCount {
        cycle: u64,
    },
    /// Break on scanline/dot position.
    VideoPosition {
        scanline: u16,
        dot: Option<u16>,
    },
}

pub enum BreakCondition {
    /// Register comparison.
    Register { reg: RegisterId, op: CompareOp, value: u16 },
    /// Memory value comparison.
    Memory { addr: u16, op: CompareOp, value: u8 },
    /// Hit count — break only after N hits.
    HitCount { target: u32, current: u32 },
    /// Compound conditions.
    And(Vec<BreakCondition>),
    Or(Vec<BreakCondition>),
}
```

For performance, execution breakpoints use a fast-check set:

```rust
pub struct BreakpointSet {
    /// Bloom filter or hash set for fast "is there a breakpoint
    /// anywhere near this address?" check.
    execution_addresses: HashSet<u16>,
    /// Full breakpoint list for condition evaluation.
    breakpoints: Vec<(BreakpointId, Breakpoint)>,
}
```

The hot path checks the hash set (one lookup per instruction when breakpoints are enabled). Full condition evaluation only happens on hash set hits.

### 4.6 Stepping modes

```rust
pub enum StepMode {
    /// Execute one CPU instruction.
    Instruction,
    /// Execute one master clock tick.
    MasterCycle,
    /// Execute until the current subroutine returns (step out).
    StepOut { return_sp: u16 },
    /// Execute until PC leaves the current function/block.
    StepOver,
    /// Execute one scanline.
    Scanline,
    /// Execute one frame.
    Frame,
    /// Run until next breakpoint or manual pause.
    Continue,
}
```

`MasterCycle` stepping is uniquely powerful for debugging timing issues — you can watch the exact interleaving of CPU, PPU, and other components tick by tick.

### 4.7 Component inspectors

Every major component exposes a queryable state snapshot. These are not observation hooks — they're synchronous queries that return the current state.

```rust
/// CPU state inspection.
pub trait CpuInspector {
    fn registers(&self) -> CpuRegisters;
    fn flags(&self) -> CpuFlags;
    fn halted(&self) -> bool;
    fn interrupt_state(&self) -> InterruptState;
    fn current_instruction(&self) -> DisassembledInstruction;
    /// Disassemble N instructions starting at the given address.
    fn disassemble(&self, start: u16, count: usize) -> Vec<DisassembledInstruction>;
}

/// Memory inspection.
pub trait MemoryInspector {
    fn read_byte(&self, addr: u16) -> u8;
    fn read_range(&self, start: u16, len: u16) -> Vec<u8>;
    /// Read without triggering I/O side effects.
    /// Uses the same memory map but bypasses I/O handlers.
    fn peek(&self, addr: u16) -> u8;
    /// Memory map information — what's mapped where.
    fn memory_map_info(&self) -> Vec<MemoryRegionInfo>;
}

/// Video/PPU inspection.
pub trait VideoInspector {
    fn current_scanline(&self) -> u16;
    fn current_dot(&self) -> u16;
    fn registers(&self) -> VideoRegisters;
    fn palette(&self) -> Vec<Color>;
    fn tile_data(&self, index: usize) -> Vec<u8>;
    fn sprite_data(&self) -> Vec<SpriteInfo>;
    fn nametable_data(&self) -> Vec<u8>;
    fn framebuffer(&self) -> &[u8];  // current rendered frame
}

/// Audio/APU inspection.
pub trait AudioInspector {
    fn channel_count(&self) -> usize;
    fn channel_info(&self, channel: usize) -> AudioChannelInfo;
    fn channel_waveform(&self, channel: usize, samples: usize) -> Vec<f32>;
    fn master_volume(&self) -> f32;
    fn registers(&self) -> AudioRegisters;
}

pub struct AudioChannelInfo {
    pub name: String,           // "Pulse 1", "Triangle", "SID Voice 1"
    pub enabled: bool,
    pub frequency: f32,         // Hz
    pub volume: f32,            // 0.0..1.0
    pub waveform_type: String,  // "Pulse", "Triangle", "Noise", etc.
    pub duty_cycle: Option<f32>,
    pub envelope_state: Option<EnvelopeInfo>,
    pub muted_by_user: bool,
}

/// Media device inspection.
pub trait MediaInspector {
    fn tape_state(&self) -> Option<TapeInspection>;
    fn disk_state(&self) -> Option<DiskInspection>;
    fn optical_state(&self) -> Option<OpticalInspection>;
}

pub struct TapeInspection {
    pub position_ns: u64,
    pub duration_ns: u64,
    pub playing: bool,
    pub motor_on: bool,
    pub current_signal: bool,
    pub next_edge_ns: Option<u64>,
    pub counter_display: i32,
    /// Waveform data around current position for visualisation.
    pub waveform_window: Vec<f32>,
}
```

### 4.8 Signal tracer

Captures the state of hardware signal lines at master-clock resolution. Essential for debugging timing-sensitive interactions.

```rust
pub struct SignalTraceEntry {
    pub cycle: u64,
    pub signals: SignalSnapshot,
}

bitflags! {
    pub struct SignalSnapshot: u64 {
        // Z80 signals
        const MREQ    = 0b0000_0001;
        const IORQ    = 0b0000_0010;
        const RD      = 0b0000_0100;
        const WR      = 0b0000_1000;
        const WAIT     = 0b0001_0000;
        const INT      = 0b0010_0000;
        const NMI      = 0b0100_0000;
        const BUSREQ   = 0b1000_0000;
        const BUSACK   = 0b0001_0000_0000;
        const HALT     = 0b0010_0000_0000;
        const M1       = 0b0100_0000_0000;
        const RFSH     = 0b1000_0000_0000;
        // System-specific signals
        const ULA_CONTENTION = 0b0001_0000_0000_0000;
        const EAR      = 0b0010_0000_0000_0000;
        const MIC      = 0b0100_0000_0000_0000;
        // ...extensible per system
    }
}
```

The signal trace is stored as a ring buffer of `(cycle, changed_signals)` — only recording transitions, not every-cycle snapshots. This makes it compact enough to keep a useful window even at master-clock rate.

### 4.9 Structured event log

Higher-level events with semantic meaning, timestamped in master cycles:

```rust
pub enum EmulatorEvent {
    // CPU events
    InstructionExecuted { pc: u16, opcode: u8, cycle: u64 },
    InterruptTaken { kind: InterruptKind, vector: u16, cycle: u64 },
    HaltEntered { cycle: u64 },
    HaltExited { reason: HaltExitReason, cycle: u64 },

    // Memory/bus events
    BankSwitch { slot: u8, bank: u8, cycle: u64 },
    DmaTransfer { src: u16, dst: u16, len: u16, cycle: u64 },

    // Video events
    FrameStart { frame_number: u64, cycle: u64 },
    ScanlineStart { scanline: u16, cycle: u64 },
    VBlankStart { cycle: u64 },
    SpriteOverflow { scanline: u16, cycle: u64 },

    // Audio events
    ChannelStateChange { channel: u8, param: String, value: f32, cycle: u64 },

    // Media events
    TapeEdge { position_ns: u64, new_level: bool, cycle: u64 },
    TapeEarSampled { position_ns: u64, value: bool, cycle: u64 },
    DiskSectorRead { track: u8, sector: u8, cycle: u64 },
    DiskSeek { from_track: u8, to_track: u8, cycle: u64 },
    OpticalSeek { from_lba: u32, to_lba: u32, cycle: u64 },

    // Transport events
    MediaInserted { media_type: String, identity: String, cycle: u64 },
    MediaEjected { media_type: String, cycle: u64 },
    SaveStateCreated { slot: u8, cycle: u64 },
    SaveStateLoaded { slot: u8, cycle: u64 },
}
```

The event log feeds the MCP server, the regression harness, and the UI event timeline.

### 4.10 MCP integration

The MCP server exposes the observation layer as queryable tools. This is the same infrastructure that the interactive debugger uses, just with a different consumer.

```rust
/// MCP-callable queries. Each maps to an MCP tool.
pub trait McpDebugInterface {
    // State queries
    fn cpu_state(&self) -> CpuRegisters;
    fn memory_read(&self, addr: u16, len: u16) -> Vec<u8>;
    fn memory_peek(&self, addr: u16, len: u16) -> Vec<u8>;  // no side effects
    fn disassemble(&self, addr: u16, count: usize) -> Vec<DisassembledInstruction>;
    fn video_state(&self) -> VideoRegisters;
    fn audio_state(&self) -> Vec<AudioChannelInfo>;
    fn media_state(&self) -> MediaInspection;

    // Execution control
    fn step(&mut self, mode: StepMode);
    fn run(&mut self);
    fn pause(&mut self);
    fn reset(&mut self);

    // Breakpoints
    fn add_breakpoint(&mut self, bp: Breakpoint) -> BreakpointId;
    fn remove_breakpoint(&mut self, id: BreakpointId);
    fn list_breakpoints(&self) -> Vec<(BreakpointId, Breakpoint)>;

    // Trace control
    fn set_trace_mode(&mut self, mode: TraceMode);
    fn get_trace_entries(&self, last_n: usize) -> Vec<CpuTraceEntry>;
    fn get_event_log(&self, last_n: usize) -> Vec<EmulatorEvent>;
    fn get_signal_trace(&self, last_n: usize) -> Vec<SignalTraceEntry>;

    // Media control
    fn tape_play(&mut self);
    fn tape_stop(&mut self);
    fn tape_seek(&mut self, position_ns: u64);
    fn load_media(&mut self, path: &str) -> Result<MediaIdentification, MediaError>;
    fn eject_media(&mut self, device: DeviceId);

    // Capture
    fn capture_framebuffer(&self) -> Vec<u8>;
    fn capture_audio(&self, duration_ms: u32) -> Vec<f32>;
    fn capture_channel_audio(&self, channel: usize, duration_ms: u32) -> Vec<f32>;
    fn capture_screenshot(&self, path: &str);

    // Speed control
    fn set_speed(&mut self, speed: SpeedMode);
    fn get_speed(&self) -> SpeedMode;
    fn set_turbo(&mut self, enabled: bool);

    // Machine configuration
    fn capabilities(&self) -> MachineCapabilities;
    fn current_config(&self) -> MachineConfig;
    fn available_variants(&self) -> Vec<VariantDescriptor>;
    fn available_extensions(&self) -> Vec<ExtensionDescriptor>;
    fn attach_extension(&mut self, ext: ExtensionId) -> Result<(), ExtensionError>;
    fn detach_extension(&mut self, ext: ExtensionId) -> Result<(), ExtensionError>;
    fn set_cpu_speed(&mut self, hz: u64) -> Result<(), ClockError>;
}
```

This is the same interface the CI regression harness uses:

```
MCP tool: step(mode=Frame)
MCP tool: capture_framebuffer()
MCP tool: compare with baseline → pass/fail
```

And the same interface Claude Code uses during development:

```
"Run for 100 frames, show me the last 20 instructions, what's in memory at $4000-$4010?"
```

### 4.11 Audio channel visualisation

Per-channel audio is a stated signature feature of emu198x. The observability layer provides it:

```rust
/// Per-channel audio capture for visualisation.
pub struct ChannelAudioCapture {
    /// Ring buffers of per-channel samples, pre-mixed.
    channels: Vec<ChannelBuffer>,
    /// Master mix for comparison.
    master: ChannelBuffer,
}

pub struct ChannelBuffer {
    pub name: String,
    pub samples: VecDeque<f32>,
    pub sample_rate: u32,
    /// Current visualisation-friendly metrics.
    pub rms_level: f32,
    pub peak_level: f32,
    pub frequency_estimate: f32,
}
```

The APU inspector fills these buffers as a side effect of normal audio generation — not as an observation hook. The cost is one extra sample write per channel per audio sample, which is negligible. The visualiser reads from the ring buffers at display refresh rate.

This means per-channel audio works whether or not debugging is enabled. It's always-on, always cheap.

### 4.12 Trace export formats

For regression baselines and cross-tool analysis:

```rust
pub enum TraceExportFormat {
    /// Compact binary format for regression comparison.
    /// Fixed-width records, fast to compare.
    Binary,
    /// Human-readable text, one line per entry.
    /// Compatible with diff tools.
    Text,
    /// JSON for external tool consumption.
    Json,
    /// FUSE-compatible trace format (Spectrum-specific).
    FuseTrace,
}
```

The binary format is the regression baseline. Text format is for human debugging. JSON is for MCP and external tools. FUSE trace format enables comparison against the established Spectrum emulator test suite.

### 4.13 Regression harness integration

The observation layer, combined with the MCP server and TOSEC media collection, forms an automated regression harness:

```
For each ROM/tape/disk in corpus:
  1. Load media (MCP: load_media)
  2. Run for N frames (MCP: step(Frame) × N)
  3. Capture framebuffer hash + audio hash (MCP: capture_*)
  4. Compare against stored baseline
  5. If new: store as baseline
  6. If changed: report regression with diff
  7. Optionally: capture trace for failing tests
```

The trace is only captured on failure to keep storage manageable. The framebuffer hash and audio hash are cheap to compute and compare.

This is the same infrastructure as the existing TOSEC ROM verification design (perceptual hashing, framebuffer entropy analysis) but extended to run continuously in CI.

---

## 5. Debug views

### 5.1 The missing layer

The inspector traits (§4.7) provide raw data — `tile_data()` returns bytes, `sprite_data()` returns structs, `palette()` returns colours. But between "here are 8KB of CHR ROM bytes" and "here's a visual tile bank the user can browse" there's an interpretation layer that's system-specific.

A Spectrum doesn't have tiles. The NES has 8×8 tiles in CHR ROM/RAM with a 4-colour palette per tile. The SNES has 8×8 and 16×16 tiles with up to 256 colours. The Amiga has bitplanes, not tiles. The C64 has character ROM/RAM with colour attributes. "Show me the sprite banks" means completely different things per system.

### 5.2 View model layer

Between the inspector traits and the shell rendering, a system-specific view model layer interprets raw hardware state into visual debugging views. It produces renderer-agnostic output that any shell can consume.

```rust
/// System-specific debug view generation.
/// Lives in machine-*-views crates.
pub trait DebugViews {
    fn available_views(&self) -> Vec<DebugViewDescriptor>;
    fn render_view(
        &self,
        view: DebugViewId,
        inspector: &dyn MachineInspector,
    ) -> DebugViewOutput;
}

pub struct DebugViewDescriptor {
    pub id: DebugViewId,
    pub name: String,
    pub category: ViewCategory,
    pub size_hint: (u32, u32),
}

pub enum ViewCategory {
    VideoMemory,    // tile banks, nametables, pattern tables
    Sprites,        // OAM, sprite list, sprite preview
    Palettes,       // palette viewer/editor
    Registers,      // chip register state
    Memory,         // raw memory views
    Audio,          // channel state, waveforms
    Timing,         // scanline, dot position, clock state
    Media,          // tape waveform, disk state
}
```

### 5.3 View output types

```rust
pub enum DebugViewOutput {
    /// Pixel buffer — tile banks, nametables, sprite previews.
    PixelBuffer {
        width: u32,
        height: u32,
        pixels: Vec<u8>,  // RGBA
        overlays: Vec<ViewOverlay>,
        hit_regions: Vec<HitRegion>,
    },

    /// Structured data — register views, palette swatches, OAM tables.
    Structured {
        sections: Vec<ViewSection>,
    },

    /// Waveform data — audio channels, tape signal.
    Waveform {
        channels: Vec<WaveformChannel>,
        time_range_ns: (u64, u64),
    },

    /// Timeline data — DMA slots, contention, events.
    Timeline {
        tracks: Vec<TimelineTrack>,
        range: (u64, u64),  // master cycles
    },
}
```

### 5.4 Structured data rows

```rust
pub struct ViewSection {
    pub title: String,
    pub rows: Vec<ViewRow>,
}

pub enum ViewRow {
    /// Labelled value: "PC: $C000"
    LabelValue {
        label: String,
        value: String,
        highlight: Option<HighlightColour>,
    },
    /// Flag register: individual bit display
    Flags {
        label: String,
        flags: Vec<(String, bool)>,
    },
    /// Colour swatch
    ColourSwatch {
        label: String,
        colours: Vec<(u8, u8, u8)>,
    },
    /// Hex dump row
    HexRow {
        address: u16,
        bytes: Vec<u8>,
        ascii: String,
        highlights: Vec<(usize, HighlightColour)>,
    },
    Separator,
}
```

### 5.5 Hit regions for interactivity

The `HitRegion` system makes pixel-buffer views interactive without the shell needing system-specific knowledge.

```rust
pub struct HitRegion {
    pub x: u32, pub y: u32,
    pub width: u32, pub height: u32,
    pub target: HitTarget,
}

pub enum HitTarget {
    Tile { bank: u8, index: u16 },
    Sprite { oam_index: u8 },
    NametableCell { table: u8, x: u8, y: u8 },
    PaletteEntry { palette: u8, index: u8 },
    MemoryAddress { addr: u16 },
    AudioChannel { channel: u8 },
    CustomRegion { id: String, metadata: String },
}
```

When the user clicks on a tile in the pattern table view, the shell finds `HitTarget::Tile { bank: 0, index: 42 }` and can highlight that tile in the nametable, show OAM references, cross-reference to the CHR address, or open a memory view at the right offset. The shell handles interaction; the view model handles interpretation.

### 5.6 System-specific view catalogues

**NES:**

| View | What it shows |
|------|---------------|
| Pattern Table 0 | 256 tiles from CHR $0000-$0FFF with selectable palette |
| Pattern Table 1 | 256 tiles from CHR $1000-$1FFF |
| Nametable Viewer | All four nametables as pixel maps, mirroring visualised |
| OAM Viewer | All 64 sprites — index, tile, position, flags, rendered preview |
| Palette Viewer | Background + sprite palettes (4×4 each), colour swatches |
| PPU Registers | $2000-$2007 decoded with flag names |
| Scroll State | X/Y scroll, fine scroll, toggle state |
| Mapper State | Current bank mappings, IRQ counter, variant-specific state |

**Spectrum:**

| View | What it shows |
|------|---------------|
| Screen Memory | Pixel buffer + attribute overlay showing ink/paper/bright/flash per cell |
| Attribute Map | 32×24 grid with attribute bytes colour-coded |
| Border State | Current border colour, recent border changes timeline |
| ULA State | Contention state, floating bus value, frame position |
| Memory Banks | 128K bank mapping, active pages per slot |
| I/O Ports | Last values written to key ports |

**C64:**

| View | What it shows |
|------|---------------|
| Character ROM/RAM | Character set rendered as tile grid |
| Screen RAM | 40×25 character grid with colour RAM overlay |
| VIC-II Sprites | All 8 sprites — data, position, multicolour, expand, priority |
| VIC-II State | Raster position, mode, bank, scroll, badline status |
| SID State | Per-voice frequency, waveform, ADSR, filter, ring mod |
| CIA State | Timer A/B, port state, interrupt mask/status |

**Amiga:**

| View | What it shows |
|------|---------------|
| Bitplane Viewer | Each bitplane individually + composite |
| Copper List | Decoded copper instructions with current position |
| Sprite Viewer | Hardware sprites with position, data, priorities |
| Blitter State | Source/dest/mask, shift, operation, size, active/idle |
| DMA Timeline | Per-scanline slot allocation (bitplane, sprite, copper, blitter, disk, audio) |
| Palette | All 32 colours (64 in EHB mode) |
| Custom Registers | Decoded custom chip register file |

### 5.7 Memory editor

The memory editor is both a view and a control. `MemoryInspector::peek()` (read without side effects) is critical — a memory editor that triggers I/O reads on scroll would be useless.

```rust
pub struct MemoryEditorState {
    pub base_address: u16,
    pub row_width: u16,
    pub address_space_size: u32,
    pub highlights: Vec<MemoryHighlight>,
    pub region_labels: Vec<MemoryRegionLabel>,
}

pub struct MemoryHighlight {
    pub start: u16,
    pub len: u16,
    pub colour: HighlightColour,
    pub label: String,  // "SP", "PC", "breakpoint"
}
```

Reads via `peek()`. Writes via `poke()` (debug write, no side effects) or through the real bus for "what happens if I write this" testing.

### 5.8 Shell rendering contract

The shell's job is straightforward — it receives `DebugViewOutput` variants and renders them:

```rust
fn render_debug_panel(
    &mut self,
    views: &dyn DebugViews,
    inspector: &dyn MachineInspector,
) {
    for view_id in &self.open_views {
        let output = views.render_view(*view_id, inspector);
        match output {
            DebugViewOutput::PixelBuffer { .. } => self.render_texture(..),
            DebugViewOutput::Structured { .. } => self.render_property_grid(..),
            DebugViewOutput::Waveform { .. } => self.render_waveform(..),
            DebugViewOutput::Timeline { .. } => self.render_timeline(..),
        }
    }
}
```

Adding a new system's debug views never touches shell code. Shell rendering improvements benefit every system's views.

---

## 6. Display pipeline and speed control

### 6.1 The pipeline

The raw framebuffer from the emulated video hardware goes through a processing chain before reaching the screen. Each stage is optional and configurable.

```
Machine framebuffer (native resolution, integer pixels)
  → Pixel aspect ratio correction
  → Signal processing (optional: composite/RF/S-Video decode)
  → Phosphor blur (beam profile simulation)
  → Scanlines, phosphor bloom, mask, curvature
  → Output scaling (to window/display resolution)
  → Shell display
```

This pipeline also feeds the capture system — screenshots and video can be captured at any stage.

### 6.2 Pixel aspect ratio

Real CRT displays didn't show square pixels. The pixel aspect ratio (PAR) is the width:height ratio of a single emulated pixel as it appeared on a real display.

| System | Resolution | Display | PAR | Effective DAR |
|--------|-----------|---------|-----|---------------|
| Spectrum 48K | 256×192 | 4:3 CRT | ~1.18:1 | 4:3 |
| NES NTSC | 256×240 | 4:3 CRT | 8:7 (~1.14:1) | ~4:3 |
| NES PAL | 256×240 | 4:3 CRT | ~1.39:1 | ~4:3 |
| C64 PAL | 320×200 | 4:3 CRT | ~0.94:1 | ~4:3 |
| C64 multicolour | 160×200 | 4:3 CRT | ~1.87:1 | ~4:3 |
| Amiga lores PAL | 320×256 | 4:3 CRT | ~0.94:1 | ~4:3 |
| Amiga hires PAL | 640×256 | 4:3 CRT | ~0.47:1 | ~4:3 |
| Mega Drive H40 | 320×224 | 4:3 CRT | ~1.00:1 | ~4:3 |
| SNES | 256×224 | 4:3 CRT | 8:7 (~1.14:1) | ~4:3 |
| Game Boy | 160×144 | LCD (square) | 1:1 | 10:9 |
| Master System | 256×192 | 4:3 CRT | ~1.18:1 | 4:3 |

PAR is a system-defined property, not baked into the renderer:

```rust
pub struct DisplayGeometry {
    /// Native framebuffer dimensions.
    pub native_width: u32,
    pub native_height: u32,
    /// Pixel aspect ratio (width / height of one pixel on the real display).
    /// 1.0 = square pixels.
    pub par: f64,
    /// Visible area within the framebuffer (excluding overscan borders).
    pub visible_area: Rect,
    /// Whether the system used interlaced output.
    pub interlaced: bool,
    /// Nominal refresh rate.
    pub refresh_rate: f64,
    /// Signal type the system natively output.
    pub signal_type: SignalType,
}

pub enum SignalType {
    Composite,   // NES, C64, most consoles via standard video out
    SVideo,      // C64 (with S-Video cable), some consoles
    RGB,         // Amiga, Spectrum (via SCART), consoles via SCART
    RF,          // most home computers/consoles via TV antenna input
    Digital,     // Game Boy LCD, later handhelds
}
```

PAR correction is applied by rendering the framebuffer to a correctly proportioned quad. The user can toggle between PAR-corrected and raw square pixels.

### 6.3 Signal processing (system-specific)

Some systems relied on the analogue signal path for visual effects. These effects are system-specific because they depend on how the hardware generates the video signal.

**NES NTSC composite artefacting** — the NES encodes colour by selecting a signal phase for each pixel. When decoded by a composite video decoder, adjacent pixels bleed into each other, producing colours that don't exist in the NES palette. Games were designed for this — Blaster Master's waterfall, Castlevania's sky gradients, and many games use NTSC artefacting for additional colours. This is not a filter; it's a faithful decode of the signal the hardware actually produces.

**C64 composite/luma bleed** — the VIC-II's chroma output bleeds between adjacent pixels, producing distinctive colour fringing. Multicolour mode games relied on this bleed for smoother colour transitions than the raw pixel data suggests.

**CPC / Spectrum** — typically connected via RGB SCART, producing clean output. RF connection adds significant colour bleed and noise, but this is rarely desired.

```rust
pub trait SignalProcessor: Send + Sync {
    fn process(
        &self,
        input: &Framebuffer,
        output: &mut Framebuffer,
        mode: SignalMode,
    );
}

pub enum SignalMode {
    /// Raw RGB — no signal processing. Still blurred by phosphor stage.
    Direct,
    /// Composite video decode — system-specific artefacting.
    Composite,
    /// S-Video decode — luma/chroma separated, less artefacting.
    SVideo,
    /// RF decode — worst quality, most artefacting and noise.
    RF,
}
```

### 6.4 Phosphor blur

Real CRTs were significantly blurrier than modern emulator displays suggest. The electron beam has a Gaussian profile — it doesn't illuminate one phosphor dot and stop. The spot size is larger than one pixel pitch. Adjacent pixels bleed into each other. Even on a high-end RGB monitor like a Sony PVM, a 256-pixel-wide image was a continuous luminance waveform with 256 peaks blending into each other, not 256 crisp rectangles.

**"Pixel perfect" with hard square pixels is a display mode that never existed on any real hardware.** It is useful for development and pixel art analysis, but it is the least authentic viewing option.

The sharpness hierarchy on real hardware was: RF (very blurry, noisy) → composite (blurry, colour bleed) → S-Video (less colour bleed, still soft) → RGB SCART (cleanest, still analogue-soft) → PVM/BVM (sharpest available, still not pixel-sharp).

```rust
pub struct PhosphorBlur {
    /// Horizontal blur radius in native pixels.
    /// Typical: 0.3-0.6 for PVM, 0.5-1.0 for RGB SCART,
    /// 1.0-2.0 for S-Video, 2.0-3.0 for composite, 3.0-5.0 for RF.
    pub horizontal_radius: f32,

    /// Vertical blur radius in native pixels.
    /// Typically less than horizontal because scanlines constrain vertical spread.
    /// Typical: 0.2-0.5 for PVM, 0.3-0.8 for consumer CRT.
    pub vertical_radius: f32,

    /// Blur profile shape.
    pub profile: BlurProfile,
}

pub enum BlurProfile {
    /// Gaussian — most physically accurate for electron beam spot.
    Gaussian,
    /// Lanczos — sharper falloff than Gaussian, good for "PVM look".
    Lanczos,
}
```

Phosphor blur is applied in the CRT shader as a separable 2-pass convolution — very cheap on GPU. It sits between signal processing and scanline/mask rendering.

### 6.5 CRT simulation (system-agnostic)

After signal processing and phosphor blur, generic CRT simulation is applied. This is system-agnostic.

```rust
pub struct CrtParameters {
    /// Phosphor blur.
    pub blur: PhosphorBlur,

    /// Scanline rendering.
    pub scanline_weight: f32,       // 0.0 = no scanlines, 1.0 = full black between lines
    pub scanline_shape: ScanlineShape,

    /// Phosphor bloom (bright areas glow beyond their boundary).
    pub phosphor_bloom: f32,        // 0.0 = none, 1.0 = heavy
    /// Temporal phosphor persistence between frames.
    pub phosphor_persistence: f32,

    /// Shadow mask / aperture grille.
    pub mask_type: MaskType,
    pub mask_intensity: f32,        // 0.0 = none, 1.0 = fully visible

    /// Geometry.
    pub curvature: f32,             // 0.0 = flat, 1.0 = heavy barrel distortion
    pub corner_rounding: f32,
    pub vignetting: f32,            // 0.0 = none, 1.0 = heavy edge darkening

    /// Colour.
    pub colour_temperature: f32,    // Kelvin, typical CRT ~6500K
    pub saturation: f32,            // 1.0 = normal
    pub brightness: f32,
    pub contrast: f32,
    pub gamma: f32,                 // typical CRT ~2.2-2.5
}

pub enum ScanlineShape {
    Sharp,     // hard black line between scanlines
    Gaussian,  // soft Gaussian falloff
    Sinc,      // more accurate phosphor profile
}

pub enum MaskType {
    None,
    ShadowMask,      // traditional TV dot triad
    ApertureGrille,  // Trinitron vertical stripes
    SlotMask,        // rectangular aperture pattern
}
```

### 6.6 CRT implementation approach

The CRT filter runs as a GPU shader. CPU-side CRT simulation is too expensive for real-time at high resolutions.

For egui (native shell): custom wgpu render pipeline with CRT fragment shader. The framebuffer texture is rendered to a quad with the CRT shader applied.

For WASM shell: WebGL2 shader, same algorithm adapted for GLSL ES.

The shader needs the framebuffer at native resolution as input (not pre-scaled), plus the CRT parameters as uniforms. PAR correction is applied as part of the output quad geometry, not by resampling the framebuffer.

### 6.7 Presets

The default should be an authentic CRT preset, not pixel-perfect. Hard square pixels are useful for development but are the least authentic viewing option.

| Preset | Default? | Blur | Signal | Scanlines | Mask | Curvature | PAR |
|--------|----------|------|--------|-----------|------|-----------|-----|
| **Development** | No | None | None | None | None | None | Off |
| **Clean RGB** | **Yes** | Light | Direct | Light | None | None | On |
| **PVM/BVM** | No | Light | Direct | Medium | Aperture grille | None | On |
| **Consumer RGB (SCART)** | No | Medium | Direct | Medium | Shadow mask | Light | On |
| **S-Video** | No | Medium | S-Video | Medium | Shadow mask | Light | On |
| **Composite** | No | Heavy | Composite | Medium | Shadow mask | Medium | On |
| **RF Memories** | No | Heavy | RF | Heavy | Shadow mask | Heavy | On |
| **Handheld LCD** | No | None | None | None | LCD grid | None | Square |

**Clean RGB** is the recommended default: PAR-corrected, lightly blurred (approximating a good RGB monitor), with subtle scanlines. It looks right without being distracting. Users who want raw pixel analysis switch to Development mode. Users who want nostalgia go to Composite or RF.

The preset system is data-driven — presets are serialisable `CrtParameters` + `SignalMode` pairs, stored as config files. Users can create and share custom presets.

### 6.8 Capture points

The display pipeline has defined capture points where the capture system (§8) can tap in:

| Capture point | What you get |
|---------------|-------------|
| `Raw` | Native resolution, raw pixel values, no processing |
| `SignalProcessed` | After composite/RF decode, before CRT |
| `CrtProcessed` | Full CRT simulation applied (blur, scanlines, mask, curvature) |
| `FinalOutput` | As displayed in the window (including window scaling) |

Screenshots and video can be captured at any point. Asset export (§7) always captures at `Raw`.

### 6.9 Speed control

The emulation speed is decoupled from the display refresh rate. The GUI shell is always locked to the host's vsync (typically 60Hz). The emulator runs at a configurable multiple of real speed, and the display layer drops or duplicates emulated frames to match the host refresh rate.

```rust
pub struct SpeedControl {
    pub speed: SpeedMode,
    pub audio_mode: SpeedAudioMode,
    /// Whether turbo was auto-engaged (e.g., during tape loading).
    pub auto_turbo_active: bool,
    pub auto_turbo_config: AutoTurboConfig,
}

pub enum SpeedMode {
    /// Locked to a multiple of real speed, frame-paced to host display.
    Locked(SpeedMultiplier),
    /// Unlocked — run as fast as possible, no frame pacing.
    Turbo,
}

pub enum SpeedMultiplier {
    Quarter,   // 0.25x — slow motion
    Half,      // 0.5x  — half speed
    Normal,    // 1.0x  — real time
    Double,    // 2.0x
    Quad,      // 4.0x
    Octa,      // 8.0x
}

pub enum SpeedAudioMode {
    /// Pitch-shifted — audio plays faster/slower with speed.
    /// Natural for most use cases.
    PitchShifted,
    /// Muted — no audio output. Default for turbo.
    Muted,
    /// Normal pitch with time-stretch (expensive, rarely needed).
    TimeStretched,
}
```

**How speed interacts with the scheduler:** The scheduler runs in master cycles, not wall-clock time. Speed control determines how many master cycles to execute per host frame:

- At 1x: `master_freq / native_refresh_rate` cycles per host frame
- At 2x: double that
- At 0.5x: half that
- Turbo: as many as possible before next vsync

**How speed interacts with audio:** At 2x, `PitchShifted` mode outputs audio at double pitch. At 0.5x, half pitch. For turbo mode, audio is muted by default — playing audio at 800% speed is just noise.

**How speed interacts with media:** Tape transport advances in nanoseconds derived from master cycles. At 2x, the tape runs at 2x — the conversion is unchanged, but more master cycles execute per wall-clock second. Tape loading at 8x or turbo is a natural fit.

### 6.10 Auto-turbo

Auto-turbo detects when the machine is in a loading loop and temporarily engages turbo mode.

```rust
pub struct AutoTurboConfig {
    /// Auto-turbo when tape is playing and machine is in loading routine.
    pub tape_loading: bool,
    /// Auto-turbo during floppy disk access.
    pub disk_loading: bool,
    /// Restore previous speed when loading completes.
    pub restore_on_complete: bool,
    /// Cooldown before disengaging (avoids flickering on inter-block gaps).
    pub disengage_cooldown_ms: u32,
}
```

Detection is system-specific via a trait:

```rust
pub trait LoadingDetector {
    fn is_loading(&self, inspector: &dyn MachineInspector) -> bool;
}
```

Spectrum: if the last N instructions were in the ROM tape loading routine ($0556-$0604 for 48K), the machine is loading. C64: if the datasette motor is on. FDS: if the disk drive motor is on and the CPU is in the BIOS disk read loop. Disk systems: if the disk motor is spinning and the CPU is in a known DOS/BIOS wait loop.

The auto-turbo engages when `is_loading()` returns true and disengages after the cooldown expires with `is_loading()` returning false. Audio is muted during auto-turbo.

### 6.11 Headless speed policy

Headless (MCP/CI/batch) operation uses different speed rules because there's no display to sync to:

```rust
pub struct HeadlessCapturePolicy {
    /// Video recording: real speed for correct frame timing.
    pub video_speed: SpeedMode,
    /// Screenshot: turbo to target frame, then capture.
    pub screenshot_speed: SpeedMode,
    /// Audio recording: real speed for correct audio output.
    pub audio_speed: SpeedMode,
    /// Asset extraction: turbo.
    pub asset_extraction_speed: SpeedMode,
    /// Regression testing: turbo.
    pub regression_speed: SpeedMode,
}

impl Default for HeadlessCapturePolicy {
    fn default() -> Self {
        Self {
            video_speed: SpeedMode::Locked(SpeedMultiplier::Normal),
            screenshot_speed: SpeedMode::Turbo,
            audio_speed: SpeedMode::Locked(SpeedMultiplier::Normal),
            asset_extraction_speed: SpeedMode::Turbo,
            regression_speed: SpeedMode::Turbo,
        }
    }
}
```

Video must run at real speed because frame timing matters — dropped or doubled frames produce incorrect video. Audio must run at real speed because sample timing matters. Screenshots only need correct frame content, so turbo to the target frame then capture. Regression and asset extraction are throughput-limited — run as fast as possible.

---

## 7. Asset export

### 15.1 Design intent

The user should be able to grab individual assets directly from the emulated system and export them as clean, usable files. Tiles, sprites, palettes, audio channels, character sets, backgrounds — anything visible in the debug views should be exportable.

This serves preservation, education (CL198x content creation), fan art reference, and music production (chiptune ripping).

### 15.2 Visual asset export

```rust
pub trait AssetExporter {
    /// Export individual assets from the current machine state.
    fn export_tile(
        &self,
        bank: u8,
        index: u16,
        palette: u8,
        inspector: &dyn MachineInspector,
    ) -> ExportedImage;

    fn export_sprite(
        &self,
        oam_index: u8,
        inspector: &dyn MachineInspector,
    ) -> ExportedImage;

    fn export_tile_bank(
        &self,
        bank: u8,
        palette: u8,
        inspector: &dyn MachineInspector,
    ) -> ExportedImage;

    fn export_nametable(
        &self,
        table: u8,
        inspector: &dyn MachineInspector,
    ) -> ExportedImage;

    fn export_full_sprite_sheet(
        &self,
        inspector: &dyn MachineInspector,
    ) -> ExportedImage;

    fn export_palette(
        &self,
        inspector: &dyn MachineInspector,
    ) -> ExportedPalette;

    fn export_background(
        &self,
        inspector: &dyn MachineInspector,
    ) -> ExportedImage;
}

pub struct ExportedImage {
    pub width: u32,
    pub height: u32,
    /// RGBA pixels. Transparent background where no pixel data exists.
    pub pixels: Vec<u8>,
    /// Metadata about the asset.
    pub metadata: AssetMetadata,
}

pub struct AssetMetadata {
    pub system: String,
    pub asset_type: String,       // "tile", "sprite", "nametable", etc.
    pub source_address: Option<u32>,
    pub palette_used: Option<u8>,
    pub native_size: (u32, u32),  // size before any scaling
    pub description: String,
}

pub struct ExportedPalette {
    pub name: String,
    pub colours: Vec<PaletteColour>,
}

pub struct PaletteColour {
    pub r: u8, pub g: u8, pub b: u8,
    pub index: u8,
    pub native_value: u16,  // raw hardware palette value
    pub label: String,      // "$0F", "dark grey", etc.
}
```

Visual assets are always exported at native resolution with transparency. No PAR correction, no CRT filtering — the user gets raw pixel data. Export formats:

| Format | Use case |
|--------|----------|
| PNG | Default — lossless, transparency, widely supported |
| PNG sprite sheet | All tiles/sprites in a single image with metadata sidecar |
| ASE/Aseprite | With palette and frame data for pixel art tools |
| GPL/PAL | Palette-only export for GIMP/Photoshop |
| JSON | Machine-readable metadata (addresses, palette indices, dimensions) |

### 7.3 Audio asset export

Per-channel audio export is a first-class feature, not a debug afterthought.

```rust
pub trait AudioExporter {
    /// Export a single audio channel for a time range.
    fn export_channel(
        &self,
        channel: usize,
        duration_ms: u32,
        format: AudioExportFormat,
    ) -> Vec<u8>;

    /// Export all channels as separate files.
    fn export_all_channels(
        &self,
        duration_ms: u32,
        format: AudioExportFormat,
    ) -> Vec<(String, Vec<u8>)>;

    /// Export the master mix.
    fn export_master(
        &self,
        duration_ms: u32,
        format: AudioExportFormat,
    ) -> Vec<u8>;

    /// Export channel register state log for a time range.
    /// Useful for music analysis and chiptune recreation.
    fn export_register_log(
        &self,
        duration_ms: u32,
    ) -> Vec<AudioRegisterEvent>;
}

pub struct AudioRegisterEvent {
    pub cycle: u64,
    pub channel: u8,
    pub register: String,
    pub value: u16,
}

pub enum AudioExportFormat {
    Wav16,    // 16-bit PCM WAV
    Wav32f,   // 32-bit float WAV
    Flac,     // lossless compression
    Ogg,      // lossy, smaller files
}
```

The register log export is particularly valuable — it captures the exact sequence of register writes that produced the music, which can be replayed in other tools or analysed for CL198x content.

### 7.4 Export via MCP

All export operations are available through the MCP interface, enabling scripted asset extraction:

```
MCP: load_media("zelda.nes")
MCP: step(Frame, count=300)  // advance to title screen
MCP: export_tile_bank(bank=0, palette=0)  // → PNG file
MCP: export_sprite(oam_index=0)  // → PNG file
MCP: export_all_channels(duration_ms=5000)  // → WAV files
MCP: export_register_log(duration_ms=10000)  // → JSON
```

This enables automated asset extraction from large ROM collections — useful for CL198x content pipelines and preservation cataloguing.

### 7.5 Batch asset extraction

For CL198x and preservation work, a batch mode that extracts assets from a running game:

```rust
pub struct BatchExportConfig {
    /// Run for this many frames before capturing.
    pub skip_frames: u32,
    /// Capture window in frames.
    pub capture_frames: u32,
    /// What to export.
    pub targets: Vec<BatchExportTarget>,
    /// Output directory.
    pub output_dir: String,
}

pub enum BatchExportTarget {
    AllTileBanks,
    AllSprites,
    AllPalettes,
    UniqueScreens,  // deduplicated framebuffer captures
    AudioChannels { duration_ms: u32 },
    RegisterLog { duration_ms: u32 },
    Screenshot { capture_point: CapturePoint },
}
```

---

## 8. Capture pipeline

### 8.1 Overview

The capture pipeline produces screenshots, video recordings, GIF animations, and audio recordings from the running emulator. It taps into the display pipeline at defined capture points and the audio system at the channel or master level.

### 8.2 Screenshot capture

```rust
pub struct ScreenshotConfig {
    /// Where in the display pipeline to capture.
    pub capture_point: CapturePoint,
    /// Output format.
    pub format: ImageFormat,
    /// Scale factor (1 = native resolution, 2 = 2x, etc.).
    /// Only applies to Raw and SignalProcessed capture points.
    pub scale: u32,
    /// Whether to apply PAR correction.
    pub par_correction: bool,
}

pub enum CapturePoint {
    /// Native resolution, raw pixel values.
    Raw,
    /// After signal processing (composite/RF decode).
    SignalProcessed,
    /// Full CRT simulation applied.
    CrtProcessed,
    /// As displayed in the window.
    FinalOutput,
}

pub enum ImageFormat {
    Png,
    Bmp,
    Jpg { quality: u8 },
    WebP { quality: u8 },
}
```

Default screenshot: `CapturePoint::Raw` at 1x with PAR correction, PNG format. This gives a clean, correctly-proportioned image at the exact resolution the hardware produced.

### 8.3 Video recording

```rust
pub struct VideoRecordingConfig {
    pub capture_point: CapturePoint,
    pub format: VideoFormat,
    pub par_correction: bool,
    /// Frame rate — None uses the system's native refresh rate.
    pub frame_rate: Option<f64>,
    /// Include audio.
    pub audio: bool,
    /// Maximum duration — None for unlimited (manual stop).
    pub max_duration_seconds: Option<f64>,
}

pub enum VideoFormat {
    /// H.264 in MP4 container. Good balance of quality/size.
    Mp4 { crf: u8 },
    /// VP9 in WebM container. Better compression, slower encode.
    WebM { crf: u8 },
    /// Lossless video for archival/editing.
    Ffv1,
    /// Raw frame sequence — individual PNGs.
    PngSequence,
}
```

Video encoding uses FFmpeg as a subprocess (linked via command-line pipe, not as a library — avoids licensing complexity and keeps the dependency optional). Frames are piped to FFmpeg's stdin as raw RGBA; audio as raw PCM. The emulator doesn't need to know about video codecs.

```
Emulator → raw frames (pipe) → FFmpeg → encoded video file
         → raw audio  (pipe) ↗
```

For systems without FFmpeg installed, `PngSequence` provides a fallback that can be assembled later.

### 8.4 GIF capture

GIF is important for sharing short clips — loading sequences, gameplay moments, glitch documentation.

```rust
pub struct GifConfig {
    pub capture_point: CapturePoint,
    pub par_correction: bool,
    /// Scale — GIFs are typically small. 1x or 2x is common.
    pub scale: u32,
    /// Duration in seconds.
    pub duration_seconds: f64,
    /// Frame rate — GIF supports variable frame timing.
    /// None uses half the system refresh rate (decent quality, smaller file).
    pub frame_rate: Option<f64>,
    /// Maximum colours (GIF is 256-colour per frame).
    pub max_colours: u16,
    /// Dithering for colour reduction.
    pub dither: DitherMode,
}

pub enum DitherMode {
    None,
    Floyd,
    Ordered,
}
```

For retro systems with limited palettes, GIF works surprisingly well — a 16-colour Spectrum or 25-colour NES frame needs no colour reduction at all. The quantisation only matters for CRT-filtered or composite-decoded output.

GIF encoding can use the `gif` Rust crate directly — no external dependency needed.

### 8.5 Audio recording

```rust
pub struct AudioRecordingConfig {
    /// Which audio to capture.
    pub source: AudioCaptureSource,
    pub format: AudioExportFormat,
    /// Duration — None for unlimited.
    pub max_duration_seconds: Option<f64>,
    /// Sample rate — None uses the emulator's internal rate.
    pub sample_rate: Option<u32>,
}

pub enum AudioCaptureSource {
    /// Master mix — what the user hears.
    Master,
    /// Single channel.
    Channel(usize),
    /// All channels as separate tracks in a multi-channel file.
    AllChannelsSeparate,
    /// All channels as separate files.
    AllChannelsFiles,
}
```

### 8.6 MCP capture integration

All capture operations are MCP-callable:

```rust
pub trait McpCaptureInterface {
    fn screenshot(&self, config: ScreenshotConfig, path: &str);
    fn start_video(&mut self, config: VideoRecordingConfig, path: &str);
    fn stop_video(&mut self);
    fn capture_gif(&mut self, config: GifConfig, path: &str);
    fn start_audio_recording(&mut self, config: AudioRecordingConfig, path: &str);
    fn stop_audio_recording(&mut self);

    // Asset export
    fn export_tile(&self, bank: u8, index: u16, palette: u8, path: &str);
    fn export_sprite(&self, oam_index: u8, path: &str);
    fn export_tile_bank(&self, bank: u8, palette: u8, path: &str);
    fn export_all_channels(&self, duration_ms: u32, dir: &str);
    fn export_palette(&self, path: &str);
    fn export_register_log(&self, duration_ms: u32, path: &str);
}
```

This enables scripted content capture for CL198x:

```
MCP: load_media("manic_miner.tzx")
MCP: tape_play()
MCP: step(Frame, count=500)  // wait for game to load
MCP: start_video({ capture_point: Raw, par_correction: true, format: Mp4 { crf: 18 } }, "manic_miner_gameplay.mp4")
MCP: step(Frame, count=300)  // record 5 seconds of gameplay
MCP: stop_video()
MCP: screenshot({ capture_point: CrtProcessed }, "manic_miner_crt.png")
MCP: screenshot({ capture_point: Raw, par_correction: true }, "manic_miner_clean.png")
MCP: export_all_channels(duration_ms=10000, "manic_miner_audio/")
```

### 8.7 Tape loading screech capture

A specific use case worth calling out: capturing the tape loading screech as audio. The tape audio renderer produces audio that can be captured independently of the machine's normal audio output:

```
MCP: load_media("manic_miner.tzx")
MCP: start_audio_recording({ source: TapeAudio, format: Wav16 }, "loading_screech.wav")
MCP: tape_play()
MCP: step(Frame, count=1000)  // let the game load
MCP: stop_audio_recording()
```

This is valuable for CL198x content (demonstrating what loading sounded like) and for preservation (the screech is part of the cultural experience).

---

## 9. UI and window management

### 9.1 Multi-window panel architecture

The emulator UI follows a multi-window panel model similar to professional creative tools. Each panel — main display, disassembler, memory editor, tile viewer, audio mixer, tape deck — is an independent OS-native window that can be freely positioned, resized, moved across monitors, minimised, or closed without affecting other panels.

The emulator core runs in a background thread. Panel windows connect to it and query the data they need through the existing observation layer and inspector traits. Each panel is another consumer of the same infrastructure.

### 9.2 Native window chrome

Each panel is a real OS window with native chrome:

- **macOS**: real `NSWindow` instances. Panels appear separately in Mission Control. They can be grouped via macOS native window tabbing. The app appears in the Dock once with all its windows.
- **Windows**: real `HWND` instances. Panels appear in the taskbar (grouped). They can be snapped to screen edges with Windows Snap.
- **Linux**: real X11/Wayland windows. Window manager handles tiling, workspaces, virtual desktops.

The rendering inside each window uses egui for tool/debug panels and a custom wgpu pipeline for the main display (CRT shader). But the window itself is always OS-native.

Implementation: `winit` for cross-platform window creation. Each panel gets its own `winit::Window` and its own egui/wgpu rendering context.

### 9.3 Panel system

```rust
pub trait PanelWindow: Send {
    fn descriptor(&self) -> PanelDescriptor;
    fn render(&mut self, ctx: &PanelContext);
    fn handle_event(&mut self, event: &PanelEvent);
    /// Does this panel need the emulator paused to update?
    fn requires_pause(&self) -> bool { false }
}

pub struct PanelDescriptor {
    pub id: PanelId,
    pub title: String,
    pub category: PanelCategory,
    pub default_size: (u32, u32),
    pub resizable: bool,
    /// Can multiple instances exist? (e.g., multiple memory editors at different addresses)
    pub multi_instance: bool,
}

pub enum PanelCategory {
    Emulation,      // main display, transport controls
    Debug,          // disassembly, registers, memory, breakpoints
    Video,          // tile viewer, nametable, sprite viewer, palette
    Audio,          // channel waveforms, register state, mixer controls
    Media,          // tape deck, disk browser, media info
    Peripheral,     // printer output, serial monitor, network status
    Ide,            // source editor, assembler output, BASIC editor
    Settings,       // input mapping, display config, variant selector
}
```

### 9.4 Window manager

```rust
pub struct WindowManager {
    /// All open panel windows.
    panels: Vec<OpenPanel>,
    /// Layout persistence — remembers window positions per system.
    layout: WindowLayout,
    /// Available panels for current system (from MachineCapabilities).
    available_panels: Vec<PanelDescriptor>,
}

pub struct OpenPanel {
    pub descriptor: PanelDescriptor,
    pub window: winit::window::Window,
    pub renderer: Box<dyn PanelWindow>,
    pub position: (i32, i32),
    pub size: (u32, u32),
    pub monitor: Option<MonitorId>,
}

pub struct WindowLayout {
    /// Saved panel positions per system variant.
    layouts: HashMap<(SystemId, VariantId), Vec<SavedPanelState>>,
}

pub struct SavedPanelState {
    pub panel_id: PanelId,
    pub position: (i32, i32),
    pub size: (u32, u32),
    pub monitor: Option<MonitorId>,
    pub visible: bool,
    /// Panel-specific state (e.g., memory editor base address, selected palette).
    pub panel_state: Option<String>,
}
```

Window positions and which panels are open persist in the config system (§26). Different systems have different default layouts — the Spectrum defaults to main display + disassembler + memory + tape deck. The NES defaults to main display + pattern tables + nametable + OAM.

### 9.5 Panel catalogue

Every panel listed below is an independent window. The set of available panels adapts based on `MachineCapabilities` — systems without tiles don't offer a tile viewer; systems without AY chips don't show AY channel panels.

**Emulation panels:**

| Panel | Description |
|-------|-------------|
| Main Display | Emulated video output with CRT shader, PAR correction |
| Transport Controls | Play/pause/step/reset, speed control, turbo toggle |
| Media Browser | Mount/eject tape, disk, cartridge, memory card |

**Debug panels:**

| Panel | Description |
|-------|-------------|
| Disassembly | Live disassembly with PC tracking, breakpoint gutters, symbol labels |
| CPU Registers | Live register state with edit-in-place |
| Memory Editor | Hex editor with peek/poke, region labels, search |
| Breakpoints | List, add, remove, enable, disable with conditions |
| Watch Expressions | User-defined expressions evaluated each step |
| Call Stack | SP-based stack trace (heuristic) |
| Trace Log | Recent instruction trace with filters |
| Signal Trace | Hardware signal timeline (MREQ, IORQ, INT, EAR, etc.) |
| Event Log | Structured emulator events timeline |

**Video panels (system-dependent):**

| Panel | Systems | Description |
|-------|---------|-------------|
| Tile/Pattern Viewer | NES, SNES, Mega Drive, Game Boy, CPC, MSX | Tile banks with selectable palette |
| Nametable/Tilemap | NES, SNES, Mega Drive, Spectrum Next | Background map viewer |
| Sprite/OAM Viewer | NES, SNES, Mega Drive, C64, Amiga | All sprites with properties |
| Palette Viewer | All | Colour swatches with native values |
| Screen Memory | Spectrum, C64 | Raw screen + attribute overlay |
| Bitplane Viewer | Amiga | Individual bitplanes + composite |
| Copper/DMA Timeline | Amiga | Per-scanline DMA slot allocation |
| ULA State | Spectrum | Contention, border, floating bus |

**Audio panels:**

| Panel | Description |
|-------|-------------|
| Channel Waveforms | Per-channel oscilloscope display |
| Audio Registers | Chip register state (AY, SID, APU, Paula) |
| Mixer | Per-source and per-channel volume, mute, solo |
| Frequency Spectrum | FFT display of master or per-channel audio |

**Media panels:**

| Panel | Description |
|-------|-------------|
| Tape Deck | Transport controls, counter, waveform, position scrubber |
| Disk Info | Current track/sector, drive activity, disk contents |
| Printer Output | Rendered printer pages, export controls |
| Serial Monitor | Raw byte stream for RS-232/modem debugging |
| Network Status | Connection state, packet counts, Econet station info |

**IDE panels (see §13):**

| Panel | Description |
|-------|-------------|
| Source Editor | Assembly/BASIC editor with syntax highlighting |
| Assembler Output | Build log, errors, warnings |
| Symbol Table | Browse/search assembled symbols |
| Listing | Side-by-side source and generated bytes |
| BASIC Editor | BASIC-specific editor with line numbering |

### 9.6 The main display panel

The main display is special — it hosts the CRT shader pipeline and handles input capture (keyboard, mouse, light gun). It's the only panel that uses a custom wgpu render pipeline rather than egui.

When the main display has focus, keyboard and mouse events are routed to the emulated system through the input manager. When another panel has focus, keyboard events go to that panel's UI (e.g., typing in the memory editor search box).

The main display panel also hosts an overlay for on-screen notifications (speed indicator, rewind indicator, screenshot flash, media insert confirmation).

---

## 10. Audio output pipeline

### 9.1 The problem

Multiple audio sources run at different effective rates and must mix into a single output stream with controlled latency. The Spectrum has a beeper. Add a 128K and there's an AY chip. Add a SpecDrum and there's a DAC. Load a tape and there's tape screech. The C64 has SID output. Add a second SID in an extension. The NES base APU has 5 channels; a VRC6 cartridge adds 3 more; an FDS adds a wavetable channel. The Amiga has 4 Paula channels plus possible CD audio from CD32.

The mixer must handle all of this without knowing system specifics.

### 9.2 Audio source trait

Every component that produces audio implements a common trait:

```rust
pub trait AudioSource: Send + Sync {
    /// Descriptor for this source (name, channel count, type).
    fn descriptor(&self) -> AudioSourceDescriptor;

    /// Fill the buffer with samples at the requested sample rate.
    /// Called by the mixer at the output cadence.
    /// The source converts from its internal rate to the requested rate.
    fn render(&mut self, sample_rate: u32, buffer: &mut [f32]);

    /// Number of output channels (1 = mono, 2 = stereo).
    fn channel_count(&self) -> usize { 1 }

    /// Per-channel descriptors for the visualiser/inspector.
    fn sub_channels(&self) -> Vec<AudioChannelDescriptor>;

    /// Render a single sub-channel in isolation (for per-channel export).
    fn render_sub_channel(
        &mut self,
        channel: usize,
        sample_rate: u32,
        buffer: &mut [f32],
    );
}

pub struct AudioSourceDescriptor {
    pub name: String,           // "AY-3-8912", "SID 6581", "APU", "Tape Audio"
    pub native_rate: u32,       // rate at which the source naturally produces samples
    pub sub_channel_count: usize,
}
```

The Spectrum beeper is an `AudioSource` with 1 sub-channel. The AY chip is an `AudioSource` with 3 sub-channels. The NES APU is an `AudioSource` with 5 sub-channels. A VRC6 mapper is a separate `AudioSource` with 3 sub-channels that the machine registers when the cartridge is loaded.

### 9.3 Mixer

```rust
pub struct AudioMixer {
    /// Registered audio sources.
    sources: Vec<AudioSourceEntry>,
    /// Output sample rate (typically 44100 or 48000).
    output_sample_rate: u32,
    /// Output buffer size in samples (affects latency).
    buffer_size: u32,
    /// Master volume.
    master_volume: f32,
    /// Per-source volume and mute state.
    source_controls: Vec<SourceControl>,
    /// Ring buffer for per-channel capture (feeds visualiser + export).
    channel_capture: ChannelAudioCapture,
}

pub struct AudioSourceEntry {
    pub source: Box<dyn AudioSource>,
    pub id: AudioSourceId,
}

pub struct SourceControl {
    pub id: AudioSourceId,
    pub volume: f32,         // 0.0 .. 1.0
    pub muted: bool,
    pub solo: bool,          // if any source is soloed, only soloed sources play
    /// Per-sub-channel mute (for muting individual AY/SID channels).
    pub sub_channel_mutes: Vec<bool>,
}
```

The mixer's render loop:

1. For each registered source, call `render()` with a scratch buffer
2. Apply per-source volume and mute/solo
3. Sum into the master output buffer
4. Write per-source and per-sub-channel samples into the channel capture ring buffers (for visualisation and export)
5. Apply master volume
6. Push to the audio output backend

### 9.4 Latency management

Audio latency is the delay between the emulated hardware producing a sound and the user hearing it. Too little buffer → crackle and underruns. Too much → input feels laggy.

```rust
pub struct AudioOutputConfig {
    pub sample_rate: u32,       // 44100 or 48000
    pub buffer_size: u32,       // samples per buffer (64-2048)
    pub buffer_count: u32,      // number of buffers in ring (2-4)
    pub backend: AudioBackend,
}

pub enum AudioBackend {
    /// cpal — cross-platform, default
    Cpal,
    /// Platform-specific low-latency (CoreAudio, WASAPI, ALSA)
    PlatformNative,
    /// Null — no output (headless mode)
    Null,
}
```

Typical latency = `buffer_size / sample_rate × buffer_count`. With 512 samples at 48kHz and 2 buffers: ~21ms. Acceptable for most use cases. Games with tight audio-visual sync may benefit from 256 samples (~10ms).

### 9.5 Speed control interaction

At 2x speed, the emulated hardware produces samples at 2x the normal rate. The mixer has two strategies:

**Pitch-shifted (default):** output the samples at 2x rate. The audio plays at double pitch. Simple — just produce twice as many samples per mixer callback. The output sample rate stays the same; the emulator fills the buffer with more emulated time per callback.

**Muted (turbo):** discard all audio. The sources still run (to keep their internal state correct) but the mixer doesn't output anything. Avoids the cost of audio output during high-speed operation.

**Time-stretched (expensive):** resample to maintain original pitch at altered speed. Requires a time-stretch algorithm (WSOLA or similar). Rarely worth the cost for retro audio.

At 0.5x speed, pitch-shifted mode produces half-pitch audio. The mixer fills the buffer with half as much emulated time per callback, and the remaining buffer is silence (or the mixer produces smaller buffers and the audio backend pads).

### 9.6 Source registration and hot-plugging

Sources are registered and deregistered at runtime as extensions are attached/detached or media is loaded/ejected:

```rust
impl AudioMixer {
    pub fn register_source(&mut self, source: Box<dyn AudioSource>) -> AudioSourceId;
    pub fn deregister_source(&mut self, id: AudioSourceId);

    /// Called when an extension with audio is attached.
    /// E.g., NES cartridge with VRC6 expansion audio.
    pub fn on_extension_attached(&mut self, source: Box<dyn AudioSource>);

    /// Called when tape is loaded and tape audio should be available.
    pub fn on_media_audio_available(&mut self, source: Box<dyn AudioSource>);
}
```

The per-channel visualiser and the audio inspector adapt automatically — they query the mixer's current source list and render whatever channels exist.

### 9.7 System-specific mixing concerns

**NES expansion audio** — the real hardware mixes expansion audio on the cartridge connector, before the console's audio output. The mixing ratio between APU and expansion audio is determined by resistor values on the cartridge board, which differ between mappers. The mixer needs per-source gain that's configured by the machine based on the active mapper.

**Amiga** — Paula outputs 4 channels, two left and two right (channels 0+3 left, 1+2 right). The stereo separation is extreme — 100% hard-panned. Many users prefer a "stereo blend" that mixes some left into right and vice versa. This is a per-machine audio post-processing step, configured in the source control.

**C64 SID** — when two SID chips are present (some expansions), they're typically at different addresses and the software writes to them independently. Each is a separate `AudioSource`. The mixer combines them.

**Tape screech** — tape audio is a separate `AudioSource` registered when a tape is loaded. It produces audio from the `TapeTimeline::render_audio()` method. It mixes alongside the machine's normal audio output, which is correct — on real hardware, the loading screech came through the TV speaker while the beeper could also be active.

---

## 11. Input system

### 9.1 Keyboard mapping

Every 8/16-bit system's keyboard is a matrix scanned by hardware or a controller sending scancodes. The input mapper converts host keyboard events into matrix positions or scancodes.

Two mapping modes are needed:

**Symbolic** — when the user types `A`, the emulated system sees whatever key combination produces `A` on its keyboard. On the Spectrum, `!` requires `SYMBOL SHIFT + 1`. On the C64, `{` requires `Shift + Commodore + [`. The mapper knows the emulated keyboard's character map and generates the correct modifier combinations. Best for typing.

**Positional** — `Q` maps to the key in the same physical position. Games use QAOP+Space or specific key positions for control; they don't care what character the key produces. Best for gaming.

```rust
pub struct InputProfile {
    pub system: SystemId,
    pub name: String,
    pub keyboard_mode: KeyboardMode,
    pub keyboard_map: Vec<KeyMapping>,
    pub joystick_maps: Vec<JoystickMapping>,
    pub mouse_config: Option<MouseConfig>,
}

pub enum KeyboardMode {
    /// Map by character produced.
    Symbolic,
    /// Map by physical key position.
    Positional,
    /// Auto-detect: symbolic when text cursor active, positional otherwise.
    Auto,
}
```

### 9.2 Emulated keyboard interface

```rust
pub trait EmulatedKeyboard: Send + Sync {
    fn key_down(&mut self, key: EmulatedKey);
    fn key_up(&mut self, key: EmulatedKey);
    fn matrix_state(&self) -> &KeyMatrix;
    fn layout(&self) -> &KeyboardLayout;
}

pub struct KeyboardLayout {
    pub system: SystemId,
    /// Character → key combination map (for symbolic mapping).
    pub symbol_map: Vec<SymbolEntry>,
    /// Physical position → emulated key (for positional mapping).
    pub position_map: Vec<PositionEntry>,
    /// System-specific keys needing explicit bindings
    /// (SYMBOL SHIFT, Commodore, BREAK, Run/Stop, etc.)
    pub special_keys: Vec<SpecialKeyDef>,
}

pub struct SymbolEntry {
    pub character: char,
    pub keys: Vec<EmulatedKey>,  // may require multiple simultaneous keys
}

pub struct SpecialKeyDef {
    pub name: String,
    pub key: EmulatedKey,
    pub default_binding: Option<HostKey>,
}
```

### 9.3 Joystick and gamepad

| System | Interface | Directions | Buttons |
|--------|-----------|------------|---------|
| Spectrum | Kempston / Sinclair / Cursor | 4-way digital | 1 |
| C64 | DB9 (Atari-style) | 4-way digital | 1 |
| NES | Proprietary serial | D-pad | 4 (A, B, Select, Start) |
| SNES | Proprietary serial | D-pad | 8 (A, B, X, Y, L, R, Select, Start) |
| Mega Drive | DB9 multiplexed | D-pad | 3 or 6 + Start |
| Amiga | DB9 / CD32 pad | 4-way digital | 1-3 / 7 (CD32) |
| Master System | Proprietary | D-pad | 2 |
| Atari 2600 | DB9 | 4-way + paddle | 1 |

```rust
pub struct JoystickMapping {
    pub port: u8,
    pub source: JoystickSource,
    pub directions: DirectionMapping,
    pub buttons: Vec<ButtonMapping>,
}

pub enum JoystickSource {
    Gamepad { device_id: u32 },
    Keyboard,
    Combined { device_id: u32 },
}

pub enum HostInput {
    Key(HostKey),
    GamepadButton(u32, GamepadButton),
    GamepadAxis(u32, GamepadAxis, AxisDirection),
    MouseButton(MouseButton),
}
```

### 9.4 Analogue inputs

**Mouse** — Amiga, Atari ST, Spectrum (AMX/Kempston mouse). Reports relative movement delta + buttons. Main concerns: sensitivity matching and mouse capture (grab host cursor, hide it).

**Paddle** — Atari 2600, C64. Reports analogue position (0-255) from a potentiometer. Maps from gamepad analogue stick/trigger or mouse X position.

**Trackball** — Atari systems. Relative movement like mouse with different sensitivity. Maps from host mouse.

**Spinner** — arcade systems. Reports rotation. Maps from mouse X or gamepad analogue stick.

```rust
pub enum AnalogueInputType {
    Mouse { sensitivity: f32, capture_mode: MouseCaptureMode },
    Paddle { range: (u8, u8) },
    Trackball { sensitivity: f32 },
    Spinner { sensitivity: f32 },
}

pub enum MouseCaptureMode {
    ClickCapture,    // capture on click, Escape to release
    AlwaysCapture,   // capture when emulator has focus
    NeverCapture,    // use host cursor position relative to window
}
```

### 9.5 Light gun and light pen

These report a screen position. The hardware works by detecting the CRT beam position — when triggered, the hardware records which scanline/dot the beam is illuminating.

For emulation, the host mouse position maps to emulated screen coordinates. The display pipeline's PAR correction and CRT transform must be inverted to convert host mouse position back to emulated video hardware coordinates (scanline + dot). This inverse transform is an `emu-display` concern; the conversion to hardware registers is a `machine-*` concern.

```rust
pub struct LightGunMapping {
    pub source: LightGunSource,
    pub trigger: HostInput,
    pub offscreen: Option<HostInput>,  // for reload gestures
}

pub enum LightGunSource {
    Mouse,       // host mouse, through inverse display transform
    Absolute,    // tablet/touchscreen
}
```

### 9.6 Input manager

```rust
pub struct InputManager {
    pub profile: InputProfile,
    pub host_devices: Vec<HostInputDevice>,
    pub keyboard_mode_override: Option<KeyboardMode>,
}

pub trait InputReceiver {
    fn on_host_key_down(&mut self, key: HostKey, machine: &mut dyn MachineInput);
    fn on_host_key_up(&mut self, key: HostKey, machine: &mut dyn MachineInput);
    fn on_host_gamepad(&mut self, event: GamepadEvent, machine: &mut dyn MachineInput);
    fn on_host_mouse(&mut self, event: MouseEvent, machine: &mut dyn MachineInput);
}

/// Machine-side input interface.
pub trait MachineInput {
    fn keyboard(&mut self) -> Option<&mut dyn EmulatedKeyboard>;
    fn joystick(&mut self, port: u8) -> Option<&mut dyn EmulatedJoystick>;
    fn mouse(&mut self) -> Option<&mut dyn EmulatedMouse>;
    fn paddle(&mut self, port: u8) -> Option<&mut dyn EmulatedPaddle>;
    fn light_gun(&mut self) -> Option<&mut dyn EmulatedLightGun>;
}
```

The Input Manager lives in `emu-input`. It receives raw host events from the shell, applies mappings, and feeds the emulated input devices in `machine-*`. The shell doesn't know about emulated keyboards or joystick ports.

---

## 12. Peripheral devices

### 10.1 Serial and parallel ports

The transport layer that printers, modems, and other peripherals connect to.

```rust
pub trait ParallelPort: Send + Sync {
    fn write_data(&mut self, byte: u8);
    fn read_data(&self) -> u8;
    fn strobe(&mut self);
    fn busy(&self) -> bool;
    fn ack(&self) -> bool;
    fn attach(&mut self, device: Box<dyn ParallelDevice>);
    fn detach(&mut self) -> Option<Box<dyn ParallelDevice>>;
}

pub trait SerialPort: Send + Sync {
    fn write_byte(&mut self, byte: u8);
    fn read_byte(&mut self) -> Option<u8>;
    fn bytes_available(&self) -> usize;
    fn set_baud_rate(&mut self, baud: u32);
    fn set_config(&mut self, config: SerialConfig);
    fn dtr(&self) -> bool;
    fn set_dtr(&mut self, state: bool);
    fn dsr(&self) -> bool;
    fn rts(&self) -> bool;
    fn set_rts(&mut self, state: bool);
    fn cts(&self) -> bool;
    fn attach(&mut self, device: Box<dyn SerialDevice>);
    fn detach(&mut self) -> Option<Box<dyn SerialDevice>>;
}

pub struct SerialConfig {
    pub data_bits: u8,      // 5, 6, 7, 8
    pub stop_bits: u8,      // 1, 2
    pub parity: Parity,
    pub flow_control: FlowControl,
}
```

### 10.2 Printers

The emulated system writes bytes to a printer port. The emulated printer interprets them according to its command language and produces visible output.

```rust
pub trait EmulatedPrinter: Send + Sync {
    fn write_byte(&mut self, byte: u8);
    fn busy(&self) -> bool;
    fn has_output(&self) -> bool;
    fn current_page(&self) -> Option<&PrinterPage>;
    fn form_feed(&mut self);
    fn pages(&self) -> &[PrinterPage];
    fn export(&self, format: PrinterExportFormat) -> Vec<u8>;
}

pub struct PrinterPage {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
}

pub enum PrinterExportFormat {
    Png,        // individual page images
    Pdf,        // multi-page document
    Text,       // raw text extraction
    RawStream,  // raw byte stream for analysis/replay
}
```

**Printer types by system:**

| System | Interface | Typical printer | Protocol |
|--------|-----------|-----------------|----------|
| Spectrum | ZX Printer port / Centronics | ZX Printer, Epson | ZX Printer protocol, ESC/P |
| C64 | Serial bus (IEC) | MPS-801/803 | Commodore-specific |
| BBC Micro | Centronics parallel | Epson | ESC/P |
| Amiga | Centronics parallel | Epson, HP | ESC/P, PCL |
| Atari ST | Centronics parallel | Epson | ESC/P |
| MSX | Centronics parallel | Epson | ESC/P |
| Apple II | Parallel card / serial | ImageWriter | Various |

A generic ESC/P interpreter covers most systems. The ZX Printer (sends pixel rows directly, no command language) and Commodore IEC printers need specific implementations.

The ZX Printer is a good first target: simple protocol, visually distinctive (thermal silvered paper), deeply associated with the Spectrum experience. Rendering with the characteristic silver-on-black appearance would connect users to the original hardware feel.

### 10.3 Other attachable peripherals

| Device | Port | Educational value |
|--------|------|-------------------|
| Printer | Parallel / serial / IEC | Tangible output, understanding I/O |
| Modem | Serial | Networking, BBS culture (see §11) |
| Mouse | Serial / custom | Input device protocol |
| MIDI interface | Serial | Music, creative computing |
| Plotter | Serial / parallel | Vector graphics output |
| Speech synthesiser | Custom | The Spectrum's Currah µSpeech, C64 SAM |
| Tape interface | Serial | Storage, signal encoding |

Peripherals attach to the port traits as `ParallelDevice` or `SerialDevice` implementations. The machine-side UART/PIO chip implementation lives in `machine-*`; the peripheral device implementation lives in `emu-peripheral`.

---

## 13. Networking

### 11.1 Why networking matters

Networking existed across the 8/16-bit era and has genuine educational and preservation value. BBC Micro Econet was how a generation of UK schoolchildren first experienced networking. BBS culture was central to the modem era. Amiga and ST had TCP/IP stacks. Some games supported networked multiplayer.

### 11.2 Network interface

```rust
pub trait NetworkInterface: Send + Sync {
    fn send_frame(&mut self, frame: &[u8]);
    fn receive_frame(&mut self) -> Option<Vec<u8>>;
    fn mac_address(&self) -> [u8; 6];
    fn link_up(&self) -> bool;
}

pub enum NetworkBackend {
    /// User-mode networking — NAT via host, no root required.
    /// Emulated DHCP/DNS.
    UserMode {
        dhcp_range: (Ipv4Addr, Ipv4Addr),
        gateway: Ipv4Addr,
        dns: Ipv4Addr,
    },
    /// TAP/TUN — bridge to host network. Requires elevated privileges.
    TapDevice { device_name: String },
    /// Connect to another emulator instance.
    /// For Econet, ZX Net, null modem games.
    PeerToPeer { peer_address: SocketAddr },
    /// Loopback — for testing.
    Loopback,
    /// No network.
    Disconnected,
}
```

### 11.3 BBC Micro Econet

Econet deserves first-class support for its educational significance. A real LAN with station addressing, packet framing, acknowledgement, and contention — concepts that map directly to modern networking but at human-comprehensible speeds.

```rust
pub struct EconetStation {
    pub station: u8,     // 0-254 (255 = broadcast)
    pub network: u8,     // for bridged Econets
    pub adlc: Mc68b54,   // framing chip
    pub clock: bool,
}

pub enum EconetBackend {
    /// Virtual Econet — multiple emulator instances connected.
    Virtual { hub_address: SocketAddr },
    /// Single-station with virtual file server
    /// serving files from a host directory.
    WithFileServer { server_dir: PathBuf },
}
```

An emulated Econet file server serving files from a host directory would be immediately practical for BBC Micro emulation and deeply educational.

### 11.4 Modem emulation

BBS culture was central to the 8/16-bit era. Active retro BBSes still exist and are accessible via telnet. A modem emulator bridges this gap.

```rust
pub struct ModemEmulator {
    /// Hayes AT command interpreter.
    pub at_state: AtCommandState,
    pub backend: ModemBackend,
    /// Baud rate (affects character transmission timing).
    pub baud_rate: u32,
}

pub enum ModemBackend {
    /// Connect to a telnet server (for BBSes).
    Telnet { address: String, port: u16 },
    /// Null modem to another emulator instance.
    NullModem { peer: SocketAddr },
    Disconnected,
}
```

The modem attaches to a serial port as a `SerialDevice`. AT commands are interpreted locally (ATD, ATH, ATS registers). When a "dial" command specifies a number that maps to a telnet address, the modem establishes a TCP connection and bridges the serial data stream.

### 11.5 Ethernet card emulation

For systems with Ethernet hardware (Amiga Ariadne/X-Surf, C64 RR-Net, Apple II Uthernet):

The machine crate emulates the NIC chip (NE2000, CS8900A, etc.). The `NetworkInterface` trait connects it to a `NetworkBackend`. User-mode networking is the default — it provides DHCP, DNS, and NAT without elevated privileges, which is sufficient for most use cases (web browsing, FTP, telnet). TAP/TUN is available for advanced use.

### 11.6 System networking coverage

| System | Network hardware | Backend |
|--------|-----------------|---------|
| BBC Micro | Econet (ADLC chip) | Virtual Econet / file server |
| Amiga | Ethernet (NE2000, etc.) | User-mode / TAP |
| Atari ST | Ethernet / serial PPP | User-mode / TAP |
| C64 | RR-Net (CS8900A) | User-mode / TAP |
| Apple II | Uthernet (CS8900A) | User-mode / TAP |
| Spectrum | Interface 1 RS-232 / ZX Net | Null modem / peer-to-peer |
| Most systems | Modem via serial port | Telnet bridge |

---

---

## 14. IDE, assembler, and BASIC

### 14.1 From debugger to IDE

The debugger already provides disassemblers, breakpoints, stepping, register views, memory editors, and trace logging. Adding assemblers and a source editor completes the development environment. The IDE is not a separate application — it's a set of additional panels consuming the same observation layer.

### 14.2 Assembler

Each CPU has a corresponding assembler alongside its disassembler:

```rust
pub trait Assembler: Send + Sync {
    fn assemble(&self, source: &str, config: &AssemblerConfig)
        -> Result<AssemblyResult, Vec<AssemblyError>>;
    fn target_cpu(&self) -> &str;
    fn directives(&self) -> Vec<DirectiveInfo>;
}

pub struct AssemblerConfig {
    pub origin: u32,
    pub include_paths: Vec<PathBuf>,
    pub defines: HashMap<String, String>,
    pub output_format: AssemblyOutputFormat,
}

pub enum AssemblyOutputFormat {
    /// Raw binary blob.
    RawBinary,
    /// System-specific loadable format (TAP block, PRG, SNA, etc.)
    SystemSpecific(SystemId),
}

pub struct AssemblyResult {
    pub binary: Vec<u8>,
    pub symbols: SymbolTable,
    pub warnings: Vec<AssemblyWarning>,
    pub listing: Vec<ListingLine>,
}

pub struct AssemblyError {
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub severity: ErrorSeverity,
}
```

### 14.3 Symbol table

The symbol table bridges assembler, debugger, and disassembler. When source is assembled, the symbol table maps labels to addresses. The disassembler uses it to show labels instead of raw addresses. Breakpoints can be set by label name.

```rust
pub struct SymbolTable {
    symbols: BTreeMap<String, SymbolEntry>,
    reverse: BTreeMap<u32, Vec<String>>,
    source_map: Vec<SourceMapping>,
}

pub struct SymbolEntry {
    pub name: String,
    pub address: u32,
    pub kind: SymbolKind,
    pub source: Option<SourceLocation>,
}

pub enum SymbolKind {
    Code,       // subroutine entry point
    Data,       // data label
    Constant,   // EQU / define
    Variable,   // RAM location
    IoPort,     // hardware register
}

pub struct SourceMapping {
    pub file: PathBuf,
    pub line: u32,
    pub address_range: (u32, u32),
}
```

### 14.4 External symbol file import

Not everyone will use the built-in assembler. The debugger imports symbol files from external tools:

| Tool | Format | Notes |
|------|--------|-------|
| PASMO | `.sym` | `label EQU $address` |
| z88dk | `.map` | Linker map file |
| cc65/ca65 | `.dbg` | Debug info file |
| vasm | `.sym` | Symbol table output |
| RGBDS | `.sym` | Game Boy assembler |
| Generic | `.sym` | One `label = $address` per line |

### 14.5 Development workflow

Edit source in the Source Editor panel → Assemble → binary + symbol table produced → binary loaded into emulated memory (as raw poke, or wrapped in system-specific format) → symbol table shared with debugger → disassembly shows labels → breakpoints set by label → stepping highlights current source line in editor → modify source → reassemble → cycle repeats.

```rust
pub struct IdeProject {
    pub name: String,
    pub target_system: SystemId,
    pub target_cpu: String,
    pub source_files: Vec<PathBuf>,
    pub include_paths: Vec<PathBuf>,
    pub assembler_config: AssemblerConfig,
    pub current_symbols: Option<SymbolTable>,
    pub last_build: Option<AssemblyResult>,
}
```

### 14.6 BASIC text loading

Every BASIC-equipped system stores programs in a tokenised format. The system's interpreter can't load a plain text `.bas` file — it expects its own token format. But for educational and development purposes, loading BASIC from plain text is enormously valuable.

```rust
pub trait BasicTokeniser: Send + Sync {
    /// Tokenise a text BASIC program into the system's internal format.
    fn tokenise(&self, source: &str) -> Result<TokenisedBasic, Vec<BasicError>>;
    /// Detokenise from internal format back to text.
    fn detokenise(&self, data: &[u8]) -> Result<String, BasicError>;
    /// Validate without tokenising.
    fn validate(&self, source: &str) -> Vec<BasicWarning>;
}

pub struct TokenisedBasic {
    /// Tokenised program bytes.
    pub data: Vec<u8>,
    /// Where in memory to load.
    pub load_address: u16,
    /// System variables that need updating (BASIC pointers, etc.)
    pub system_vars: Vec<(u16, Vec<u8>)>,
}

pub struct BasicError {
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub line_text: String,
}
```

The tokeniser lives in `machine-*` because each system's BASIC has different tokens, line number formats, and quirks:

| System | BASIC | Token format | Quirks |
|--------|-------|-------------|--------|
| Spectrum | Sinclair BASIC | 1-byte tokens, 5-byte number encoding inline | Keywords are single keypresses, FN/DEF FN |
| C64 | CBM BASIC V2 | 1-byte tokens ($80-$FF), PETSCII | Abbreviated keywords (pR for PRINT) |
| BBC Micro | BBC BASIC | 1-byte tokens ($80-$FF) | Inline assembler, OSCLI |
| Amstrad CPC | Locomotive BASIC | 1-byte and 2-byte tokens | 16-bit LE line numbers |
| MSX | MSX BASIC | CBM-style tokens | |
| Apple II | Applesoft BASIC | CBM-style tokens | |
| Atari 8-bit | Atari BASIC | Statement-level tokenisation | Different from CBM approach |

### 14.7 BASIC loading pipeline

A `.bas` text file enters the emulator as a source artifact:

```
.bas text file (UTF-8)
  → Sniffer identifies as BASIC text (by extension + content heuristic)
  → BasicTokeniser produces TokenisedBasic
  → Injector loads bytes into emulated memory at load_address
  → Injector updates system variables (pointers, line count)
  → Optionally sends RUN keystrokes to the emulated keyboard
```

This is closest to the state artifact pathway — it modifies machine state directly — but it's not a snapshot. It's a "program load" operation.

### 14.8 BASIC editor panel

The BASIC Editor is a specialised panel for BASIC development:

| Feature | How it works |
|---------|-------------|
| Load `.bas` text file | Tokenise and inject into emulated memory |
| Save `.bas` text file | Detokenise current BASIC program from memory to text |
| Syntax highlighting | BASIC keyword highlighting |
| Auto-number | Automatic line numbering with configurable step |
| Renumber | Renumber all lines, updating GOTOs and GOSUBs |
| Run | Inject tokenised program, send RUN keystrokes |
| Cross-reference | Find all references to a variable or line number |
| Export to media | Tokenise and save as loadable media file (TAP block, PRG, etc.) |

The detokeniser is a preservation tool — load any BASIC program from tape/disk/snapshot and export as readable text.

### 14.9 Where IDE components live

| Component | Crate | Rationale |
|-----------|-------|-----------|
| Z80 assembler | `cpu-z80` (or `asm-z80`) | Paired with disassembler, CPU-specific |
| 6502 assembler | `cpu-6502` (or `asm-6502`) | Same |
| 68000 assembler | `cpu-m68k` (or `asm-m68k`) | Same |
| Symbol table | `emu-debug` | Shared between assembler and debugger |
| Symbol file import | `emu-debug` | Parsing external tool output |
| Source editor panel | `emu-ide` | Editor widget, project management |
| BASIC tokenisers | `machine-*` | System-specific token tables |
| BASIC editor panel | `emu-ide` | BASIC-specific editor features |
| IDE project model | `emu-ide` | Source files, build config, workflow |

---

## 15. Tape

### 5.1 Core insight

Model tape as **signal over time**, not as bytes or blocks.

The canonical tape abstraction answers: what signal level exists at position X? What does the machine see on its input pin right now? What should the user hear right now?

Every input format — TAP, TZX, PZX, CSW, WAV — is normalised into one internal representation consumed by both the machine input path and the audio renderer.

### 5.2 Architecture

```
[Format Parser] → [Importer/Adapter]
                        |
                        v
                   TapeTimeline (trait object)
                        |
                   TapeTransport
                  /      |       \
   TapeInputPath  TapeAudioRenderer  TapeCounter/UI
```

### 5.3 Canonical model: trait, not enum

```rust
pub type TapePositionNs = u64;

pub trait TapeTimeline: Send + Sync {
    fn signal_at(&self, position: TapePositionNs) -> bool;
    fn duration(&self) -> TapePositionNs;
    fn next_edge(&self, position: TapePositionNs) -> Option<TapePositionNs>;
    fn prev_edge(&self, position: TapePositionNs) -> Option<TapePositionNs>;

    fn render_audio(
        &self,
        start: TapePositionNs,
        sample_rate: u32,
        buffer: &mut [f32],
    ) {
        let ns_per_sample = 1_000_000_000u64 / sample_rate as u64;
        for (i, sample) in buffer.iter_mut().enumerate() {
            let pos = start + (i as u64) * ns_per_sample;
            *sample = if self.signal_at(pos) { 0.8 } else { -0.8 };
        }
    }
}
```

### 5.4 Pulse-domain representation

For TZX/PZX/TAP-like formats. All control flow resolved at import time.

```rust
pub struct PulseTimeline {
    edges: Vec<TapePositionNs>,
    initial_level: bool,
    total_duration: TapePositionNs,
}
```

### 5.5 Sampled representation

For WAV/CSW/direct recording.

```rust
pub struct SampledTimeline {
    samples: Vec<f32>,
    sample_rate: u32,
    total_duration: TapePositionNs,
    conditioning: SignalConditioning,
}

pub struct SignalConditioning {
    pub high_threshold: f32,
    pub low_threshold: f32,
    pub dc_removal: bool,
    pub invert: bool,
}
```

### 5.6 Transport

```rust
pub trait TapeTransport {
    fn play(&mut self);
    fn stop(&mut self);
    fn rewind(&mut self);
    fn fast_forward(&mut self);
    fn position(&self) -> TapePositionNs;
    fn seek(&mut self, position: TapePositionNs);
    fn counter_value(&self) -> i32;
    fn counter_reset(&mut self);

    // Machine-driven (default no-ops)
    fn set_motor(&mut self, _running: bool) {}
    fn motor_running(&self) -> bool { true }
    fn sense_pressed(&self) -> bool { false }
}
```

### 5.7 Audible loading screech

Audio rendering is another consumer of the `TapeTimeline` trait.

| Renderer mode | Behaviour |
|---------------|-----------|
| **Ideal** | Clean square wave from `render_audio()` default |
| **Conditioned** | Rise/fall shaping, low-pass filtering, optional hiss |
| **Authentic** | Preserved analogue character from sampled sources |

Conditioned mode parameters are tunable per system.

### 5.8 Tape writing

When the machine writes to tape (Spectrum MIC, C64 datasette write line), the transport records the output signal as new edges or samples. The modified timeline can be exported through the format export path.

### 5.9 Tape format importers

System-aware — content inspection, not just file extension.

| Format | Crate strategy | Rationale |
|--------|---------------|-----------|
| TZX | `parser-tzx` + `format-tzx` | Complex block types, control flow |
| PZX | `parser-pzx` + `format-pzx` | Similar complexity |
| TAP | `format-tap` (single) | Trivial |
| CSW | Assess at implementation | |
| WAV | Existing `hound` crate | Standard format |
| C64 TAP | `format-c64tap` | Different from Spectrum TAP |

### 5.10 Malformed file strategy

1. Tolerant parsing with diagnostics
2. ±5% timing tolerance in importers
3. Known-issues catalogue for regression, not runtime
4. No per-game hacks in the runtime

---

## 16. Floppy disk

### 10.1 Two levels of truth

**Logical-level** (ADF, D64, DSK, TRD): decoded sector data. Most software.

**Flux-level** (IPF, KryoFlux, SCP): raw magnetic flux transitions. Copy-protected software.

### 10.2 Architecture

```rust
pub trait DiskImage: Send + Sync {
    fn sides(&self) -> u8;
    fn tracks_per_side(&self) -> u8;
    fn read_sector(&self, track: u8, head: u8, sector: u8)
        -> Result<Vec<u8>, DiskError>;
    fn write_sector(&mut self, track: u8, head: u8, sector: u8, data: &[u8])
        -> Result<(), DiskError>;
    fn has_flux(&self, track: u8, head: u8) -> bool { false }
    fn read_flux(&self, _track: u8, _head: u8) -> Option<&FluxTrack> { None }
}

pub struct FluxTrack {
    transitions: Vec<u32>,  // flux positions in ns within one rotation
    rotation_ns: u32,
}

pub struct FloppyDrive {
    disk: Option<Box<dyn DiskImage>>,
    current_track: u8,
    current_head: u8,
    motor_running: bool,
    rotation_position: u32,
    write_protected: bool,
}
```

### 10.3 Format coverage

| Format | Type | Systems |
|--------|------|---------|
| ADF | Logical | Amiga |
| D64 | Logical | C64 |
| DSK | Logical | Spectrum +3, CPC |
| TRD | Logical | Spectrum (TR-DOS) |
| SCL | Logical | Spectrum (TR-DOS) |
| IPF | Flux | Amiga, Atari ST, CPC |
| KryoFlux | Flux | Any |
| SCP | Flux | Any |
| G64 | Flux-ish | C64 |

---

## 17. Optical disc

### 15.1 Architecture

```rust
pub trait OpticalDisc: Send + Sync {
    fn toc(&self) -> &[DiscTrack];
    fn read_sector_raw(&self, lba: u32) -> Result<[u8; 2352], DiscError>;
    fn read_subchannel_q(&self, lba: u32) -> Result<SubchannelQ, DiscError>;
    fn sector_count(&self) -> u32;
    fn is_audio_track(&self, lba: u32) -> bool;
}

pub struct OpticalDrive {
    disc: Option<Box<dyn OpticalDisc>>,
    head_position: u32,
    spinning: bool,
    seek_model: Box<dyn SeekModel>,
    audio_playback: Option<AudioPlaybackState>,
    tray_open: bool,
}
```

### 15.2 Format coverage

| Format | Sectors | Subchannel | Primary target |
|--------|---------|------------|----------------|
| BIN/CUE | Raw 2352 | Sometimes | Yes |
| CHD | Compressed | Yes | Yes (archival) |
| ISO | Mode 1 only | No | Fallback |
| CCD/IMG/SUB | Raw | Yes | Protected discs |

DVD: deferred until PS2.

---

## 18. Microdrive

```rust
pub struct MicrodriveCartridge {
    sectors: Vec<MicrodriveSector>,
    loop_duration_ns: u64,
    write_protected: bool,
}

pub trait MicrodriveMechanism: Send + Sync {
    fn head_position(&self) -> u64;
    fn advance(&mut self, ns: u64);
    fn current_sector_header(&self) -> Option<&MicrodriveSectorHeader>;
    fn read_sector_data(&self) -> Option<&[u8]>;
    fn write_sector_data(&mut self, data: &[u8]) -> Result<(), MicrodriveError>;
}
```

Continuous loop of tape, sector-addressed. QL shares the mechanism; filesystem interpretation differs.

---

## 19. Famicom Disk System

```rust
pub struct FdsDisk {
    pub side_a: FdsDiskSide,
    pub side_b: FdsDiskSide,
}

pub struct FdsDrive {
    disk: Option<FdsDisk>,
    current_side: DiskSide,
    head_position: u32,
    motor_running: bool,
    transfer_active: bool,
    write_mode: bool,
    transfer_timer: u32,
    crc_accumulator: u16,
    disk_inserted: bool,
    write_protected: bool,
}
```

Sequential, not random access. Machine-driven motor control. Read/write. Gap-length-sensitive. RAM adapter ASIC also contains 32KB RAM and wavetable sound channel.

---

## 20. ROM media / Cartridges

```rust
pub trait CartridgeMapper: Send + Sync {
    fn read(&self, address: u16) -> u8;
    fn write(&mut self, address: u16, value: u8);
    fn has_persistent_storage(&self) -> bool;
    fn battery_ram(&self) -> Option<&dyn BatteryBackedRam>;
    fn battery_ram_mut(&mut self) -> Option<&mut dyn BatteryBackedRam>;
}
```

Format parser determines mapper. Machine memory bus delegates to mapper. Mapper implementations in `machine-*`. Database-driven identification supplements broken headers.

---

## 21. State artifacts

### 15.1 Snapshots

```
MediaSource → Sniffer → Parser → SnapshotImage → Applicator → Machine State
```

| Format | System | Notes |
|--------|--------|-------|
| Z80 | Spectrum | Multiple versions, compression |
| SNA | Spectrum | Simple fixed layout |
| SZX | Spectrum | Modern, extensible |

### 15.2 Input recordings

RZX: initial snapshot + input stream for deterministic replay.

---

## 22. Persistent storage

### 20.1 Two sub-patterns

**Block-addressed** (PS1 memory card, GameCube, N64 Controller Pak, Dreamcast VMU):

```rust
pub trait BlockStorage: PersistentMediaState + Send + Sync {
    fn block_size(&self) -> usize;
    fn block_count(&self) -> usize;
    fn read_block(&self, index: usize, buf: &mut [u8]) -> Result<(), StorageError>;
    fn write_block(&mut self, index: usize, data: &[u8]) -> Result<(), StorageError>;
    fn inserted(&self) -> bool;
}
```

**Battery-backed RAM** (NES/SNES/Mega Drive cart SRAM, Saturn backup RAM):

```rust
pub trait BatteryBackedRam: PersistentMediaState + Send + Sync {
    fn size(&self) -> usize;
    fn read(&self, offset: usize) -> u8;
    fn write(&mut self, offset: usize, value: u8);
}
```

### 20.2 Persistence trait

```rust
pub trait PersistentMediaState: Send + Sync {
    fn source_identity(&self) -> &MediaIdentity;
    fn is_dirty(&self) -> bool;
    fn save(&self) -> Vec<u8>;
    fn load(&mut self, data: &[u8]) -> Result<(), MediaError>;
}
```

### 16.3 Auto-save policy

- Periodic save when dirty (every 5 seconds)
- Save on emulator exit
- Save at end of write transaction for block devices
- Never auto-save mid-transaction

### 16.4 Delta vs full persistence

| Media type | Approach |
|------------|----------|
| Cart SRAM | Full `.sav` file |
| FDS disk writes | Delta against original |
| PS1 memory card | Full `.mcr`/`.mcd` |
| Amiga writable floppy | Delta against original |

---

## 23. Media hotswap and multi-disk

### 21.1 Hotswap

```rust
pub trait MountableDevice {
    fn is_media_present(&self) -> bool;
    fn eject(&mut self) -> Option<Box<dyn Any>>;
    fn insert(&mut self, media: Box<dyn Any>) -> Result<(), MediaError>;
    fn media_changed(&mut self) -> bool;
}
```

### 21.2 Multi-disk bundles

```rust
pub struct MediaBundle {
    pub name: String,
    pub entries: Vec<MediaBundleEntry>,
}
```

Inferred from TOSEC naming conventions or defined manually.

---

## 24. Media identification

### 18.1 Sniffer pipeline

1. Extension hint
2. Magic bytes / header signatures
3. System context
4. Disambiguation via content structure
5. Database lookup (No-Intro, TOSEC, Redump)

```rust
pub struct MediaIdentification {
    pub format: MediaFormat,
    pub system_hint: Option<SystemId>,
    pub confidence: IdentificationConfidence,
    pub database_match: Option<DatabaseMatch>,
    pub warnings: Vec<IdentificationWarning>,
}
```

### 18.2 Compressed archives

ZIP, 7z, RAR, GZ handled as an unwrapping step before identification. Multi-file sets (CUE+BIN) assembled automatically.

---

## 25. Save state and media interaction

Save state captures:
- full machine state
- all mounted device transport state
- media identity per mounted medium

Save state does **not** capture the full media image. On load, verify mounted media matches stored identity.

```rust
pub struct MountedDeviceState {
    pub media_identity: MediaIdentity,
    pub transport_state: Vec<u8>,
}
```

---

## 26. Write-back and archive preservation

### 20.1 Write-protect model

- Physical write protect (format-specific)
- Emulator-imposed read-only lock
- Archive preservation mode (default): originals read-only, changes stored as deltas

### 20.2 Default: archive preservation

Original files are never modified. All write-back goes through deltas or separate save files.

---

## 27. Rewind and time travel

### 25.1 Concept

Rewind allows the user to hold a button and reverse emulation by several seconds. This is implemented as periodic state snapshots stored in a ring buffer, replayed in reverse when rewind is engaged.

### 25.2 Snapshot ring buffer

```rust
pub struct RewindBuffer {
    /// Ring buffer of compressed state snapshots.
    snapshots: VecDeque<RewindSnapshot>,
    /// Maximum number of snapshots to retain.
    capacity: usize,
    /// Interval between snapshots in emulated frames.
    snapshot_interval: u32,
    /// Frames since last snapshot.
    frames_since_snapshot: u32,
    /// Total rewind duration available (derived from capacity × interval).
    max_rewind_seconds: f32,
}

pub struct RewindSnapshot {
    /// Compressed machine state.
    state: Vec<u8>,
    /// Frame number this snapshot represents.
    frame: u64,
    /// Master cycle at snapshot time.
    cycle: u64,
    /// Size in bytes (for memory budget tracking).
    compressed_size: usize,
}
```

### 25.3 Sizing

Snapshot size varies dramatically by system:

| System | Uncompressed state | Compressed (LZ4) | At 1/frame, 10s buffer |
|--------|-------------------|-------------------|------------------------|
| Spectrum 48K | ~50 KB | ~15 KB | ~7.5 MB |
| Spectrum 128K | ~150 KB | ~40 KB | ~20 MB |
| NES | ~40 KB | ~10 KB | ~6 MB |
| C64 | ~80 KB | ~25 KB | ~12.5 MB |
| Amiga (1MB chip) | ~1.2 MB | ~400 KB | ~200 MB |
| SNES | ~200 KB | ~60 KB | ~30 MB |

For systems with large RAM (Amiga, later consoles), per-frame snapshots are expensive. Strategies:

- **Reduce snapshot frequency** — every 4th or 8th frame instead of every frame. Rewind granularity is coarser but memory use drops proportionally.
- **Delta compression** — store only the bytes that changed since the previous snapshot. Most frames change only a small fraction of RAM. Initial snapshot is full; subsequent snapshots are deltas.
- **Keyframe + delta** — full snapshot every N frames, deltas in between. Seeking to an arbitrary point requires finding the nearest keyframe and replaying deltas forward.

```rust
pub enum RewindCompression {
    /// Full snapshot every time. Simple, predictable size.
    Full,
    /// Delta against previous snapshot. Much smaller for most systems.
    Delta,
    /// Keyframe every N snapshots, deltas between. Best for large-state systems.
    KeyframeDelta { keyframe_interval: u32 },
}
```

### 25.4 What rewind captures and doesn't capture

**Captured:** full machine state — CPU, memory, all chip registers, video state, audio state, clock position, mounted device transport state (tape position, disk head, motor state).

**Not captured:** persistent storage state (memory cards, battery SRAM, writable disk deltas). If the user rewinds past a save operation, the save file is not "unsaved." This matches the save state model (§23) and prevents data loss.

**Not captured:** host state (window position, UI state, input state). Rewind affects the emulated machine only.

### 25.5 Rewind playback

When the user engages rewind, the emulator:

1. Stops forward emulation
2. Steps backward through the snapshot buffer, one snapshot per display frame (or faster/slower depending on rewind speed)
3. Restores each snapshot to the machine state
4. Renders the frame from the restored state
5. Optionally plays audio in reverse (or mutes — reverse audio is usually unpleasant)

When the user releases rewind, forward emulation resumes from the current restored state. All snapshots after the current point are discarded (the timeline forks).

### 25.6 Interaction with speed control

At 2x speed, snapshots are taken at 2x rate (twice as many per second of wall-clock time but the same interval in emulated frames). The rewind buffer fills faster. At turbo speed, snapshot frequency can be reduced to avoid overwhelming the buffer.

### 25.7 MCP integration

```rust
pub trait McpRewindInterface {
    fn rewind_available_seconds(&self) -> f32;
    fn rewind_to_frame(&mut self, frame: u64);
    fn rewind_by_seconds(&mut self, seconds: f32);
    fn rewind_snapshot_count(&self) -> usize;
    fn rewind_memory_usage_bytes(&self) -> usize;
}
```

---

## 28. Configuration and settings

### 26.1 Configuration hierarchy

```
Global config (applies to all systems)
  → System family config (applies to all variants of a system)
    → Variant config (applies to a specific variant)
      → Session overrides (temporary changes for this run)
```

Each level inherits from the level above and can override any setting.

### 26.2 Configuration domains

```rust
pub struct GlobalConfig {
    /// Audio output settings.
    pub audio: AudioOutputConfig,
    /// Display defaults (CRT preset, PAR preference).
    pub display: DisplayConfig,
    /// Speed control defaults.
    pub speed: SpeedControlConfig,
    /// Rewind settings.
    pub rewind: RewindConfig,
    /// File paths.
    pub paths: PathConfig,
    /// UI preferences.
    pub ui: UiConfig,
    /// MCP server settings.
    pub mcp: McpConfig,
}

pub struct PathConfig {
    /// Where to look for ROMs.
    pub rom_dirs: Vec<PathBuf>,
    /// Where to store save files (battery SRAM, memory cards).
    pub save_dir: PathBuf,
    /// Where to store save states.
    pub state_dir: PathBuf,
    /// Where to store screenshots, video, audio captures.
    pub capture_dir: PathBuf,
    /// Where to store rewind buffer (if spilling to disk).
    pub rewind_dir: PathBuf,
    /// Recent files list.
    pub recent_files: Vec<RecentFileEntry>,
}

pub struct SystemConfig {
    pub system: SystemId,
    /// Default variant for this system.
    pub default_variant: VariantId,
    /// Default region.
    pub default_region: Region,
    /// Default extensions to attach.
    pub default_extensions: Vec<ExtensionId>,
    /// Input profile for this system.
    pub input_profile: InputProfile,
    /// Display overrides (system-specific CRT/signal defaults).
    pub display_overrides: Option<DisplayConfig>,
    /// Audio overrides (per-channel volumes, mutes).
    pub audio_overrides: Option<AudioMixerConfig>,
}
```

### 26.3 Persistence

Configuration is stored as TOML files:

```
~/.config/emu198x/              (or platform-appropriate location)
├── global.toml                 — global settings
├── systems/
│   ├── spectrum.toml           — Spectrum family defaults
│   ├── nes.toml                — NES family defaults
│   ├── c64.toml                — C64 family defaults
│   └── ...
├── input/
│   ├── spectrum-default.toml   — default Spectrum input profile
│   ├── spectrum-gaming.toml    — gaming-oriented profile
│   ├── nes-default.toml
│   └── ...
├── display/
│   ├── presets.toml            — CRT preset definitions
│   └── custom-presets/         — user-created presets
└── roms.toml                   — ROM locations and hash cache
```

TOML is chosen over JSON/YAML because it's human-readable, human-editable, widely supported in Rust (`toml` crate), and doesn't have YAML's footgun-rich syntax.

### 26.4 ROM location management

ROMs are located by hash, not path. On first run or when ROM directories change, the emulator scans configured directories, hashes every file, and builds an index:

```rust
pub struct RomIndex {
    /// Map from SHA-256 hash to file location.
    entries: HashMap<RomHash, PathBuf>,
    /// Last scan time.
    last_scan: SystemTime,
}
```

When a variant needs a ROM, it declares a `RomRequirement`. The ROM manager looks up the required hash in the index. If not found, it reports the missing ROM with a human-readable name and expected hash, and suggests where to place it.

### 26.5 Recent files and session restore

The recent files list tracks the last N media files loaded per system. On launch, the emulator can optionally restore the last session (load the same media, restore the save state).

---

## 29. Error handling

### 27.1 Policy

Errors are values, not panics. Every fallible operation returns `Result<T, E>` with a typed error. Errors propagate upward to the nearest consumer that can meaningfully handle them. The project-wide rules:

- **Never panic** in library crates (`cpu-*`, `machine-*`, `emu-*`, `format-*`, `parser-*`). `unwrap()` and `expect()` are banned outside of tests and provably-infallible cases (e.g., indexing a fixed-size array with a bounded value).
- **Never silently swallow errors.** If an error is handled, log it. If it's not handled, propagate it.
- **Surface errors to the user** through the shell's notification system. The shell decides how to display them (toast notification, status bar, dialog).
- **Return errors from MCP tools** as structured error responses. The MCP client (Claude Code, regression harness) decides what to do with them.
- **Log everything** at appropriate levels. Errors at `error`, recoverable issues at `warn`, notable events at `info`, implementation detail at `debug`/`trace`.

### 27.2 Error types

```rust
/// Top-level emulator error — what the shell and MCP server deal with.
pub enum EmulatorError {
    /// Media loading/identification failures.
    Media(MediaError),
    /// Machine configuration problems.
    Config(ConfigError),
    /// Runtime emulation errors (should be rare — indicates a bug).
    Emulation(EmulationError),
    /// File I/O errors.
    Io(IoError),
    /// ROM-related errors.
    Rom(RomError),
}

pub enum MediaError {
    /// File not recognised by any parser.
    UnrecognisedFormat { path: PathBuf, attempted: Vec<String> },
    /// Parser recognised the format but the file is corrupt.
    CorruptFile { path: PathBuf, format: String, detail: String },
    /// Parser succeeded but the data doesn't make sense.
    InvalidContent { detail: String, warnings: Vec<String> },
    /// Import into canonical form failed.
    ImportFailed { format: String, detail: String },
    /// File system error during media access.
    IoError(std::io::Error),
}

pub enum RomError {
    /// Required ROM not found in any configured directory.
    NotFound { name: String, expected_hash: String, expected_size: usize },
    /// ROM found but hash doesn't match any known version.
    HashMismatch { name: String, path: PathBuf, got_hash: String },
    /// ROM found but size is wrong.
    SizeMismatch { name: String, path: PathBuf, expected: usize, got: usize },
    /// Multiple ROM versions found, need user selection.
    AmbiguousVersion { name: String, candidates: Vec<RomCandidate> },
}

pub enum ConfigError {
    /// Extension is incompatible with this variant.
    IncompatibleExtension { extension: ExtensionId, variant: VariantId, reason: String },
    /// Two extensions conflict with each other.
    ExtensionConflict { a: ExtensionId, b: ExtensionId, reason: String },
    /// Invalid configuration value.
    InvalidValue { key: String, value: String, expected: String },
}
```

### 27.3 Diagnostic context

Errors carry enough context for the user to understand what went wrong and what to do about it. The shell formats them as human-readable messages. The MCP server returns them as structured data.

```rust
pub trait DiagnosticError: std::error::Error {
    /// Human-readable summary.
    fn summary(&self) -> String;
    /// Detailed explanation with context.
    fn detail(&self) -> String;
    /// Suggested action for the user.
    fn suggestion(&self) -> Option<String>;
    /// Severity level.
    fn severity(&self) -> ErrorSeverity;
}

pub enum ErrorSeverity {
    /// Fatal — cannot continue. ROM missing, corrupt save state.
    Fatal,
    /// Error — operation failed but emulator continues. Media load failed.
    Error,
    /// Warning — operation succeeded with caveats. Malformed TZX block skipped.
    Warning,
    /// Info — notable but not problematic. ROM version is unusual.
    Info,
}
```

### 27.4 Warning accumulation

Some operations (particularly media import) produce multiple warnings without being outright failures. These accumulate during the operation and are returned alongside the result:

```rust
pub struct ImportResult<T> {
    pub value: T,
    pub warnings: Vec<ImportWarning>,
}

pub struct ImportWarning {
    pub code: WarningCode,
    pub message: String,
    /// Position in the source file where the issue was found.
    pub source_location: Option<String>,
}
```

The malformed TZX strategy (§13.10) produces warnings for spec violations that don't prevent loading. These warnings surface in the UI and in MCP responses so the user knows the file has issues.

---

## 30. Testing strategy

### 28.1 Test layers

Testing operates at four levels, each with different tools and coverage targets:

| Layer | What it tests | How | Speed |
|-------|--------------|-----|-------|
| **Unit** | Individual functions, data structures, algorithms | `#[test]`, `proptest` | Fast (seconds) |
| **Integration** | Crate interactions, trait implementations | `#[test]` with multi-crate fixtures | Fast-medium |
| **Validation** | CPU/chip accuracy against known-good reference | External test ROMs/suites | Medium (minutes) |
| **Regression** | Full-system output stability | TOSEC harness, framebuffer/audio hashing | Slow (hours) |

### 28.2 CPU validation suites

CPU accuracy is validated against established test suites. These run as integration tests:

| CPU | Test suite | What it validates |
|-----|-----------|-------------------|
| Z80 | FUSE test suite | Every documented instruction, flags, timing |
| Z80 | Patrik Rak's Z80 tests | Undocumented behaviour, flag affection |
| Z80 | zexall/zexdoc | Thorough instruction exerciser |
| 6502 | Blargg's NES CPU tests | Instructions, timing, interrupt delivery |
| 6502 | Klaus Dormann's 6502 suite | Functional and decimal mode tests |
| 6502 | Tom Harte's ProcessorTests | Per-instruction cycle-accurate tests (10,000 per opcode) |
| 68000 | Musashi test vectors | Instruction behaviour, addressing modes |

```rust
#[test]
fn z80_fuse_tests() {
    let suite = FuseTestSuite::load("tests/fuse/");
    for test in suite.tests() {
        let mut cpu = Z80::new(Z80Config::default());
        cpu.set_state(&test.initial_state);
        cpu.execute_one(&mut test.memory);
        assert_eq!(cpu.state(), test.expected_state,
            "FUSE test {} failed", test.name);
    }
}
```

These tests are deterministic, fast, and comprehensive. They should run in CI on every commit that touches a CPU crate.

### 28.3 Chip validation

Beyond CPUs, other chips have testable behaviour:

| Chip | Validation approach |
|------|-------------------|
| AY-3-8912 | Frequency accuracy tests, envelope timing, noise LFSR sequence |
| SID 6581/8580 | Filter response curve comparison against real chip captures |
| ULA (Spectrum) | Contention timing tables, floating bus values, frame timing |
| PPU (NES) | Blargg's PPU tests (vbl_nmi_timing, sprite_hit, sprite_overflow) |
| VIC-II | Raster timing, badline detection, sprite collision |

### 28.4 Format parser tests

Every format parser needs:

- **Round-trip tests** — parse a file, re-serialise, compare byte-for-byte (for formats that support writing)
- **Known-good corpus** — a set of well-formed files that must parse without error
- **Known-bad corpus** — a set of malformed files that must produce appropriate errors or warnings, not panics
- **Fuzz testing** — `cargo-fuzz` with `arbitrary` to generate random inputs and ensure no panics

```rust
#[test]
fn tzx_round_trip() {
    let bytes = include_bytes!("fixtures/test.tzx");
    let parsed = parser_tzx::parse(bytes).unwrap();
    let reserialized = parser_tzx::serialize(&parsed);
    assert_eq!(bytes.as_slice(), reserialized.as_slice());
}

#[test]
fn tzx_malformed_no_panic() {
    // Every byte sequence should either parse or return an error, never panic.
    for case in load_malformed_corpus("fixtures/malformed/") {
        let result = parser_tzx::parse(&case);
        // We don't care if it's Ok or Err, just that it doesn't panic.
        let _ = result;
    }
}
```

### 28.5 Clock tree and scheduler tests

Mathematical validation that the clock tree produces correct timing relationships:

```rust
#[test]
fn nes_ntsc_cpu_ppu_ratio() {
    let tree = nes_ntsc_clock_tree(0);  // phase 0 for determinism
    // CPU ticks every 12 master cycles, PPU every 4.
    // In 12 master cycles: 3 PPU ticks, 1 CPU tick.
    let mut scheduler = Scheduler::new();
    scheduler.advance(12);
    assert_eq!(scheduler.component_ticks("cpu"), 1);
    assert_eq!(scheduler.component_ticks("ppu"), 3);

    // Over one NTSC frame (341×262×4 = 357368 master cycles):
    // CPU should get 29780.666... → 29780 or 29781 ticks.
    scheduler.advance(341 * 262 * 4);
    assert!(scheduler.component_ticks("cpu") == 29780
         || scheduler.component_ticks("cpu") == 29781);
}
```

### 28.6 Integration tests

Multi-crate integration tests validate that components work together correctly:

- **Media pipeline tests** — load a TAP file → import to PulseTimeline → verify signal_at() produces correct levels at known positions → verify audio render produces non-silent output
- **Save state round-trip** — run a machine for N frames → save state → run for M more frames → restore state → verify machine state matches the saved point
- **Extension attachment** — attach a DivMMC to a Spectrum → verify it intercepts the expected addresses → verify it doesn't break normal operation
- **Input mapping** — send host key events through the InputManager → verify the correct matrix positions are pressed on the emulated keyboard

### 28.7 Regression harness (TOSEC)

Full-system regression testing as described in §4.13. This is the outermost test layer. It runs less frequently (nightly or weekly) due to the size of the corpus and the time required.

Key metrics per ROM/tape/disk:

- **Boot success** — does the emulator reach a stable frame without crashing?
- **Framebuffer hash** — does the rendered output match the stored baseline?
- **Audio hash** — does the audio output match the stored baseline?
- **Timing validation** — does the frame take the expected number of master cycles?

New baselines are generated when a system is first brought up. Regressions are flagged when a previously-passing ROM produces different output.

### 28.8 CI pipeline

```
On every commit:
  → cargo fmt --check
  → cargo clippy --workspace
  → cargo test --workspace          (unit + integration + CPU validation)
  → cargo test --workspace --release (catch debug/release behaviour differences)

Nightly:
  → Full regression harness against TOSEC corpus
  → Fuzz testing (parser crates, 10 minutes per target)
  → Performance benchmarks (frames per second per system)

On release:
  → All of the above
  → Cross-platform build verification (Linux, macOS, Windows, WASM)
```

---

---

## 31. Reference management

### 31.1 Principle

Every cycle-accurate implementation decision must be traceable to a documented source. "Because the other emulator does it this way" is not a valid reference. The sources are: manufacturer datasheets, hardware reverse-engineering analyses, die photography studies, hardware test results, and authoritative community documentation.

When Claude Code implements a machine feature, it should be able to consult the relevant reference material on disk, cite the specific source, and include the citation in code comments. When a reviewer questions a timing value or a register behaviour, the citation trail should lead directly to the evidence.

### 31.2 Reference storage

References are cached on disk alongside the project in a structured directory:

```
emu198x/
├── refs/
│   ├── manifest.toml           — master catalogue of all references
│   ├── cpu/
│   │   ├── z80/
│   │   │   ├── z80-cpu-user-manual.pdf
│   │   │   ├── z80-undocumented.pdf
│   │   │   ├── z80-timing.txt
│   │   │   └── z80n-extended-instructions.pdf
│   │   ├── 6502/
│   │   │   ├── mos-6502-datasheet.pdf
│   │   │   ├── 6502-cycle-timing.pdf
│   │   │   └── 65c816-programming-manual.pdf
│   │   └── m68k/
│   │       ├── m68000-users-manual.pdf
│   │       ├── m68000-8-16-32-bit-reference.pdf
│   │       └── m68060-users-manual.pdf
│   ├── systems/
│   │   ├── sinclair-spectrum/
│   │   │   ├── spectrum-ula-book.pdf
│   │   │   ├── spectrum-contention-timing.txt
│   │   │   ├── spectrum-floating-bus.pdf
│   │   │   ├── spectrum-next-developer-guide.pdf
│   │   │   ├── if1-microdrive-manual.pdf
│   │   │   └── zx-printer-protocol.txt
│   │   ├── commodore-c64/
│   │   │   ├── mos-6567-vic-ii.pdf
│   │   │   ├── mos-6581-sid.pdf
│   │   │   ├── mos-8580-sid.pdf
│   │   │   ├── sid-filter-analysis.pdf
│   │   │   ├── c64-programmers-reference.pdf
│   │   │   └── vic-ii-exposed.pdf
│   │   ├── nintendo-nes/
│   │   │   ├── nesdev-wiki-snapshot/        — offline mirror
│   │   │   ├── 2c02-ppu-reference.pdf
│   │   │   ├── apu-reference.txt
│   │   │   ├── mapper-documentation/
│   │   │   └── fds-technical-reference.pdf
│   │   ├── commodore-amiga/
│   │   │   ├── amiga-hardware-reference-manual.pdf
│   │   │   ├── agnus-register-map.pdf
│   │   │   ├── paula-audio-dma.pdf
│   │   │   ├── aga-differences.txt
│   │   │   └── accelerator-bus-timing.pdf
│   │   └── acorn-bbc/
│   │       ├── bbc-micro-advanced-user-guide.pdf
│   │       ├── econet-specification.pdf
│   │       ├── mc68b54-adlc-datasheet.pdf
│   │       └── tube-protocol-specification.pdf
│   ├── chips/
│   │   ├── ay-3-8910-datasheet.pdf
│   │   ├── ym2149-datasheet.pdf
│   │   ├── wd1770-datasheet.pdf
│   │   ├── wd1793-datasheet.pdf
│   │   ├── mc6845-crtc-datasheet.pdf
│   │   ├── z80-pio-datasheet.pdf
│   │   ├── z80-sio-datasheet.pdf
│   │   ├── ne2000-datasheet.pdf
│   │   └── cs8900a-datasheet.pdf
│   ├── formats/
│   │   ├── tzx-specification.txt
│   │   ├── pzx-specification.txt
│   │   ├── tap-format.txt
│   │   ├── csw-specification.txt
│   │   ├── z80-snapshot-format.txt
│   │   ├── sna-format.txt
│   │   ├── szx-specification.txt
│   │   ├── ines-header-format.txt
│   │   ├── cue-sheet-syntax.txt
│   │   └── chd-format-specification.txt
│   ├── community/
│   │   ├── fuse-test-suite/                — test vectors
│   │   ├── blargg-test-roms/               — NES test ROMs + docs
│   │   ├── tom-harte-processor-tests/      — 6502/Z80 test vectors
│   │   ├── lorenz-test-suite/              — C64 test suite
│   │   └── nesdev-wiki-snapshot/           — offline wiki mirror
│   └── analysis/
│       ├── spectrum-ula-die-photo-analysis.pdf
│       ├── sid-6581-die-analysis.pdf
│       ├── vic-ii-die-analysis.pdf
│       └── ppu-2c02-die-analysis.pdf
```

### 31.3 Reference manifest

The manifest catalogues every reference with metadata:

```toml
# refs/manifest.toml

[[reference]]
id = "z80-user-manual"
title = "Z80 CPU User Manual"
author = "Zilog"
year = 2016
path = "cpu/z80/z80-cpu-user-manual.pdf"
source_url = "https://www.zilog.com/docs/z80/um0080.pdf"
systems = ["all-z80"]
topics = ["instruction-set", "timing", "interrupts", "bus-cycles"]
notes = "Official Zilog manual. Revision 11. Some undocumented behaviour not covered."

[[reference]]
id = "spectrum-ula-book"
title = "The ZX Spectrum ULA: How to Design a Microcomputer"
author = "Chris Smith"
year = 2010
path = "systems/sinclair-spectrum/spectrum-ula-book.pdf"
isbn = "978-0-9565071-0-5"
systems = ["sinclair-spectrum"]
topics = ["ula", "contention", "video-timing", "memory-contention", "io-contention"]
notes = "Definitive ULA reference. Based on die photography. The primary source for contention timing."

[[reference]]
id = "sid-6581-datasheet"
title = "MOS 6581 Sound Interface Device (SID) Datasheet"
author = "MOS Technology"
year = 1982
path = "systems/commodore-c64/mos-6581-sid.pdf"
systems = ["commodore-c64"]
topics = ["sid", "audio", "filter", "envelope", "waveform"]
notes = "Original datasheet. Filter specifications are nominal — real chips vary significantly."

[[reference]]
id = "nesdev-wiki"
title = "NESDev Wiki (offline snapshot)"
author = "NESDev Community"
year = 2025
path = "community/nesdev-wiki-snapshot/"
source_url = "https://www.nesdev.org/knowledge/"
systems = ["nintendo-nes"]
topics = ["ppu", "apu", "mappers", "timing", "bus-conflicts"]
notes = "Community-maintained. Snapshot date in directory. The most comprehensive NES reference."
```

### 31.4 Code citations

When implementing hardware behaviour, cite the source in code comments:

```rust
// The ULA contends memory access when the CPU reads from the
// contested memory region ($4000-$7FFF) during active display.
// The contention pattern repeats every 8 T-states within each
// scanline, with delays of 6,5,4,3,2,1,0,0 T-states.
//
// Ref: spectrum-ula-book, Chapter 7 "Memory Contention", pp. 147-162
// Ref: spectrum-contention-timing (Ramsoft technical note)
fn apply_contention(&self, t_state: u32) -> u32 {
    // ...
}

// The SID's combined waveform output when multiple waveform bits
// are set simultaneously differs between 6581 and 8580.
// On the 6581, combined waveforms are generated by ANDing the
// waveform outputs together, producing characteristic "thin" sounds.
// On the 8580, the combination logic was changed, producing different
// (generally louder) results.
//
// Ref: sid-6581-datasheet, Section 5 "Waveform Generation"
// Ref: sid-6581-die-analysis, Section 3.2 "Waveform Selector"
fn combined_waveform(&self, waveform_bits: u8) -> u16 {
    match self.model {
        SidModel::Mos6581 => { /* ... */ }
        SidModel::Mos8580 => { /* ... */ }
    }
}
```

The citation format is: `Ref: {manifest-id}, {location-within-document}`

This means anyone reading the code can look up the manifest entry, find the PDF, and go to the cited page to verify the implementation.

### 31.5 Claude Code integration

Claude Code accesses references through the project's file system. The workflow:

1. **Before implementing a feature**, Claude Code reads the relevant reference material from `refs/`. The manifest helps identify which references are relevant for a given system/topic.

2. **During implementation**, Claude Code cites references in code comments using the manifest ID format.

3. **When a reference is needed but not cached**, Claude Code notes the gap. Missing references are tracked in the manifest with `cached = false` and a source URL where available, so they can be acquired.

```toml
[[reference]]
id = "amiga-hw-reference"
title = "Amiga Hardware Reference Manual"
author = "Commodore"
year = 1991
path = "systems/commodore-amiga/amiga-hardware-reference-manual.pdf"
source_url = "https://archive.org/details/..."
cached = false  # not yet downloaded
systems = ["commodore-amiga"]
topics = ["custom-chips", "dma", "copper", "blitter", "audio"]
notes = "Need to acquire. Essential for Amiga chipset implementation."
```

### 31.6 Reference acquisition

References come from several sources:

| Source | What it provides | Legal status |
|--------|-----------------|-------------|
| Manufacturer datasheets | Chip specifications, timing diagrams | Typically freely available |
| Archive.org | Out-of-print technical manuals | Varies — lending library model |
| NESDev Wiki / similar | Community documentation | CC-licensed or public domain |
| Published books | In-depth analysis (Chris Smith ULA book, etc.) | Purchased, not redistributable |
| Die photography analyses | Transistor-level behaviour documentation | Typically freely published |
| Community test results | Empirical hardware measurements | Typically freely shared |
| Format specifications | TZX spec, iNES spec, etc. | Typically freely available |

**Important:** copyrighted books (Chris Smith's ULA book, Commodore reference manuals) are reference material for development, not for redistribution. The `refs/` directory is not committed to the public repository. The manifest is committed (it's just metadata), but the actual PDFs are `.gitignore`d. Each developer acquires their own copies.

The manifest serves as a reading list and acquisition guide even when the files aren't present.

### 31.7 Offline wiki snapshots

Community wikis (NESDev, Spectrum Computing, C64 Wiki) are invaluable but can change or disappear. Periodic offline snapshots ensure the reference material remains available:

```bash
# Example: snapshot NESDev wiki
wget --mirror --convert-links --page-requisites \
  --no-parent https://www.nesdev.org/knowledge/ \
  -P refs/community/nesdev-wiki-snapshot/
```

Snapshots are dated in the manifest so it's clear how current they are.

### 31.8 Reference-driven development workflow

The ideal development flow for a new system or feature:

1. **Gather references** — identify and acquire datasheets, manuals, community docs. Add to manifest.
2. **Read before coding** — Claude Code reads the relevant references before writing any implementation code.
3. **Cite while coding** — every hardware-behaviour decision gets a `Ref:` comment.
4. **Validate against tests** — CPU test suites and hardware test results verify the implementation matches the documented behaviour.
5. **Document gaps** — where references are ambiguous, contradictory, or missing, note it in the code and the manifest. "Behaviour at this edge case is undocumented. Current implementation based on empirical testing with real hardware / other emulator comparison."

### 31.9 Handling contradictory references

References sometimes disagree. The priority order:

1. **Die photography / transistor-level analysis** — this is what the silicon actually does
2. **Empirical hardware testing** — measurements on real chips
3. **Manufacturer datasheets** — sometimes contain errors or omit undocumented behaviour
4. **Community documentation** — usually accurate but occasionally based on emulator behaviour rather than hardware
5. **Other emulators' source code** — useful as a cross-reference but never a primary source; they may have the same bug

When references conflict, cite all of them and document the decision:

```rust
// The Z80's MEMPTR (internal WZ register) is updated on BIT n,(HL)
// to the value of HL + 1, not HL.
//
// Ref: z80-undocumented, Section 4.1 — says MEMPTR = HL
// Ref: z80-memptr-investigation (Boo-Hoo/Ets) — says MEMPTR = HL + 1
// Ref: real-hardware-test-results — confirms HL + 1
//
// Decision: using HL + 1 based on hardware testing. The undocumented
// doc appears to have this wrong.
```

---

## 32. System variants, extensions, and modern recreations

### 24.1 Scope

Every 8-bit and 16-bit platform is potentially in scope, including all model variants, regional variants, period hardware extensions, and modern backward-compatible recreations.

### 24.2 Variant categories

**Model variants** — different revisions of the same system sharing most hardware. These should be configurations of one machine implementation, not separate codebases.

**Regional variants** — same hardware, different crystal, different video encoding (PAL/NTSC/SECAM). Affect clock tree, frame timing, and sometimes colour palette.

**Period hardware extensions** — add-ons available during the system's commercial life. Must be composable — a user might run a 128K Spectrum with Interface 1, Multiface, and DivMMC simultaneously.

**Modern recreations** — new systems backward-compatible with originals but adding significant hardware. The Spectrum Next, Mega65, Commander X16.

### 24.3 Configuration-driven machine construction

Variants are configurations, not separate machines. The machine builds itself from a config:

```rust
pub struct MachineConfig {
    pub system: SystemId,
    pub variant: VariantId,
    pub region: Region,
    pub extensions: Vec<ExtensionId>,
    pub rom_set: RomSetId,
    pub hardware_config: HardwareConfig,
}

pub enum Region {
    Pal,
    Ntsc,
    Secam,
    Custom { crystal_hz: u64, video_standard: VideoStandard },
}

pub struct HardwareConfig {
    /// Selectable CPU speed (e.g., Next: 3.5/7/14/28 MHz).
    pub cpu_speeds: Vec<u64>,
    /// Default CPU speed.
    pub default_cpu_speed: u64,
    /// Chip variants where acoustically/behaviourally significant.
    pub chip_variants: Vec<ChipVariant>,
    /// Memory size (where configurable).
    pub memory_size: Option<usize>,
}
```

### 24.4 Variant scale

The number of variants per family is larger than it first appears:

**Spectrum family (~15+ variants):** 16K, 48K (Issue 2/3/4 — different ULAs), 128K (Toastrack), +2 (Amstrad grey), +2A/+2B (different ULA and paging), +3/+3B (floppy), TC2048 (Timex), TS2068 (Timex Sinclair, different video modes), TK90X (Brazilian clone), Pentagon 128/512/1024 (Russian, different timing), Scorpion (Russian, different memory).

**C64 family (~10+ variants):** C64 breadbin (board rev 250407/250425/250466), C64C (cost-reduced, 8580 SID), SX-64 (portable), C128 (Z80 + VDC), C128D, C64 GS (cartridge-only), MAX Machine / Ultimax, Plus/4 (TED chip).

**NES family (~8+ variants):** NES NTSC (front-loader), NES-101 (top-loader), Famicom, Famicom AV, Sharp Twin Famicom (FDS built in), VS System (arcade, different palettes, coin-op), PlayChoice-10 (arcade, dual-screen), Dendy (Russian, NTSC-on-PAL timing).

**Amiga family (~12+ variants):** A1000, A500, A500+, A600, A1200, A1500, A2000, A3000, A4000, A4000T, CDTV, CD32. Three chipset generations (OCS/ECS/AGA).

**Other families:** BBC Micro (Model A/B/B+/Master 128/Master Compact/Electron), Atari 8-bit (400/800/1200XL/800XL/65XE/130XE), MSX (MSX1/MSX2/MSX2+/MSX turboR), CPC (464/664/6128/Plus), Game Boy (DMG/Pocket/Color/Advance), Mega Drive (Model 1/2/3/Nomad/CDX/32X).

### 24.5 Extension composition

Extensions plug into the machine through a composable interface:

```rust
pub trait HardwareExtension: Send + Sync {
    fn id(&self) -> ExtensionId;
    fn requirements(&self) -> ExtensionRequirements;
    fn compatible_with(&self, other: ExtensionId) -> bool;
    fn attach(&mut self, bus: &mut dyn MachineBus);
    fn detach(&mut self, bus: &mut dyn MachineBus);
    fn tick(&mut self, cycles: u64);
    fn display_geometry_override(&self) -> Option<DisplayGeometry> { None }
    fn debug_views(&self) -> Option<&dyn DebugViews> { None }
}

pub struct ExtensionRequirements {
    pub memory_regions: Vec<MemoryRegion>,
    pub io_ports: Vec<IoPortRange>,
    pub interrupts: Vec<InterruptLine>,
    pub bus_signals: Vec<BusSignal>,
    pub slots: Vec<SlotId>,
}
```

### 24.6 Bus interception

Some extensions intercept bus activity rather than just occupying I/O ports. DivMMC pages its ROM when the CPU fetches from specific addresses ($0000, $0008, $0038, $0066, $04C6, $0562). Interface 1 shadow-pages on similar triggers.

This is distinct from `BusObserver` (passive observation). Interceptors are active — they can redirect memory access:

```rust
pub trait BusInterceptor: Send + Sync {
    /// Called before every memory read. Returns Some(value) to override.
    fn intercept_read(&mut self, addr: u16, is_opcode_fetch: bool)
        -> Option<u8>;
    /// Called before every memory write. Returns true if handled.
    fn intercept_write(&mut self, addr: u16, value: u8) -> bool;
}
```

Performance cost: one extra check per memory access per active interceptor. Manageable because most configurations have 0-2 active interceptors.

### 24.7 Notable extension catalogue

**Spectrum:**

| Extension | What it adds |
|-----------|-------------|
| Interface 1 | Microdrive, RS-232, ZX Net, shadow ROM paging |
| Interface 2 | Cartridge port, joystick ports |
| Kempston joystick | Joystick via port $1F |
| Fuller Box | AY sound + joystick |
| AY board (48K) | AY-3-8912 at 128K-compatible ports |
| Multiface | NMI button, 8KB RAM, snapshot/cheat |
| DivMMC / DivIDE | SD card/IDE, auto-paging ROM, 8-64KB RAM |
| SpecDrum | DAC for digital audio |
| Currah µSpeech | Speech synthesis |

**C64:**

| Extension | What it adds |
|-----------|-------------|
| REU (1764/1700) | DMA engine, up to 16MB expansion RAM |
| SuperCPU | 65816 at 20MHz, up to 16MB RAM |
| Action Replay / Final Cartridge | Cartridge ROM, freeze button, I/O |
| SwiftLink | 6551 ACIA for RS-232 |
| 1541 Ultimate | SD card + REU + Ethernet |

**Amiga:**

| Extension | What it adds |
|-----------|-------------|
| Accelerator cards | 68020/030/040/060 CPU, fast RAM, sometimes SCSI |
| Ethernet (Ariadne, X-Surf) | NE2000 at Zorro address |
| Graphics cards (Picasso, CyberVision) | Framebuffer at Zorro address |
| IDE controllers | IDE at Zorro address |
| PCMCIA (A600/A1200) | CF card / network adapters |

**NES (via cartridge mappers):**

| Extension | What it adds |
|-----------|-------------|
| VRC6 | 2 pulse + 1 sawtooth audio channels |
| VRC7 | 6 FM synthesis audio channels |
| N163 | Up to 8 wavetable audio channels |
| MMC5 | 2 pulse + PCM audio, extra nametable RAM |
| Sunsoft 5B | 3 channels (AY-3-8910 compatible) |
| FDS | Wavetable audio + disk system (see §16) |

NES expansion audio is a special case — it's part of the cartridge mapper, so it's already in `machine-*`. But it affects the audio pipeline (additional channels need to mix with the base APU output) and the audio inspector (additional channels in the per-channel view).

### 24.8 Chip variants with behavioural significance

Some chip revisions are not just "different revision number" — they produce different observable output that software was designed around.

| Chip | Variants | Difference |
|------|----------|------------|
| SID | MOS 6581 vs MOS 8580 | Completely different filter response curves. Music composed for one sounds wrong on the other. Combined waveform output differs. |
| ULA | Spectrum Issue 2 vs Issue 3 | Ear/mic port interaction differs. Some tape loaders are Issue-specific. |
| VIC-II | 6567 (NTSC) vs 6569 (PAL) vs 8562/8565 | Luminance levels differ between revisions. |
| AY | AY-3-8910 vs YM2149 | DAC output curves differ (linear vs logarithmic). Envelope behaviour may differ. |
| PPU | RP2C02 (NTSC) vs RP2C07 (PAL) | Different palette, timing, frame length. |

The machine config must specify chip variant when it matters:

```rust
pub enum ChipVariant {
    Sid(SidModel),       // Mos6581 or Mos8580
    Ula(UlaVariant),     // Issue2, Issue3, Plus2A, etc.
    VicII(VicIIModel),   // 6567, 6569, 8562, 8565
    Ay(AyModel),         // Ay38910 or Ym2149
    Ppu(PpuModel),       // Rp2c02 or Rp2c07
}
```

### 24.9 Modern recreations

| Platform | Classification | Rationale |
|----------|---------------|-----------|
| Spectrum Next | Variant with extensive extensions | Same Z80, backward compatible |
| SAM Coupé | Variant (borderline) | Same Z80, different video chip, compatible in a mode |
| Mega65 | New system | 45GS02 ≠ 6502, VIC-IV ≠ VIC-II |
| Commander X16 | New system | Completely new design |
| TheC64/Mini | Not relevant | Linux box running an emulator |
| MiSTer / Analogue | Not relevant | FPGA recreations of original hardware |
| C128 | Variant (borderline) | Contains a complete C64; press a button, becomes a C64 |

**The Spectrum Next specifically** is modelled as a Spectrum variant with a large extension set:

```rust
MachineConfig {
    system: SystemId::Spectrum,
    variant: VariantId::Next,
    region: Region::Pal,
    extensions: vec![
        ExtensionId::NextEnhancedUla,    // 256 colours, hardware scroll
        ExtensionId::NextTilemap,        // 40×32 or 80×32 tiles
        ExtensionId::NextSprites,        // 64 hardware sprites, 16×16
        ExtensionId::NextCopper,         // raster-chasing co-processor
        ExtensionId::NextDma,            // Z80 DMA compatible + enhanced
        ExtensionId::NextTripleAy,       // 3× AY-3-8910
        ExtensionId::NextSdCard,         // SD via esxDOS/NextZXOS
        ExtensionId::Next2MbRam,         // 2MB RAM with MMU
    ],
    rom_set: RomSetId::NextZxOs,
    hardware_config: HardwareConfig {
        cpu_speeds: vec![3_500_000, 7_000_000, 14_000_000, 28_000_000],
        default_cpu_speed: 3_500_000,
        // ...
    },
}
```

### 24.10 Runtime clock speed changes

Some systems and extensions allow CPU speed changes at runtime (Next selectable speeds, Amiga accelerators, SuperCPU). The clock tree must support this:

```rust
impl ClockTree {
    pub fn set_cpu_divisor(&mut self, new_divisor: u32) {
        self.components.get_mut("cpu").unwrap().divisor = new_divisor;
        self.scheduler.reschedule_component(ComponentId::Cpu);
    }
}
```

At 28MHz on the Next, the Z80 runs at the master oscillator frequency — no division. The ULA still runs at its normal rate. Contention behaviour changes proportionally.

### 24.11 ROM management

Each variant needs specific ROMs. Some variants have multiple ROM versions. The ROM management system:

- identifies ROMs by hash, not filename
- knows which ROMs each variant requires
- supports multiple ROM versions per variant (Issue 2 vs Issue 3, Spanish vs English)
- reports mismatched or missing ROMs
- supports patched/modified ROMs for preservation

```rust
pub struct RomRequirement {
    pub name: String,
    pub size: usize,
    pub known_hashes: Vec<RomHash>,
    pub slot: RomSlot,
    pub required: bool,
}

pub struct RomHash {
    pub hash: [u8; 32],    // SHA-256
    pub version: String,    // "Issue 2", "Spanish", "Kickstart 3.1"
    pub notes: String,
}
```

### 24.12 Variant / extension boundary rules

The line between "variant" and "new system" is: does it share the same CPU core (or a backward-compatible one) and the same base architecture? If yes, it's a variant. If the CPU is fundamentally different or the architecture shares nothing with the original, it's a new system.

An extension is any hardware that can be added to or removed from a base system configuration. If the hardware is always present in a specific variant, it's part of the variant definition, not a separate extension — but it may still be implemented as an extension internally for code reuse.

**Rule: a Spectrum 128K is a 48K + {128K memory + AY + 128K paging + 128K ROM}. If those additions are implemented as extensions internally, a 128K config is just a 48K config with those extensions pre-attached. This maximises code sharing.**

### 24.13 Machine capability declaration

Each variant declares its capabilities so the UI, MCP, debug views, and tooling can adapt without system-specific code. A Spectrum 48K doesn't have AY sound — the audio channel viewer shouldn't show AY channels. A stock A500 doesn't have a hard disk — the media browser shouldn't offer hard disk mounting.

```rust
pub struct MachineCapabilities {
    pub system_family: SystemFamily,
    pub variant: VariantId,
    pub region: Region,

    /// CPU description.
    pub cpu_type: String,
    /// Available clock speeds in Hz (for systems with selectable speeds).
    pub cpu_speeds: Vec<u64>,
    pub current_cpu_speed: u64,

    /// Total RAM in bytes.
    pub ram_bytes: u32,
    pub has_bank_switching: bool,

    /// Audio channels available in current configuration.
    pub audio_channels: Vec<AudioChannelDescriptor>,

    /// Video modes supported by current hardware config.
    pub video_modes: Vec<VideoModeDescriptor>,

    /// Input ports (keyboard, joystick, mouse, paddle, light gun).
    pub input_ports: Vec<InputPortDescriptor>,

    /// Media slots (cassette, floppy, optical, SD card, cartridge).
    pub media_slots: Vec<MediaSlotDescriptor>,

    /// Peripheral ports (serial, parallel, IEC bus, Tube, etc.)
    pub peripheral_ports: Vec<PeripheralPortDescriptor>,

    /// Network interfaces.
    pub network_interfaces: Vec<NetworkInterfaceDescriptor>,

    /// Currently attached extensions.
    pub active_extensions: Vec<ExtensionDescriptor>,

    /// Extensions available for this variant.
    pub available_extensions: Vec<ExtensionDescriptor>,

    /// Hardware features.
    pub has_rtc: bool,
    pub has_fpu: bool,
    pub has_mmu: bool,
    pub has_dma: bool,
    pub has_hardware_sprites: bool,
}

pub struct AudioChannelDescriptor {
    pub name: String,               // "Beeper", "AY Channel A", "SID Voice 1"
    pub chip: String,               // "ULA", "AY-3-8912", "MOS 6581"
    pub channel_type: String,       // "Square", "PSG", "Wavetable"
}

pub struct VideoModeDescriptor {
    pub name: String,               // "Standard", "Layer 2", "Hires"
    pub resolution: (u32, u32),
    pub colours: u32,
    pub description: String,
}

pub struct InputPortDescriptor {
    pub name: String,               // "Keyboard", "Joystick Port 1", "Mouse Port"
    pub port_type: InputPortType,
    pub active: bool,
}

pub enum InputPortType {
    Keyboard,
    JoystickDigital,
    JoystickAnalogue,
    Mouse,
    Paddle,
    LightGun,
}

pub struct MediaSlotDescriptor {
    pub name: String,               // "Cassette", "Drive A:", "SD Card", "Cartridge Slot"
    pub media_type: MediaType,
    pub writable: bool,
    pub media_present: bool,
}

pub struct PeripheralPortDescriptor {
    pub name: String,               // "Printer Port", "RS-232", "Tube", "IEC Bus"
    pub port_type: PortType,
    pub device_attached: Option<String>,
}

pub struct NetworkInterfaceDescriptor {
    pub name: String,               // "Econet", "Ethernet (Ariadne)", "Modem"
    pub interface_type: String,
    pub connected: bool,
}

pub struct ExtensionDescriptor {
    pub id: ExtensionId,
    pub name: String,               // "DivMMC", "REU 1764", "Blizzard 1260"
    pub category: ExtensionCategory,
    pub description: String,
    pub attached: bool,
}

pub enum ExtensionCategory {
    Memory,         // RAM expansions, REU
    Storage,        // DivMMC, IDE, SD card interfaces
    Audio,          // AY board, SpecDrum, expansion audio
    Video,          // Graphics cards, enhanced ULA
    Accelerator,    // CPU upgrades, fast RAM
    IO,             // Joystick interfaces, MIDI, serial cards
    Network,        // Ethernet, WiFi
    Coprocessor,    // BBC Tube second processors
    Cartridge,      // Action Replay, Final Cartridge
    Composite,      // Multi-function: 1541 Ultimate, Interface 1
}
```

The capability declaration is queried by:

- **Debug views** — which views to offer (no tile viewer for Spectrum, no bitplane viewer for NES)
- **Audio inspector** — which channels exist (3 AY channels on 128K, none on 48K, 9 on Next with triple AY)
- **Media browser** — which media slots exist (no floppy on 48K, floppy on +3)
- **Input mapper** — which input ports exist (Kempston only if attached, SNES pad on SNES)
- **Peripheral panel** — which ports are available and what's connected
- **MCP server** — which tools to expose and what queries make sense
- **Variant selector UI** — list available variants and extensions with descriptions

This is how adding a new system or variant becomes tractable — the infrastructure adapts to declared capabilities without system-specific code in the shell.

---

## 33. Generalisation

### 25.1 What is shared vs system-specific

| Layer | Shared | System-specific |
|-------|--------|-----------------|
| Clock tree / scheduler | Model, scheduler | Clock frequencies, divisors, phase |
| Machine configuration | `MachineConfig`, `HardwareExtension` trait, `BusInterceptor` | Variant definitions, extension implementations |
| Observation layer | Traits, flags, ring buffers, MCP interface | Signal definitions, component inspectors |
| Input mapping | InputManager, mapping engine, profiles | Keyboard layout, joystick port interface |
| Tape timeline | Trait + PulseTimeline + SampledTimeline | Motor control, input path |
| Disk image | Trait + flux model | Controller, DMA |
| Optical disc | Trait + sector/subchannel | Controller, seek model |
| Cartridge | Trait | All mapper implementations |
| Persistence | Trait + auto-save | Controller protocols |
| Serial/parallel ports | Port traits, device attachment | UART/PIO chip implementation |
| Peripherals | Printer rendering (ESC/P), modem AT commands | ZX Printer, Commodore IEC, system-specific protocols |
| Networking | NetworkInterface trait, backends, user-mode NAT | NIC chip emulation, Econet ADLC |
| ROM management | Hash-based identification, requirement checking | Per-variant ROM requirements and slot mappings |
| Identification | Sniffer, databases, archives | System-specific disambiguation |

### 25.2 Rule

**Generalise storage, transport, identification, observation, input mapping, peripheral attachment, and extension composition. Specialise interpretation, controller behaviour, electrical truth, keyboard layout, chip-level protocol implementation, and variant-specific wiring.**

---

## 34. Crate strategy

### 33.1 Naming families

| Prefix | Type | Responsibility |
|--------|------|---------------|
| `cpu-*` | Library | CPU cores with variant-gated feature flags, assemblers, disassemblers |
| `machine-{mfr}-{system}` | Library | Hardware models, wiring, variant construction, extension implementations, BASIC tokenisers. Named with manufacturer prefix for unambiguous identification. Multiple crates per system are acceptable (e.g., `machine-sinclair-spectrum-ula`, `machine-sinclair-spectrum-next`, `machine-commodore-amiga-chipset`). |
| `machine-{mfr}-{system}-views` | Library | System-specific debug views, signal processing, asset export |
| `{type}-{mfr}-{system}-*` | Library | System-specific subsystems that benefit from isolation (e.g., `video-sinclair-spectrum-ula`, `audio-commodore-c64-sid`). Use when a subsystem is large enough that keeping it inside the parent `machine-*` crate would harm clarity. |
| `emu-*` | Library | Runtime, tooling, media, observation, display, capture, IDE (library crates only) |
| `emu198x-{mfr}-{system}` | Binary | System binaries (composition roots). E.g., `emu198x-sinclair-spectrum`, `emu198x-nintendo-nes`. |
| `emu198x-*` | Binary | Non-system binaries: `emu198x-mcp`, `emu198x-regression`, `emu198x-tools` |
| `format-*` | Library | External artifact models |
| `parser-*` | Library | Encoding/decoding |
| `shell-*` | Library | Platform-specific window management and rendering |
| `{manufacturer}-{chip}` | Library | Hardware-identity crates |

**Manufacturer naming examples:**

| Manufacturer | System | Crate name |
|-------------|--------|------------|
| Sinclair | ZX Spectrum | `machine-sinclair-spectrum` |
| Sinclair | ZX81 | `machine-sinclair-zx81` |
| Commodore | C64 | `machine-commodore-c64` |
| Commodore | Amiga | `machine-commodore-amiga` |
| Commodore | VIC-20 | `machine-commodore-vic20` |
| Commodore | Plus/4 | `machine-commodore-plus4` |
| Nintendo | NES / Famicom | `machine-nintendo-nes` |
| Nintendo | SNES | `machine-nintendo-snes` |
| Nintendo | Game Boy | `machine-nintendo-gameboy` |
| Sega | Master System | `machine-sega-mastersystem` |
| Sega | Mega Drive | `machine-sega-megadrive` |
| Sega | Saturn | `machine-sega-saturn` |
| Sony | PlayStation | `machine-sony-playstation` |
| Acorn | BBC Micro | `machine-acorn-bbc` |
| Acorn | Electron | `machine-acorn-electron` |
| Acorn | Archimedes | `machine-acorn-archimedes` |
| Atari | 2600 | `machine-atari-2600` |
| Atari | 800 / XL / XE | `machine-atari-8bit` |
| Atari | ST | `machine-atari-st` |
| Atari | Lynx | `machine-atari-lynx` |
| Amstrad | CPC | `machine-amstrad-cpc` |
| MSX | MSX | `machine-msx` (multi-manufacturer standard) |
| Apple | Apple II | `machine-apple-ii` |
| NEC | PC Engine | `machine-nec-pcengine` |
| SNK | Neo Geo | `machine-snk-neogeo` |
| Coleco | ColecoVision | `machine-coleco-colecovision` |
| — | Commander X16 | `machine-commanderx16` (community project, no single manufacturer) |

### 33.2 Full crate structure

```
Library crates — emu-* (runtime infrastructure):

emu-machine         — MachineConfig, HardwareExtension trait, BusInterceptor trait,
                      MachineCapabilities, ROM management, variant registry
emu-observe         — core traits (BusObserver, inspectors, event types)
emu-debug           — breakpoint engine, stepping, trace management, symbol table,
                      symbol file import
emu-debug-views     — shared DebugViewOutput types, ViewRow, HitTarget, memory editor
emu-audio           — AudioMixer, AudioSource trait, latency management, speed interaction
emu-input           — InputManager, keyboard/joystick mapping, profile management
emu-display         — display pipeline: PAR, CRT parameters, speed control, shader interface
emu-capture         — screenshot, video, GIF, audio recording pipeline
emu-export          — asset export traits and formats (PNG, WAV, palette files)
emu-ide             — IdeProject, source editor widget, BASIC editor panel, build workflow
emu-rewind          — RewindBuffer, snapshot compression, delta encoding, time travel
emu-config          — configuration hierarchy, TOML persistence, ROM index management
emu-peripheral      — printer rendering (ESC/P, ZX Printer), port traits, peripheral devices
emu-network         — NetworkBackend, user-mode networking, Econet hub, modem emulator
emu-mcp             — MCP tool definitions, query dispatch logic (library)
emu-regression      — baseline comparison, harness logic, TOSEC corpus management (library)

Library crates — shell:

shell-native        — multi-window panel manager (winit), native window chrome,
                      egui rendering for tool panels, wgpu CRT shader for main display
shell-wasm          — browser-based single-window shell, WebGL2 CRT shader

Library crates — CPU:

cpu-z80             — Z80/Z80N core, disassembler, assembler
cpu-6502            — 6502/65C02/65C816 core, disassembler, assembler
cpu-m68k            — 68000-68060 core, disassembler, assembler

Library crates — machine:

machine-sinclair-spectrum        — Spectrum system wiring, variants, extensions, Sinclair BASIC tokeniser
machine-sinclair-spectrum-ula    — ULA variants (Issue 2/3/+2A/Next enhanced ULA) — if large enough
machine-sinclair-spectrum-next   — Next-specific extensions (tilemap, sprites, copper, DMA, triple AY)
machine-nintendo-nes             — NES/Famicom wiring, variants, mapper registry
machine-nintendo-nes-fds         — FDS RAM adapter, drive mechanism, wavetable audio
machine-commodore-c64             — C64/C128 wiring, variants, CBM BASIC tokeniser
machine-commodore-c64-sid         — SID 6581/8580 emulation — complex enough to warrant isolation
machine-commodore-amiga           — Amiga system wiring, variants, accelerator support
machine-commodore-amiga-chipset   — OCS/ECS/AGA custom chips (Agnus, Denise, Paula)
machine-acorn-bbc             — BBC Micro wiring, variants, Tube interface, BBC BASIC tokeniser

machine-sinclair-spectrum-views  — Spectrum debug views, screen/attribute/ULA/Next layer views
machine-nintendo-nes-views       — NES debug views, NTSC composite decode, asset exporter
machine-commodore-c64-views       — C64 debug views, composite luma/chroma decode
machine-commodore-amiga-views     — Amiga debug views, bitplane/copper/blitter/DMA views

Binary crates — emu198x-* prefix:

emu198x-sinclair-spectrum    — Spectrum system binary (composition root)
emu198x-nintendo-nes         — NES system binary
emu198x-commodore-c64         — C64 system binary
emu198x-commodore-amiga       — Amiga system binary
emu198x-acorn-bbc         — BBC Micro system binary
emu198x-mcp         — MCP server binary
emu198x-regression  — TOSEC regression harness binary
emu198x-tools       — Asset extraction, format conversion, ROM scanning tools
```

The decision to split a `machine-*` crate into sub-crates is driven by complexity. If the SID emulation is 5000 lines with its own filter model, envelope generator, and waveform tables, it earns its own crate (`machine-commodore-c64-sid` or `audio-commodore-c64-sid`). If the Spectrum ULA is 500 lines, it stays in `machine-sinclair-spectrum`. The split is always optional — a single `machine-*` crate per system is the starting point; sub-crates emerge when a subsystem grows large enough.

Dependency flow:

```
emu-machine  →  (standalone, defines MachineConfig, extension traits, ROM management)
machine-*  →  emu-machine + emu-observe + emu-input + emu-audio
machine-*-views  →  machine-* + emu-debug-views + emu-export
emu-debug  →  emu-observe (symbol table, breakpoints, stepping)
emu-audio  →  (standalone, AudioSource trait, mixer)
emu-input  →  (standalone, maps host events to machine input traits)
emu-display  →  (standalone, takes framebuffer + parameters)
emu-capture  →  emu-display + emu-export + emu-audio
emu-ide  →  emu-debug + emu-export (source editor, project model, BASIC editor)
emu-rewind  →  emu-machine
emu-config  →  emu-machine
emu-peripheral  →  (standalone, implements device traits)
emu-network  →  (standalone, implements network backends)
emu-mcp  →  emu-debug + emu-capture + emu-export + emu-input + emu-machine
            + emu-audio + emu-rewind + emu-ide (MCP tool dispatch library)
emu-regression  →  emu-observe + emu-capture + emu-machine (harness logic library)
shell-native  →  emu-debug-views + emu-display + emu-capture + emu-input
                + emu-machine + emu-audio + emu-rewind + emu-config + emu-ide
shell-wasm  →  same emu-* deps (single-window, WebGL2)
emu198x-*  →  shell-* + machine-* + machine-*-views + emu-config
              (composition roots that wire everything together)
emu198x-mcp  →  emu-mcp + machine-* (thin binary wrapper)
emu198x-regression  →  emu-regression + machine-* (thin binary wrapper)
```

`machine-*` builds itself from `MachineConfig` (provided by `emu-machine`). Shells query `MachineCapabilities` to adapt their UI without knowing system internals. Shells do not depend on any `machine-*` crate — the `emu198x-*` binary crates are the composition roots that wire machine, shell, and views together.

### 33.3 Boundary rules

- `emu-*` crates are always libraries; `emu198x-*` crates are always binaries
- `emu-machine` defines `MachineConfig`, extension traits, capability declarations — knows nothing about any specific system
- `machine-*` crates can be split into sub-crates when a subsystem (ULA, SID, chipset) is complex enough to warrant isolation; the parent `machine-*` crate re-exports what's needed
- `machine-*` builds itself from `MachineConfig`, implements `BusObserver` hooks, inspector traits, and `MachineCapabilities`
- `machine-*` does not know about breakpoints, MCP, regression, display pipeline, or capture
- `machine-*` implements extensions as `HardwareExtension` trait objects; variant-specific wiring stays internal
- `machine-*-views` knows hardware semantics, produces renderer-agnostic output
- `machine-*-views` does not know about shell rendering, CRT filters, or capture formats
- `cpu-*` crates expose feature flags for instruction set variants (Z80N, 68k ISA levels, 65C816 extensions); the machine config enables the right flags
- `emu-debug` orchestrates observation but doesn't know machine internals
- `emu-display` handles PAR/CRT/scaling but doesn't know machine internals
- `emu-capture` produces files but doesn't know machine internals
- `emu198x-*` binary crates are composition roots — they wire machine, shell, and views together; all system-specific dependency resolution happens here
- shell crates render `DebugViewOutput`, manage the display pipeline, and query `MachineCapabilities` to adapt their UI, but never interpret hardware state
- format/parser crates know nothing about observation, views, variants, or display

---

## 35. Implementation sequence

Each system should be independently releasable as a complete, polished product. The phasing below builds shared infrastructure first, then brings up systems one at a time. Each system release is a standalone launch — "emu198x for ZX Spectrum" is a shippable product, not a tech demo waiting for 99 more systems.

### Phase 1 — Foundation: clock tree, scheduler, error handling, references

- `ClockFrequency`, `ClockDivisor`, `ClockTree`
- Next-event `Scheduler`
- Error handling types (`EmulatorError`, `MediaError`, `RomError`, `ConfigError`)
- `DiagnosticError` trait with summary/detail/suggestion
- Logging infrastructure (`tracing` crate integration)
- Reference management: `refs/` directory structure, `manifest.toml` schema, `.gitignore` for PDFs
- Acquire and catalogue initial references for Spectrum (Z80 manual, ULA book, contention docs)
- Validate clock tree with Spectrum 48K: 14MHz master, ULA at ÷2, CPU at ÷4

### Phase 2 — Observation foundation

- `BusObserver` trait with no-op defaults
- `ObservationFlags` category bitflags
- `CpuTraceEntry` and ring-buffer trace
- `EmulatorEvent` log
- Wire into existing machine bus paths

### Phase 3 — Audio output pipeline

- `AudioSource` trait
- `AudioMixer` with source registration, per-source volume/mute/solo
- Audio backend integration (cpal)
- Latency management (configurable buffer size)
- Per-channel capture ring buffers (feeds visualisation and export)
- Speed control interaction (pitch-shifted, muted, time-stretched)

### Phase 4 — Canonical tape types and runtime

- `TapeTimeline` trait
- `PulseTimeline` and `SampledTimeline`
- `TapeTransport` with counter
- `TapeInputPath` and `TapeAudioRenderer`
- Tape audio as `AudioSource` registered with mixer

### Phase 5 — Tape importers

1. TAP
2. TZX
3. WAV
4. PZX
5. CSW

### Phase 6 — Debugger core

- Breakpoint engine (execution, memory, I/O, cycle, video position)
- Stepping modes (instruction, master cycle, scanline, frame, step out)
- Component inspectors (CPU, memory, video, audio)
- Trace export (binary, text, JSON)

### Phase 7 — Debug views, display pipeline, and shell

- `DebugViewOutput` types, `ViewRow`, `HitTarget`, `HitRegion`
- `DebugViews` trait
- First system-specific views crate (`machine-sinclair-spectrum-views`)
- Memory editor with peek/poke
- `DisplayGeometry` and PAR correction
- Phosphor blur, CRT shader (wgpu for native, WebGL2 for WASM)
- CRT parameter presets (Development, Clean RGB, PVM, Consumer, Composite, RF)
- Multi-window panel architecture (`shell-native` with `winit`)
- `PanelWindow` trait, `WindowManager`, layout persistence per system
- Main display panel with CRT shader and input capture
- First tool panels: disassembly, registers, memory editor

### Phase 8 — Capture and export pipeline

- Screenshot capture at all pipeline points (Raw, SignalProcessed, CrtProcessed, FinalOutput)
- Video recording via FFmpeg subprocess pipe
- GIF capture with palette quantisation
- Audio recording (master mix + per-channel, from mixer's capture buffers)
- Visual asset export (tile, sprite, tile bank, nametable → PNG)
- Audio asset export (per-channel WAV, register log JSON)
- Palette export (GPL, JSON)

### Phase 9 — MCP debug, capture, and export interface

- MCP tool definitions for all query/control/capture/export operations
- Scripted debugging via Claude Code
- Automated asset extraction pipelines
- Tape loading screech capture

### Phase 10 — Configuration and settings

- `emu-config` crate
- Configuration hierarchy (global → system → variant → session)
- TOML persistence for all config domains
- ROM index with hash-based scanning
- Input profile persistence
- Recent files and session restore

### Phase 11 — System variants and extensions

- `MachineConfig` with variant, region, extensions, ROM set
- `MachineCapabilities` declaration
- Configuration-driven Spectrum construction (48K → 128K → +2A → +3 from one codebase)
- `HardwareExtension` trait and `BusInterceptor`
- First extensions: Kempston joystick, AY board for 48K, Interface 1
- ROM management with hash-based identification
- Chip variant configuration (ULA Issue 2 vs Issue 3)

### Phase 12 — Snapshots, save states, and rewind

- Snapshot applicator pathway (Z80, SNA, SZX)
- Save state capture/restore including mounted device state and mixer state
- `RewindBuffer` with configurable compression (full, delta, keyframe+delta)
- Rewind playback with audio muting
- Memory budget management for large-state systems

### Phase 13 — ROM media and NES bring-up

- Acquire and catalogue NES references (PPU docs, NESDev wiki snapshot, mapper docs, APU reference)
- `CartridgeMapper` trait and NES mapper registry
- Database-driven identification (No-Intro)
- Battery-backed SRAM persistence
- NES debug views (`machine-nintendo-nes-views`): pattern tables, OAM, nametables
- NES NTSC composite signal processor
- NES expansion audio sources registered with mixer

### Phase 14 — Floppy disk

- `DiskImage` trait (logical level)
- `FloppyDrive` mounted device
- ADF, DSK, TRD importers
- Writable disk persistence

### Phase 15 — Optical disc

- `OpticalDisc` trait
- CUE/BIN parser, CHD support
- `OpticalDrive` with seek model
- CD audio playback as `AudioSource`

### Phase 16 — Input system

- `emu-input` crate with InputManager and mapping engine
- `EmulatedKeyboard` trait with symbolic and positional mapping
- `KeyboardLayout` definitions for Spectrum, C64, NES
- Joystick/gamepad mapping with host device enumeration
- Input profiles (per-system default, user-customisable)
- Mouse capture for Amiga/ST mouse emulation

### Phase 17 — Peripheral devices

- Serial and parallel port traits
- ZX Printer emulation (first printer target — simple protocol, iconic)
- ESC/P printer interpreter (covers BBC Micro, Amiga, ST, MSX)
- Printer output rendering and PNG/PDF export
- Modem emulator with Hayes AT command interpreter
- Telnet bridge backend for BBS access

### Phase 18 — Networking

- `NetworkInterface` trait and backends
- User-mode networking (NAT, DHCP, DNS)
- Econet station emulation (ADLC chip) with virtual file server
- Ethernet card emulation (NE2000/CS8900A) for Amiga, C64, Apple II
- Peer-to-peer backend for multi-instance networking

### Phase 19 — Testing infrastructure

- CPU validation suite integration (FUSE, Blargg, Tom Harte, Klaus Dormann)
- Format parser fuzz targets (`cargo-fuzz`)
- Clock tree mathematical validation tests
- Multi-crate integration test harness
- TOSEC regression harness (framebuffer/audio hashing, baseline management)
- CI pipeline configuration (per-commit, nightly, release)

### Phase 20 — IDE and assembler

- Z80 assembler (in `cpu-z80`)
- 6502 assembler (in `cpu-6502`)
- Symbol table shared between assembler and debugger
- External symbol file import (PASMO, z88dk, cc65, vasm)
- Source editor panel with syntax highlighting and error markers
- Assemble → load → run → debug workflow
- Source-level debugging (step in source, not just disassembly)
- IDE project model (source files, build config)

### Phase 21 — BASIC support

- Sinclair BASIC tokeniser (in `machine-sinclair-spectrum`)
- CBM BASIC tokeniser (in `machine-commodore-c64`)
- BBC BASIC tokeniser (in `machine-acorn-bbc`)
- `.bas` text file loading pipeline (tokenise → inject → update system vars)
- BASIC editor panel with syntax highlighting, auto-number, renumber
- Detokenise-to-text export for preservation
- BASIC → media export (TAP block, PRG, etc.)

### Phase 22 — Remaining media and infrastructure

- Microdrive, FDS, memory cards
- Archive unwrapping (ZIP, 7z)
- Multi-disk bundles
- Tape writing, flux-level disk, RZX
- Additional system-specific views crates as systems are brought up
- Batch asset extraction for CL198x content pipelines

---

## 36. Design rules

### The strongest rule

**Parsers own syntax. Formats own file semantics. Emulator-domain crates own canonical media/runtime behaviour. Machines consume runtime devices, not source formats.**

### Supporting rules

- **Master oscillator, not CPU clock** — tick at the true crystal frequency; let component interleaving emerge from integer clock division
- **Trait, not enum** — canonical interfaces are uniform traits
- **Flatten at import** — source-format control flow resolved into seekable canonical form
- **One time unit per domain** — master cycles inside machines; nanoseconds at the boundary
- **Four media pathways** — transport, state artifact, ROM, persistent storage
- **Observation by default, cost by choice** — hooks always present, observation cost only when enabled
- **Views interpret, shells render** — system-specific view models produce renderer-agnostic output; shells never interpret hardware state
- **Capture at any pipeline stage** — screenshots, video, GIF, audio can tap into Raw, SignalProcessed, CrtProcessed, or FinalOutput
- **Assets are first-class exports** — tiles, sprites, palettes, audio channels are individually extractable, not just viewable
- **PAR is system-defined** — pixel aspect ratio is a property of the display geometry, not baked into the renderer or the framebuffer
- **CRT blur is the authentic default** — hard square pixels never existed on real hardware; the default preset applies light phosphor blur and PAR correction; Development mode is the explicit opt-in for raw pixels
- **Speed is decoupled from display** — GUI shells lock to host vsync; emulation rate is a configurable multiple of real speed; turbo mode auto-engages during loading
- **Headless captures at the right speed** — video and audio at real speed; screenshots, assets, and regression at turbo
- **Earn complexity** — crate splits and abstractions justified by real use cases
- **Variants are configurations, not codebases** — model, region, and extensions are parameters to one machine implementation; a 128K is a 48K with extensions pre-attached
- **Extensions compose** — hardware add-ons plug into the machine through a standard `HardwareExtension` trait with declared requirements; the machine core doesn't know about any specific extension
- **Interceptors are not observers** — bus interception (DivMMC auto-paging) is active and can redirect access; observation is passive; both use the same hook points but with different capabilities
- **Chip variants are explicit** — when a chip revision produces different observable output (6581 vs 8580 SID, Issue 2 vs Issue 3 ULA), the variant is a configuration parameter, not a build-time choice
- **Panels are native windows** — each debug/tool panel is an independent OS window with native chrome; the window manager saves layout per system variant; closing a panel doesn't affect the emulator
- **The IDE is panels, not a separate app** — source editor, assembler output, symbol table, BASIC editor are panels consuming the same observation layer as the debugger
- **Symbols bridge assembler and debugger** — the symbol table is shared infrastructure; assembler output feeds disassembly labels and breakpoint resolution; external symbol files from third-party tools are importable
- **BASIC programs load as text** — every BASIC-equipped system has a tokeniser that converts `.bas` text to the system's internal format and injects it into memory with correct system variable updates
- **Symbolic keyboard by default, positional for games** — input mapping converts host events to emulated matrix positions; both modes available, auto-detect where possible
- **Audio sources register, mixers compose** — every sound-producing component implements `AudioSource`; the mixer combines them without knowing system specifics; per-channel capture is always-on
- **Rewind captures machine state, not persistent storage** — rewinding past a save doesn't unsave; the timeline forks from the restored point
- **Errors are values, not panics** — every fallible operation returns `Result<T, E>` with diagnostic context; `unwrap()` is banned outside tests; warnings accumulate alongside results
- **Configuration inherits downward** — global → system family → variant → session; TOML persistence; ROMs identified by hash not filename
- **Test at every layer** — CPU validation suites per commit; format parser fuzzing nightly; clock tree mathematical proofs; full-system regression against TOSEC weekly
- **Every implementation cites its source** — hardware behaviour in code traces back to a datasheet, die analysis, or hardware test via `Ref:` comments and the reference manifest; "because the other emulator does it" is not a valid citation
- **Peripherals attach to ports, not to machines** — printers, modems, and network devices connect through standard port traits; machines implement the port hardware; peripherals implement the device protocol
- **No game-specific hacks** — tolerant parsing, timing tolerance, known-issues catalogue
- **Archive preservation** — original files read-only; changes stored separately
- **Defer until exercised** — don't define types for domains you haven't built yet

---

## 37. Concise takeaway

What is being built:

- a master-clock-driven scheduler with integer clock division and phase-accurate component interleaving
- a zero-cost observation layer (bus observer, CPU trace, signal trace, event log) that serves interactive debugging, MCP queries, CI regression, and real-time visualisation from the same infrastructure
- a breakpoint engine with conditional execution/memory/IO/cycle/video-position breakpoints
- a multi-window panel UI with OS-native window chrome (macOS NSWindow, Windows HWND, Linux X11/Wayland) where every debug/tool panel is an independent, freely positionable window with layout persistence per system variant
- system-specific debug views (tile banks, nametables, OAM, attributes, bitplanes, copper lists) rendered through a shell-agnostic view model with interactive hit regions
- a display pipeline with per-system pixel aspect ratio, system-specific signal processing (NES NTSC composite, C64 luma bleed), phosphor blur (because real CRTs were never pixel-sharp), and GPU-accelerated CRT simulation (scanlines, phosphor bloom, shadow mask, curvature) with Clean RGB as the default preset
- speed control at 0.25x/0.5x/1x/2x/4x/8x locked to host vsync, plus turbo mode with auto-detection of loading states
- a capture pipeline producing screenshots, video (via FFmpeg), GIF, and audio recordings at any display pipeline stage, with headless mode running real-speed for video/audio and turbo for screenshots/regression
- first-class asset export: individual tiles, sprites, palettes, audio channels, and register logs extractable as PNG, WAV, and JSON
- per-channel audio capture as an always-on feature with waveform visualisation
- an `AudioSource` trait and `AudioMixer` with per-source volume/mute/solo, latency management, speed control interaction, and hot-pluggable source registration for expansion audio
- rewind/time travel via a compressed snapshot ring buffer with delta encoding, configurable per-system memory budget, and clean separation from persistent storage state
- a `TapeTimeline` trait with pulse and sampled implementations, audible loading screech, and tape screech capture
- an `OpticalDisc` trait for CD-ROM/DVD with sector/subchannel access
- a `DiskImage` trait supporting logical and flux levels
- a `CartridgeMapper` trait with database-augmented identification
- a persistence layer with auto-save and archive preservation
- an MCP interface that exposes the full observation, capture, and export surface
- a symbolic/positional keyboard mapper, gamepad/joystick mapping, mouse/paddle/light gun support with per-system input profiles
- serial and parallel port emulation with attachable peripherals (printers, modems, MIDI)
- printer emulation with ESC/P and ZX Printer rendering, exportable as PNG/PDF
- networking: Econet emulation with virtual file server, modem emulation with telnet bridge to live BBSes, Ethernet card emulation with user-mode NAT
- an integrated IDE with per-CPU assemblers alongside disassemblers, shared symbol tables bridging assembler/debugger/disassembler, source-level debugging, and external symbol file import from third-party tools
- BASIC text loading for every BASIC-equipped system via system-specific tokenisers, plus a BASIC editor panel with detokenise-to-text export for preservation
- configuration-driven machine construction supporting every model variant, regional variant, and chip revision across all target families
- a composable `HardwareExtension` trait for period add-ons (DivMMC, REU, accelerator cards, expansion audio) and modern recreations (Spectrum Next) with bus interception for auto-paging ROMs
- ROM management with hash-based identification and per-variant ROM requirements
- TOML-based configuration hierarchy (global → system → variant → session) with ROM index, input profiles, and CRT preset persistence
- project-wide error handling via `Result<T, E>` with `DiagnosticError` providing summary, detail, and user-facing suggestions; warning accumulation for tolerant parsing
- four-layer testing: CPU validation suites (FUSE, Blargg, Tom Harte) per commit, format parser fuzzing nightly, clock tree mathematical validation, and TOSEC full-system regression
- a reference management system with on-disk cached datasheets, manuals, die analyses, and community documentation, catalogued in a searchable manifest with per-implementation `Ref:` citations tracing every hardware behaviour decision to its source
- a TOSEC-driven regression harness and batch asset extraction pipeline for CL198x content
