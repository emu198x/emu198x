> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "Unified App Architecture: EmulatedSystem Trait and Single-Binary Native App"
type: feat
date: 2026-03-19
---

# Unified App Architecture

## Overview

Redesign the Emu198x application layer so that all 13+ systems share a single
native binary with a system launcher, integrated debugger, and audio visualiser.
The UI is entirely data-driven — adding system N requires zero UI code. Per-system
WASM builds remain separate for browser embedding.

## Problem Statement

Today, each system is a separate binary with its own `main.rs`. The four core
systems (Spectrum, C64, NES, Amiga) have bespoke 750+ line `ApplicationHandler`
implementations with duplicated boilerplate for window creation, wgpu setup,
menu construction, and frame pacing. Newer systems use a generic `Runner`, but
it only handles simple cases — no debugger, no audio visualiser, no model
switching. At 13 systems this is manageable; at 100 it's unshippable.

## Proposed Solution

Extend the `Machine` trait (or create a new `EmulatedSystem` supertrait) with
the metadata, input, media, and debugger interfaces the UI needs. Build a
single `emu198x` binary that hosts all systems behind a launcher. Keep
per-system WASM crates for browser builds.

## Technical Approach

### Key Architectural Decision: Per-System Binaries Stay

The existing per-system binaries (`emu-spectrum`, `emu-nes`, etc.) remain as
lightweight wrappers. The unified `emu198x` binary is a NEW crate that
depends on all system crates. This is additive, not a rewrite.

```
emu198x-app/        # New: single binary, system launcher, unified UI
emu198x-ui/         # New: data-driven egui panels (shared across all systems)
emu-core/           # Extended: EmulatedSystem trait, debugger interface
emu-spectrum/       # Kept: thin wrapper using emu-core Runner (or standalone)
emu-c64/            # Kept: thin wrapper
...
```

### Phase 1: EmulatedSystem Trait (Foundation)

Extend the system interface so the UI can drive itself from metadata. All
methods have defaults so existing systems can adopt incrementally.

```rust
// In emu-core

/// Metadata about a system's identity and capabilities.
pub struct SystemInfo {
    /// Machine identifier: "spectrum", "c64", "nes", "amiga", etc.
    pub id: &'static str,
    /// Human-readable name: "ZX Spectrum", "Commodore 64", etc.
    pub name: &'static str,
    /// Manufacturer.
    pub manufacturer: &'static str,
    /// Release year.
    pub year: u16,
    /// Supported file extensions for media loading.
    pub file_extensions: &'static [&'static str],
    /// Configurable options (model, region, peripherals, RAM, etc.).
    pub config_options: Vec<ConfigOption>,
}

/// A configurable option exposed by a system.
///
/// The UI renders these generically as dropdowns, checkboxes, and sliders.
/// The system's factory function receives the chosen values and validates
/// constraints (e.g., "accelerator requires 68020+").
pub enum ConfigOption {
    /// Pick one from a list: model, region, chip revision.
    Choice {
        id: &'static str,
        name: &'static str,
        choices: &'static [(&'static str, &'static str)], // (id, label)
        default: &'static str,
    },
    /// On/off toggle: peripheral, expansion, feature flag.
    Toggle {
        id: &'static str,
        name: &'static str,
        default: bool,
    },
    /// Numeric value: RAM size, clock speed.
    Range {
        id: &'static str,
        name: &'static str,
        min: u32,
        max: u32,
        step: u32,
        default: u32,
        unit: &'static str, // "KB", "MHz", ""
    },
}

/// Resolved configuration values passed to the system factory.
pub type ConfigValues = std::collections::HashMap<String, ConfigValue>;

pub enum ConfigValue {
    Choice(String),
    Toggle(bool),
    Range(u32),
}

// Example: Spectrum config options
//
//   Choice: model (16K, 48K, 128K, +2, +2A, +3)
//   Choice: region (PAL)
//   Toggle: Kempston Joystick (default on)
//   Toggle: Multiface 1 (default off)
//   Toggle: 32KB RAM Pack (default off)
//
// Example: Amiga config options
//
//   Choice: model (A500, A500+, A600, A1200, A2000, A3000, A4000)
//   Choice: region (PAL, NTSC)
//   Range:  Chip RAM (512..2048 KB, step 512)
//   Range:  Fast RAM (0..131072 KB, step 1024)
//   Choice: CPU (68000, 68020, 68030, 68040)
//   Range:  CPU Clock (7..50 MHz, step 1)
//   Toggle: FPU (default off)
//
// Example: NES config options
//
//   Choice: region (NTSC, PAL, Famicom)

/// Display properties the renderer needs.
pub struct DisplayInfo {
    /// Pixel aspect ratio as width:height (e.g. 1.0 for square, 1.067 for PAL).
    pub pixel_aspect_ratio: f32,
    /// Preferred integer scale factor (2, 3, 4).
    pub preferred_scale: u32,
    /// Frame duration for timing the run loop.
    pub frame_duration: std::time::Duration,
}

/// Describes one input port (keyboard, joystick, gamepad, etc.).
pub struct InputPort {
    /// Port name: "Keyboard", "Joystick 1", "Controller", etc.
    pub name: &'static str,
    /// Port type determines the mapping UI.
    pub kind: InputPortKind,
    /// Default key bindings: (host KeyCode, action name).
    pub default_bindings: &'static [(KeyBinding, &'static str)],
}

pub enum InputPortKind {
    /// Keyboard matrix (Spectrum, C64, BBC, MSX, Amiga).
    Keyboard,
    /// Digital directional pad + buttons (NES, SMS, SG-1000).
    Gamepad { buttons: &'static [&'static str] },
    /// Analog joystick + buttons (Atari 5200).
    AnalogStick { buttons: &'static [&'static str] },
    /// Digital joystick + fire (Atari 2600/7800, C64 joystick port).
    Joystick,
}

/// Host-side key binding.
pub enum KeyBinding {
    Key(winit::keyboard::KeyCode),
    // Future: GamepadButton, MouseButton
}

/// A single audio channel for the visualiser.
pub struct AudioChannelInfo {
    /// Channel name: "Voice 1", "Channel A", "Paula Ch 0", etc.
    pub name: &'static str,
    /// Chip that owns this channel: "SID", "AY-3-8910", "Paula", etc.
    pub chip: &'static str,
}

/// Extended system interface for the unified UI.
///
/// Supertraits: Machine (frame/video/audio/reset) + Tickable (per-tick).
/// All methods have defaults so adoption is incremental.
pub trait EmulatedSystem: Machine + Send {
    /// System identity and capabilities.
    fn system_info(&self) -> &SystemInfo;

    /// Display properties for the renderer.
    fn display_info(&self) -> DisplayInfo;

    /// Input ports this system exposes.
    fn input_ports(&self) -> &[InputPort] { &[] }

    /// Route a host key event to the system.
    fn handle_key(&mut self, keycode: winit::keyboard::KeyCode, pressed: bool);

    /// Load a media file (ROM, disk, tape, cartridge).
    fn load_media(&mut self, path: &std::path::Path) -> Result<(), String> {
        Err(format!("Media loading not supported"))
    }

    /// Audio channel descriptions for the visualiser.
    fn audio_channels(&self) -> &[AudioChannelInfo] { &[] }

    /// Per-channel audio samples since last drain.
    /// Returns one Vec<f32> per channel, in the same order as audio_channels().
    fn take_channel_audio(&mut self) -> Vec<Vec<f32>> { vec![] }

    // --- Debugger interface ---

    /// CPU count (most systems have 1; C64 has 2 with 1541 drive).
    fn cpu_count(&self) -> usize { 1 }

    /// CPU name for the debugger tab.
    fn cpu_name(&self, index: usize) -> &str { "CPU" }

    /// CPU register snapshot for the debugger.
    fn cpu_registers(&self, index: usize) -> Vec<(&str, Value)> { vec![] }

    /// Read a byte from the CPU address space (non-destructive).
    fn debug_read(&self, cpu_index: usize, addr: u32) -> Option<u8> { None }

    /// Disassemble one instruction at the given address.
    /// Returns (mnemonic, byte_length).
    fn disassemble(&self, cpu_index: usize, addr: u32) -> Option<(String, u8)> {
        None
    }

    /// Set or clear a breakpoint. Returns true if supported.
    fn set_breakpoint(&mut self, cpu_index: usize, addr: u32, enabled: bool) -> bool {
        false
    }

    /// List active breakpoints.
    fn breakpoints(&self, cpu_index: usize) -> Vec<u32> { vec![] }

    /// Step one instruction (not one tick). Returns ticks consumed.
    fn step_instruction(&mut self, cpu_index: usize) -> u64 { 0 }

    /// Query observable state by path (delegates to Observable if implemented).
    fn query(&self, path: &str) -> Option<Value> { None }

    /// List available query paths.
    fn query_paths(&self) -> Vec<&str> { vec![] }

    // --- Save states ---

    /// Serialize the entire machine state.
    fn save_state(&self) -> Option<Vec<u8>> { None }

    /// Restore from a previously saved state.
    fn load_state(&mut self, _data: &[u8]) -> Result<(), String> {
        Err("Save states not supported".into())
    }
}
```

**Why a new trait instead of extending `Machine`**: `Machine` is deliberately
minimal and stable — 13 systems and 13 WASM crates depend on it. Adding 20+
methods would bloat every implementation. `EmulatedSystem` is a superset that
the unified app consumes; simpler tools (WASM wrappers, test harnesses) keep
using `Machine`.

**Why `Send` bound**: The emulator runs in a background thread. The UI thread
holds a `Box<dyn EmulatedSystem>` behind a mutex or channel.

### Phase 2: System Registration (Single Binary)

Each system provides a factory function. The app binary collects them into
a registry at startup.

```rust
// In emu198x-app

/// Factory that creates a system from config values and optional ROM data.
/// Config values come from the launcher UI (driven by SystemInfo::config_options).
/// The factory validates constraints and returns a clear error on invalid combos.
pub type SystemFactory = fn(config: &ConfigValues, rom: Option<&[u8]>) -> Result<Box<dyn EmulatedSystem>, String>;

/// Entry in the system catalogue.
pub struct SystemEntry {
    pub info: SystemInfo,
    pub factory: SystemFactory,
}

/// Global registry, populated at startup.
pub fn system_catalogue() -> Vec<SystemEntry> {
    vec![
        emu_spectrum::register(),
        emu_c64::register(),
        emu_nes::register(),
        emu_amiga::register(),
        emu_atari_2600::register(),
        // ... all 13+ systems
    ]
}
```

Each system crate exports a `register()` function that returns its
`SystemEntry`. The app binary's `main.rs` calls all of them. No dynamic
loading, no plugins — just static linking. Adding a system means adding
one line to the catalogue and one dependency to `Cargo.toml`.

### Phase 3: Unified UI Shell (emu198x-ui)

A new crate providing data-driven egui panels that consume `&dyn EmulatedSystem`.

```
┌──────────────────────────────────────────────────────┐
│ Menu Bar (File, System, Display, Audio, Tools, Help) │
├──────────┬───────────────────────────────┬───────────┤
│          │                               │           │
│ (panel)  │      Emulation Viewport       │  (panel)  │
│          │                               │           │
│          │                               │           │
├──────────┴───────────────────────────────┴───────────┤
│ Status Bar: system name | variant | FPS | media      │
└──────────────────────────────────────────────────────┘
```

**Panels (all dockable, all optional):**

| Panel | Drives from | Phase |
|-------|------------|-------|
| System Launcher | `system_catalogue()` | 3 |
| CPU Debugger | `cpu_registers()`, `disassemble()`, `step_instruction()` | 4 |
| Memory Viewer | `debug_read()` | 4 |
| Breakpoints | `set_breakpoint()`, `breakpoints()` | 4 |
| Chip Inspector | `query()`, `query_paths()` | 4 |
| Audio Visualiser | `audio_channels()`, `take_channel_audio()` | 5 |
| Audio Mixer | `audio_channels()` + mute/solo state | 5 |
| Input Config | `input_ports()` | 6 |
| Save States | `save_state()`, `load_state()` + thumbnail | 6 |

**Every panel reads from the trait. No panel knows what a VIC-II is.**

### Phase 4: CPU Debugger

The debugger is the feature that makes people take an emulator seriously.

**Requirements:**
- Register view with hex/decimal toggle, per-CPU tabs
- Disassembly view centered on PC, with breakpoint gutters
- Memory hex view with goto-address and ASCII sidebar
- Step (instruction), Step Over (skip JSR/CALL), Run to cursor
- Breakpoint list with enable/disable/delete
- Works identically for 6502, Z80, 68000, and future CPUs

**Implementation:** All debugger operations go through `EmulatedSystem` trait
methods. The debugger panel is ~500 lines of egui code that works for every
system. CPU-specific formatting (register names, disassembly syntax) comes
from the trait implementation, not the UI.

**Pause model:** The UI holds a `paused: bool` flag. When paused, the
emulation thread stops calling `run_frame()`. The UI can call
`step_instruction()` directly. When unpaused, the background thread resumes.

### Phase 5: Audio Visualiser

**Per-channel ring buffers:** Each audio chip maintains a per-channel sample
buffer alongside its existing mixed output. `take_channel_audio()` drains
these buffers each frame. The UI reads them at 60fps — it's a tap, not a
redirect.

**Waveform mode:** `egui::Painter` line drawing, one trace per channel,
colour-coded. Click to solo, right-click to mute.

**Piano roll mode:** Pitch derived from chip frequency registers (not FFT —
we have register access). Vertical axis = pitch, horizontal = time, scrolling.
Colour per channel.

**Chip changes required:**
- SID: add per-voice output buffer (3 channels)
- AY: add per-channel output buffer (3 channels)
- Paula: already exposes `audio_channel_state()` — add sample buffer (4 channels)
- TIA/POKEY/GTIA: add per-voice output buffer
- SN76489: add per-channel output buffer (4 channels)

### Phase 6: Input, Save States, and Polish

**Input configuration:** The `InputPort` descriptors drive a generic mapping
UI. Users see "NES Controller: A = Z, B = X, Up = Arrow Up" and can rebind.
Mappings persist to a TOML config file keyed by system ID.

**Save states:** `save_state()` serializes the machine to a byte blob (likely
via a custom binary format — not serde, to avoid versioning pain with 100+
systems). The UI stores blobs in `~/.emu198x/saves/{system_id}/slot_{n}.state`
alongside a PNG thumbnail.

**CRT shaders:** wgpu fragment shaders applied to the viewport texture.
Picker in the Display menu. Shader files loaded from a `shaders/` directory.

## Alternative Approaches Considered

**Enum dispatch instead of trait objects:** An `enum System { Spectrum(Spectrum),
Nes(Nes), ... }` avoids dynamic dispatch overhead but requires touching the
enum for every new system. At 100 systems, this is unmaintainable. Trait
objects with `Box<dyn EmulatedSystem>` are the right choice — the vtable
cost is negligible compared to emulation work.

**Extending `Machine` instead of a new trait:** Adding 20+ methods to
`Machine` forces all 13 WASM crates and test harnesses to provide stubs.
A separate `EmulatedSystem` supertrait keeps `Machine` lean.

**Single WASM binary:** Rejected. A Spectrum lesson page shouldn't download
Amiga emulation code. Per-system WASM builds are intentional and stay.

**Plugin/dylib architecture:** Premature complexity. Static linking is
simpler, faster to compile (incremental builds), and avoids ABI headaches.
Revisit if the binary size becomes a problem at 50+ systems.

## Acceptance Criteria

### Functional Requirements

- [ ] `EmulatedSystem` trait in `emu-core` with all methods defined
- [ ] At least 4 systems implement the full trait (Spectrum, C64, NES, one Atari)
- [ ] `emu198x-app` binary launches with system catalogue
- [ ] System launcher shows all registered systems with filtering
- [ ] Loading a ROM auto-detects the system from file extension
- [ ] CPU debugger works for 6502, Z80, and 68000 systems
- [ ] Audio visualiser shows per-channel waveforms for SID and AY
- [ ] All existing per-system binaries continue to work unchanged
- [ ] Headless/script mode works via MCP against the unified binary
- [ ] Compatibility harness runs ROM directories headlessly with pass/fail reporting
- [ ] ConfigOption-driven launcher renders system configuration generically

### Non-Functional Requirements

- [ ] Adding a new system requires only: implement `EmulatedSystem`, add one line to catalogue
- [ ] No system-specific code in `emu198x-ui` (enforced by code review)
- [ ] Frame pacing maintains <16ms input-to-display latency
- [ ] Binary size stays reasonable (under 50MB for all 13 systems)

### Quality Gates

- [ ] All existing CPU test suites pass (6502, Z80, 68000)
- [ ] All existing boot tests pass
- [ ] Debugger step/breakpoint works on at least 3 CPU architectures
- [ ] Audio visualiser runs at 60fps without affecting emulation speed
- [ ] Compatibility harness processes 1000+ ROMs per system in under 10 minutes

## Implementation Phases

### Phase 1: EmulatedSystem Trait (1-2 weeks)
- Define trait in `emu-core`
- Implement for Spectrum (simplest keyboard system)
- Implement for NES (simplest gamepad system)
- Implement for one Atari system (to prove the pattern)
- Keep all existing binaries working — trait is additive

### Phase 2: App Shell + Launcher (1-2 weeks)
- Create `emu198x-app` crate
- System registry with factory functions
- egui app shell: menu bar, viewport, status bar
- System launcher panel with catalogue browsing
- Basic ROM loading via file dialog

### Phase 3: Viewport + Frame Loop (1 week)
- Background emulation thread with channel-based state sync
- wgpu texture upload from framebuffer
- Audio output via cpal
- Frame pacing with configurable speed

### Phase 4: CPU Debugger (2-3 weeks)
- Implement `cpu_registers()`, `disassemble()`, `debug_read()` for 6502, Z80, 68000
- Breakpoint infrastructure in CPU cores
- Debugger panel: registers, disassembly, memory, breakpoints
- Step/pause/resume controls

### Phase 5: Audio Visualiser (1-2 weeks)
- Per-channel sample buffers in SID, AY, SN76489, Paula, POKEY, TIA
- `audio_channels()` and `take_channel_audio()` implementations
- Waveform panel with mute/solo
- Piano roll panel (stretch goal)

### Phase 6: Remaining Systems + Polish (2-3 weeks)
- Implement `EmulatedSystem` for all 13 systems
- Input configuration panel and persistence (6a — done)
- Save state infrastructure (6b — done, per-system serialisation pending)
- CRT shader pipeline (6d — done)
- Recording: screenshot/video/audio (6c — done)

### Phase 6e: Media & Peripheral UI

Physical media and peripheral interaction for an authentic experience.

**Drive LEDs / Motor Status:**
- New trait method `peripheral_status() -> Vec<PeripheralIndicator>` on
  EmulatedSystem, returning name, kind (LED/motor/counter), and state.
- Status bar shows drive activity LEDs, track numbers, motor state.
- Applies to: 1541 drive (C64), Amiga floppy, Spectrum +3 disk.

**Cassette Deck UI:**
- Transport controls: play, stop, pause, rewind, fast-forward, eject.
- Tape counter (position / total length) and progress bar.
- New trait method `tape_command(TapeAction)` for deck control.
- Applies to: Spectrum (TAP/TZX), C64 (TAP), BBC Micro (UEF).

**Cartridge Insert / Eject:**
- Visual indicator of loaded media in the status bar.
- Eject action returns to BIOS screen for systems with one (ColecoVision,
  SMS). Hot-swap via `load_media()`.
- Applies to: NES, SG-1000, SMS, ColecoVision, Atari 2600/5200/7800.

**Drag and Drop:**
- Handle winit `DroppedFile` events. In Launcher mode, auto-detect system
  from file extension and launch. In Running mode, pass to `load_media()`.

### Phase 6f: Virtual Printer

Capture printer output and render it in a scrollable "paper" panel.

**Level 1 — Text capture:** Intercept bytes sent to the printer port
and display as text (ASCII/PETSCII/Spectrum charset). Works for LPRINT,
LIST#, PRINT#. Minimal effort per system.

**Level 2 — Dot-matrix emulation:** Model an Epson FX-80 compatible
printer with ESC/P escape sequence parsing (bold, underline, condensed,
graphics mode, line spacing). Render to a bitmap that scrolls like
continuous-feed paper. One implementation covers all systems since
ESC/P was the universal standard.

**Per-system hookup:**
- Spectrum: intercept LPRINT (RST $10 with stream #3)
- C64: IEC serial bus device #4
- BBC Micro: VIA user port parallel output
- Amiga: CIA-A parallel port ($BFE101)
- Atari 800XL: SIO bus P: device handler

**UI:** Scrollable paper panel with dot-matrix rendered text. Export
to PNG for lesson write-ups. Teaching angle: "Write a BASIC program
that prints a pattern" with visible output on virtual paper.

**Trait additions:**
- `printer_output(&mut self) -> Vec<u8>` — drain bytes sent to printer
- `has_printer(&self) -> bool` — whether this system has a printer port

### Phase 7: Compatibility Testing at Scale

The most honest measure of emulation quality. Run thousands of ROMs
automatically, detect failures, track results over time.

**Test tiers:**

1. **Boot test** — load ROM, run N frames, check for crash/hang/black screen.
   This is the baseline. If a ROM can't boot, nothing else matters.

2. **Screenshot comparison** — run to a known point (title screen, attract
   mode), capture framebuffer, diff against a reference image from a trusted
   emulator (VICE, Fuse, Mesen, WinUAE). Pixel-diff with tolerance for
   analog path differences.

3. **Heuristic health checks** — detect common failure modes without
   reference images:
   - Black screen after N frames (hang or crash)
   - Single solid colour (boot failure)
   - CPU stuck at one address (infinite loop)
   - No audio output after N frames (audio init failure)
   - Exception/trap that shouldn't fire (illegal instruction, bus error)

4. **Demo scene torture tests** — curated list of hardware-pushing demos
   per system. These stress timing, DMA, and edge cases harder than games.

**Infrastructure:**

```rust
/// Result of running one ROM through the compatibility harness.
pub struct CompatResult {
    pub system: String,
    pub rom_path: PathBuf,
    pub rom_hash: String,        // SHA-256 for dedup
    pub status: CompatStatus,
    pub frames_run: u64,
    pub final_pc: u32,
    pub screenshot: Option<Vec<u8>>,  // PNG bytes
    pub duration_ms: u64,
}

pub enum CompatStatus {
    /// Ran to completion, screenshot captured.
    Ok,
    /// CPU stuck at one address for >1000 frames.
    Hang { pc: u32 },
    /// Black or single-colour screen after boot.
    BlackScreen,
    /// CPU hit an unhandled exception.
    Crash { vector: u8, pc: u32 },
    /// Mapper or format not supported.
    Unsupported { reason: String },
    /// Screenshot differs from reference beyond threshold.
    VisualMismatch { diff_percent: f32 },
}
```

**Execution model:** The `EmulatedSystem` trait makes this trivial —
construct from `ConfigValues`, call `load_media()`, call `run_frame()` N
times, read `framebuffer()`, check health. No UI needed. Runs headless,
parallelised across CPU cores.

**Tracking:** Results stored as JSON per run, indexed by ROM hash. A simple
HTML report shows pass/fail/regression per system. CI runs this on every
commit against a curated ROM set.

**ROM sourcing:** TOSEC collections provide comprehensive coverage. The
harness takes a directory of ROMs, auto-detects system from extension,
and runs everything. No ROM database needed — just point it at a folder.

**Chip library multiplier:** A fix to the shared SN76489 improves SG-1000,
BBC Micro, SMS, and ColecoVision simultaneously. The compatibility matrix
shows this propagation instantly.

## Open Questions (from Spec-Flow Analysis)

These were surfaced during flow analysis and need answers before or during
implementation:

### Resolved in this plan

- **Unified binary or separate binaries?** Both. Per-system binaries stay.
  The unified `emu198x-app` is additive. WASM stays per-system.
- **egui replaces or layers on current runner?** Layers. The emulation
  viewport renders via wgpu directly (fast path). egui overlays for menus,
  panels, and debugger via `egui-wgpu` + `egui-winit` integration. No
  texture copy for the viewport — egui and the viewport share the wgpu
  surface.
- **Address width for debugger?** `u32` everywhere. The existing
  `peek_memory(u16)` on `Machine` is insufficient for 68000. The new
  `debug_read(cpu_index, u32)` method on `EmulatedSystem` replaces it
  for debugger use.
- **Multi-CPU systems?** CPU index parameter on all debugger methods.
  `cpu_count()` and `cpu_name(index)` let the UI build tabs. Primary
  CPU (index 0) is the default debug target.
- **Audio mute semantics?** Mute at the output stage, not the hardware
  register level. Muted channels still participate in filter calculations
  (critical for SID resonance). The per-channel sample buffer is a tap
  after the voice but before the mixer.

### To resolve during implementation

- **Threading protocol:** Channel-based message passing with double-buffered
  framebuffer. Commands (key events, step, breakpoint) sent to emulation
  thread. State snapshots (framebuffer, registers, audio) sent back. Use
  `try_recv()` on the UI side to avoid blocking. Requires `Send` on all
  system structs — survey which systems need changes.
- **Soft vs hard reset:** Soft = CPU reset pin (peripherals reset, RAM
  preserved). Hard = reconstruct the machine (power cycle). Add
  `power_cycle(&mut self)` to the trait alongside existing `reset()`.
- **Keyboard capture model:** When viewport is focused, all keys go to the
  emulated system. A configurable "UI escape" key (default: F1 or a
  menu-bar-only modifier) switches to UI mode. This is per the existing
  `frontend.md` design.
- **Save state format:** Versioned binary with a header containing system ID,
  variant, and format version. No backwards compatibility in v1. Thumbnail
  stored as sidecar PNG.
- **ROM discovery:** Configurable ROM directory (default `~/.emu198x/roms/`).
  First-run prompt if required ROMs are missing. Embedded ROMs for systems
  where licensing allows (Spectrum, some Atari).
- **Video recording codec:** Shell out to ffmpeg CLI. WAV + PNG frame dump
  as fallback. No bundled codec library.
- **Panel docking:** Fixed layout presets (Default, Debug, Audio) for v1.
  `egui::SidePanel` / `TopBottomPanel` for structure, `egui::Window` for
  floating tools. Full drag-to-dock in v2 if needed.
- **WASM debugger scope:** Read-only state inspector. Full stepping and
  breakpoints are native-only.

## Dependencies and Risks

**Risk: Amiga complexity.** The Amiga has bespoke audio output, viewport
extraction, and a 3-layer keyboard mapper. It will be the hardest system
to fit through the generic trait. Mitigation: implement Amiga last, after
the trait is proven on simpler systems.

**Risk: egui layout limitations.** egui lacks true docking (no ImGui docking
branch equivalent). Mitigation: use `SidePanel`/`TopBottomPanel` for fixed
layout, `Window` for floating tools. Don't fight the toolkit.

**Risk: Thread synchronisation.** Emulator in background thread, UI in main
thread. Mitigation: use `try_recv()` (non-blocking) for state updates. The
documented pattern in `features/frontend.md` already covers this.

**Dependency: Observable coverage.** The debugger needs `Observable` on all
systems. Currently only ~half implement it. This is a prerequisite for Phase 4.

## Future Considerations

### Time-Travel Debugging (Rewind / Step Back)

Go beyond step/pause/resume: let learners step *backwards* through execution
and scrub a rewind timeline. No cycle-accurate emulator does this well — it
would be a strong differentiator for Code198x lessons ("step back to see what
the CPU did before the crash").

**Depends on:** `save_state()` / `load_state()` implemented per-system.

**Design:**

- **Ring buffer of snapshots.** Capture full machine state every N frames
  (e.g. every 60 frames = 1 snapshot/second) into a fixed-size ring buffer.
  Rewinding jumps to the nearest prior snapshot and replays forward to the
  exact target frame. Input events must also be recorded for replay fidelity.

- **Memory budget.** Each snapshot is the full machine state (RAM, registers,
  chip state). Spectrum: ~50 KB. C64: ~100 KB. NES: ~40 KB. Amiga: 1–8 MB
  (chip RAM + fast RAM + chip registers). A 30-second ring buffer at
  1 snapshot/sec = 1.5 MB (Spectrum) to 240 MB (Amiga 8 MB config).
  Compression (delta encoding between consecutive snapshots) could cut this
  10–50×.

- **Serialisation speed.** Snapshots must complete within the frame budget
  (~20 ms PAL, ~16 ms NTSC). Flat memcpy of contiguous structs is fine.
  Systems with heap-allocated trait objects (`Box<dyn SpectrumMemory>`) need
  a fast serialise path — ideally a pre-allocated byte buffer that the
  system fills without allocation.

- **UI.** Timeline scrubber below the viewport. Step-back button alongside
  step-forward. Frame number display. Optional "rewind" hold-button that
  plays backwards in real time (like bsnes).

**Prior art:** bsnes, Bizhawk, FCEUX, and Mesen all support rewind. bsnes
uses periodic save states with replay. Bizhawk records input + periodic
states. None are cycle-accurate in the Emu198x sense.

**Phase:** After save states land (Phase 6). Could be Phase 7 or a
standalone feature phase.

### Other Future Work

- **Cross-system trace comparison:** same Z80 code on Spectrum vs SG-1000
- **Chip audio A/B comparison:** same SN76489 in different machines
- **Chip genealogy viewer:** visualise chip family trees across systems
- **JIT compilation:** Cranelift backend as alternative CPU trait impl (defer until needed)
- **Netplay:** out of scope

## References

### Internal
- `crates/emu-core/src/machine.rs` — current `Machine` trait
- `crates/emu-core/src/observable.rs` — `Observable` trait and `Value` enum
- `crates/emu-core/src/runner.rs` — generic `Runner<M: Machine>`
- `crates/emu-spectrum/src/main.rs` — bespoke runner example (748 lines)
- `crates/emu-sg1000/src/main.rs` — generic Runner example (191 lines)
- `docs/features/frontend.md` — UI design notes and threading model
- `docs/features/observability.md` — observable state design

### External
- egui: https://github.com/emilk/egui
- Mesen debugger (reference UI): https://www.mesen.ca
