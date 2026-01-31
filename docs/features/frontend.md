# Frontend

## Overview

Each system is a **separate binary**:
- `emu-spectrum`
- `emu-c64`
- `emu-nes`
- `emu-amiga`

Each binary provides:
- System launcher with variant/option selection
- Visual media controls (tape deck, disk drive)
- Input configuration
- Display and audio output

Multiple frontend targets share the same interaction model:
- **Native** — Desktop app (Linux, macOS, Windows)
- **Web** — Browser via WASM (one per system)
- **Headless** — CLI and MCP for automation

A human clicking "Stop" on a tape deck and a script calling `tape_stop()` are equivalent operations.

## System Launcher

Each binary opens with a launcher screen for that system. This is where you configure the variant, memory, and options before starting emulation.

### Spectrum Launcher

```
┌─────────────────────────────────────┐
│  ZX Spectrum                        │
├─────────────────────────────────────┤
│                                     │
│  Model                              │
│  ┌─────────────────────────────┐    │
│  │ ○ 48K                       │    │
│  │ ○ 128K                      │    │
│  │ ○ +2                        │    │
│  │ ○ +2A                       │    │
│  │ ○ +3                        │    │
│  └─────────────────────────────┘    │
│                                     │
│  Region         [ PAL ▼ ]           │
│                                     │
│  ☐ Interface 1 (Microdrive)         │
│  ☐ Kempston joystick interface      │
│                                     │
│  ─────────────────────────────────  │
│                                     │
│  Recent:                            │
│  • Chase H.Q. (128K) — yesterday    │
│  • Manic Miner — 3 days ago         │
│                                     │
│  [ Start ]  [ Load File... ]        │
│                                     │
└─────────────────────────────────────┘
```

### C64 Launcher

```
┌─────────────────────────────────────┐
│  Commodore 64                       │
├─────────────────────────────────────┤
│                                     │
│  Region         [ PAL ▼ ]           │
│                                     │
│  SID Chip       [ 6581 ▼ ]          │
│                 6581 (original)     │
│                 8580 (later)        │
│                                     │
│  Expansions                         │
│  ☐ 1541 Drive (directly emulated)   │
│  ☐ 1541-II (accent drive)           │
│  ☐ REU (RAM Expansion Unit)         │
│      Size: [ 512K ▼ ]               │
│                                     │
│  ─────────────────────────────────  │
│                                     │
│  Recent:                            │
│  • Boulder Dash.d64 — today         │
│  • Impossible Mission.d64 — 1 week  │
│                                     │
│  [ Start ]  [ Load File... ]        │
│                                     │
└─────────────────────────────────────┘
```

### NES Launcher

```
┌─────────────────────────────────────┐
│  NES / Famicom                      │
├─────────────────────────────────────┤
│                                     │
│  System        [ NES ▼ ]            │
│                NES (Western)        │
│                Famicom (Japanese)   │
│                                     │
│  Region        [ NTSC ▼ ]           │
│                                     │
│  Expansions                         │
│  ☐ Famicom Disk System              │
│                                     │
│  ─────────────────────────────────  │
│                                     │
│  Recent:                            │
│  • Super Mario Bros 3.nes — today   │
│  • Zelda.nes — yesterday            │
│                                     │
│  [ Start ]  [ Load File... ]        │
│                                     │
└─────────────────────────────────────┘
```

### Amiga Launcher

```
┌─────────────────────────────────────┐
│  Amiga                              │
├─────────────────────────────────────┤
│                                     │
│  Model Preset   [ A500 ▼ ]          │
│                 A500                │
│                 A500+               │
│                 A600                │
│                 A1200               │
│                 A2000               │
│                 A4000               │
│                 Custom...           │
│                                     │
│  ─── or configure manually ───      │
│                                     │
│  Chipset       [ OCS ▼ ]            │
│  CPU           [ 68000 ▼ ]          │
│  Chip RAM      [ 512K ▼ ]           │
│  Fast RAM      [ None ▼ ]           │
│  Kickstart     [ 1.3 ▼ ]            │
│                                     │
│  Region        [ PAL ▼ ]            │
│                                     │
│  ─────────────────────────────────  │
│                                     │
│  Recent:                            │
│  • Shadow of the Beast.adf — today  │
│  • Workbench 1.3 — 2 days ago       │
│                                     │
│  [ Start ]  [ Load File... ]        │
│                                     │
└─────────────────────────────────────┘
```

### Launcher Behaviours

**"Load File..." button:**
- Opens file picker
- Auto-detects media type
- Can influence variant selection (128K-only game suggests 128K)
- Goes straight to emulation after selection

**Command-line bypass:**
```bash
# Skip launcher, use defaults
emu-spectrum --start game.tap

# Skip launcher, specify config
emu-spectrum --model 128k --start game.tap

# Open launcher with file pre-selected
emu-spectrum game.tap
```

**"Start" button:**
- Boots system with selected configuration
- No media loaded (unless file was selected)
- Shows main emulator window

**Recent list:**
- Remembers media + configuration pairs
- Click to launch directly with that config

### Preset vs Custom (Amiga)

Selecting a preset fills in the detailed options:

| Preset | Chipset | CPU | Chip RAM | Kickstart |
|--------|---------|-----|----------|-----------|
| A500 | OCS | 68000 | 512K | 1.3 |
| A500+ | ECS | 68000 | 1M | 2.04 |
| A600 | ECS | 68000 | 2M | 2.05 |
| A1200 | AGA | 68020 | 2M | 3.0/3.1 |

Selecting "Custom..." enables all dropdowns for manual configuration (accelerator cards, expanded RAM, etc.).

## Optional: Unified Launcher

A thin wrapper binary (`emu198x`) can provide convenience:

```bash
# Detect file type, spawn correct emulator
emu198x game.tap        # → spawns emu-spectrum
emu198x game.d64        # → spawns emu-c64
emu198x game.nes        # → spawns emu-nes
emu198x game.adf        # → spawns emu-amiga

# No file: show system picker, then spawn
emu198x
```

The picker is minimal — just choose a system, then spawn its binary:

```
┌─────────────────────────────────┐
│  Emu198x                        │
├─────────────────────────────────┤
│                                 │
│   [ ZX Spectrum ]               │
│   [ Commodore 64 ]              │
│   [ NES / Famicom ]             │
│   [ Amiga ]                     │
│                                 │
│   ───────────────────────────   │
│   Or drop a file here           │
│                                 │
└─────────────────────────────────┘
```

This wrapper does **not** contain emulation code. It just:
1. Detects file type (by extension or header)
2. Execs the appropriate binary with the file as argument

The real configuration happens in each system's own launcher.

## Media Controls

### Design Principles

1. **Visual representation** — Media devices are visible widgets, not hidden menus
2. **Drag and drop** — Drop files onto the appropriate device
3. **Manual control** — Play, Stop, Rewind, Eject buttons work
4. **Automation compatible** — Every UI action has a programmatic equivalent
5. **Status feedback** — Show what's happening (loading, motor on, head position)

### Tape Deck (Spectrum, C64)

```
┌────────────────────────────────────────┐
│ ▶ TAPE                                 │
├────────────────────────────────────────┤
│  ┌──────────────────────────────────┐  │
│  │  ◉ ════════════════════════ ◉   │  │
│  │      Jet Set Willy.tap          │  │
│  │      Block 3 of 12              │  │
│  └──────────────────────────────────┘  │
│                                        │
│  [⏮] [⏪] [▶] [⏹] [⏩] [⏏]            │
│                                        │
│  ░░░░░░░░░░░░░████░░░░░░░░░░░  25%    │
└────────────────────────────────────────┘
```

**Controls:**
- ⏮ Rewind to start
- ⏪ Rewind (fast)
- ▶ Play
- ⏹ Stop
- ⏩ Fast forward
- ⏏ Eject

**Drag and drop:**
- Drop .tap, .tzx, .t64 onto tape deck
- Visual feedback when dragging over valid target

**Status:**
- Current file name
- Block number (for multi-block tapes)
- Progress bar
- Motor indicator (spinning when active)

**Automation equivalent:**
```rust
emulator.tape_insert("game.tap")?;
emulator.tape_play();
// ... emulation runs ...
emulator.tape_stop();
emulator.tape_rewind();
emulator.tape_eject();
```

### Disk Drive (C64, Amiga, Spectrum +3)

```
┌─────────────────────────────┐
│ DRIVE 8                     │
├─────────────────────────────┤
│  ┌───────────────────────┐  │
│  │ ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄ │  │
│  │ █  Boulder Dash.d64 █ │  │ ← disk visible in slot
│  │ ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀ │  │
│  └───────────────────────┘  │
│                             │
│  💡 Track 18  [⏏]          │
└─────────────────────────────┘
```

**For Amiga (multiple drives):**
```
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│   DF0:   │ │   DF1:   │ │   DF2:   │ │   DF3:   │
│ ▄▄▄▄▄▄▄▄ │ │          │ │          │ │          │
│ █Game  █ │ │  Empty   │ │  Empty   │ │  Empty   │
│ ▀▀▀▀▀▀▀▀ │ │          │ │          │ │          │
│ 💡 T:40  │ │          │ │          │ │          │
│   [⏏]    │ │   [⏏]    │ │   [⏏]    │ │   [⏏]    │
└──────────┘ └──────────┘ └──────────┘ └──────────┘
```

**Status:**
- Disk label/filename
- Activity LED
- Track position (where head is)
- Motor status

**Drag and drop:**
- Drop .d64, .g64, .adf, .adz, .ipf onto drive
- For multi-drive systems, drop onto specific drive

**Automation equivalent:**
```rust
emulator.disk_insert("df0", "game.adf")?;
// ... emulation runs ...
emulator.disk_eject("df0");
```

### Cartridge Slot (NES, C64, Amiga)

```
┌─────────────────────────────┐
│ CARTRIDGE                   │
├─────────────────────────────┤
│  ┌───────────────────────┐  │
│  │    ╔═══════════╗      │  │
│  │    ║ Super     ║      │  │
│  │    ║ Mario     ║      │  │
│  │    ║ Bros 3    ║      │  │
│  │    ╚═══════════╝      │  │
│  └───────────────────────┘  │
│         [⏏ Remove]          │
└─────────────────────────────┘
```

Cartridge insertion typically requires reset. Prompt user:

```
Insert "Mega Man 2.nes"?
This will reset the system.
[ Cancel ] [ Insert & Reset ]
```

### Status Bar (Minimal Mode)

For less intrusive display, collapse to status bar:

```
┌─────────────────────────────────────────────────────────┐
│ [🖴 DF0: Game.adf 💡] [🖴 DF1: Empty] [🎮 Joy Port 2]   │
└─────────────────────────────────────────────────────────┘
```

Click to expand, drag files onto icons.

## Input Configuration

### Keyboard Mapping

Physical keyboard to emulated keyboard. Most keys map directly, but some need configuration:

```
┌─────────────────────────────────────────┐
│ Keyboard Mapping                        │
├─────────────────────────────────────────┤
│                                         │
│ Emulated Key    Physical Key            │
│ ─────────────   ────────────            │
│ CAPS SHIFT      Left Shift              │
│ SYMBOL SHIFT    Right Shift  [Change]   │
│ ENTER           Enter                   │
│ BREAK           Escape       [Change]   │
│                                         │
│ [ Positional ] [ Symbolic ] [ Custom ]  │
│                                         │
└─────────────────────────────────────────┘
```

**Modes:**
- **Positional** — Keys match physical position (UK layout maps to Spectrum layout)
- **Symbolic** — Keys match symbol (pressing @ produces @, wherever it is)
- **Custom** — User-defined mapping

### Joystick Mapping

Map physical input (keyboard, gamepad) to emulated joystick:

```
┌─────────────────────────────────────────┐
│ Joystick Port 2                         │
├─────────────────────────────────────────┤
│                                         │
│ Input Device: [ Keyboard ▼ ]            │
│                                         │
│         [ W ]                           │
│           ↑                             │
│   [ A ] ←   → [ D ]      [Space] Fire   │
│           ↓                             │
│         [ S ]                           │
│                                         │
│ ─── or ───                              │
│                                         │
│ Input Device: [ Xbox Controller ▼ ]     │
│                                         │
│ Left Stick / D-Pad → Directions         │
│ A Button → Fire                         │
│                                         │
└─────────────────────────────────────────┘
```

**Supported inputs:**
- Keyboard (configurable keys)
- Gamepad (auto-detected, configurable)
- Touch (on-screen controls for mobile/tablet)

### Mouse Mapping (Amiga, ST)

```
┌─────────────────────────────────────────┐
│ Mouse                                   │
├─────────────────────────────────────────┤
│                                         │
│ ☑ Capture mouse when window focused     │
│ ☐ Use raw input (bypasses OS accel)     │
│                                         │
│ Sensitivity: [────●─────] 1.0x          │
│                                         │
│ Press Escape to release mouse           │
│                                         │
└─────────────────────────────────────────┘
```

### Multiple Input Devices

Systems with multiple ports need clear assignment:

```
┌─────────────────────────────────────────┐
│ Input Devices                           │
├─────────────────────────────────────────┤
│                                         │
│ Port 1:  [ Joystick (Keyboard WASD) ▼ ] │
│ Port 2:  [ Joystick (Xbox Pad 1)    ▼ ] │
│ Mouse:   [ System Mouse             ▼ ] │
│                                         │
│ [ Swap Ports ]                          │
│                                         │
└─────────────────────────────────────────┘
```

## Display

### Aspect Ratio

```
┌─────────────────────────────────────────┐
│ Display                                 │
├─────────────────────────────────────────┤
│                                         │
│ Aspect Ratio:                           │
│ ○ Native pixels (256×192)               │
│ ● Correct aspect (4:3)                  │
│ ○ Stretch to window                     │
│                                         │
│ Scaling:                                │
│ ○ Nearest neighbor (sharp pixels)       │
│ ● Integer scaling only                  │
│ ○ Bilinear (smooth)                     │
│ ○ CRT shader                            │
│                                         │
│ Border:                                 │
│ ○ None (screen only)                    │
│ ● Visible border                        │
│ ○ Full overscan                         │
│                                         │
└─────────────────────────────────────────┘
```

### Fullscreen

- F11 or double-click to toggle
- Escape to exit
- Maintain aspect ratio with black bars

## Window Layout

### Default Layout (Running)

After launching from the system launcher:

```
┌─────────────────────────────────────────────────────────────┐
│ emu-c64                                          [─][□][×]  │
├─────────────────────────────────────────────────────────────┤
│ File  System  Media  Input  View  Help                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│                                                             │
│                    ┌─────────────────┐                      │
│                    │                 │                      │
│                    │   EMULATOR      │                      │
│                    │   DISPLAY       │                      │
│                    │                 │                      │
│                    └─────────────────┘                      │
│                                                             │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ [🖴 Drive 8: Game.d64 💡] [🎹 Tape: Empty] [🎮 Port 2]     │
└─────────────────────────────────────────────────────────────┘
```

### Return to Launcher

File → New Session (or Ctrl+N) returns to the launcher screen for that system, allowing variant/option changes without restarting the binary.

### With Media Panel Expanded

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│              ┌─────────────────┐  ┌──────────────────────┐  │
│              │                 │  │ ▶ TAPE               │  │
│              │   EMULATOR      │  │ ┌──────────────────┐ │  │
│              │   DISPLAY       │  │ │ ◉ ══════════ ◉  │ │  │
│              │                 │  │ │  Game.tap       │ │  │
│              └─────────────────┘  │ └──────────────────┘ │  │
│                                   │ [⏮][⏪][▶][⏹][⏩][⏏]│  │
│                                   │ ░░░░░██░░░░░░ 35%   │  │
│                                   └──────────────────────┘  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Debugger Layout (Development/Education)

```
┌─────────────────────────────────────────────────────────────────────┐
│ ┌─────────────────┐ ┌────────────────┐ ┌──────────────────────────┐ │
│ │                 │ │ REGISTERS      │ │ DISASSEMBLY              │ │
│ │   EMULATOR      │ │ PC: $C000      │ │ C000  LDA #$00           │ │
│ │   DISPLAY       │ │ A:  $00        │ │ C002  STA $D020          │ │
│ │                 │ │ X:  $00        │ │ C005  RTS                │ │
│ │                 │ │ Y:  $00        │ │ C006  ...                │ │
│ │                 │ │ SP: $FF        │ │                          │ │
│ │                 │ │ NV-BDIZC       │ │                          │ │
│ └─────────────────┘ └────────────────┘ └──────────────────────────┘ │
│ ┌───────────────────────────────────────────────────────────────┐   │
│ │ 0400: 20 20 20 20 48 45 4C 4C 4F 20 20 20 20 20 20 20  |    HELLO  │
│ │ 0410: 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20  |           │
│ └───────────────────────────────────────────────────────────────┘   │
│ [Step] [Run] [Pause] [Reset]     Breakpoints: $C000               │
└─────────────────────────────────────────────────────────────────────┘
```

## Drag and Drop

### Supported Operations

| Drop Target | Accepted Files | Action |
|-------------|---------------|--------|
| Tape deck | .tap, .tzx, .t64, .wav | Insert tape |
| Disk drive | .d64, .g64, .adf, .adz, .ipf | Insert disk |
| Cartridge slot | .nes, .crt, .rom | Insert cartridge (prompt reset) |
| Main window | Any supported | Auto-detect and insert |
| System picker | Any supported | Launch with that media |

### Visual Feedback

When dragging:
- Valid target highlights (green border)
- Invalid target shows "not allowed" cursor
- Drop zone text: "Drop to insert tape"

### Auto-Detection

Dropping on main window (not specific device):
1. Detect file type from extension/header
2. Find appropriate slot
3. If ambiguous (multiple drives), prompt user
4. Insert and optionally auto-run

## Menus

### File Menu
```
File
├── New Session                Ctrl+N    ← Return to launcher
├── ─────────────────────────
├── Open...                    Ctrl+O
├── Open Recent               →
├── ─────────────────────────
├── Save State               → 1-9
├── Load State               → 1-9
├── ─────────────────────────
├── Screenshot                 Ctrl+P
├── Start Recording            Ctrl+R
├── Stop Recording
├── ─────────────────────────
└── Exit                       Alt+F4
```

### System Menu
```
System
├── Reset (Soft)               Ctrl+R
├── Reset (Hard)               Ctrl+Shift+R
├── ─────────────────────────
├── Pause                      Pause
├── ─────────────────────────
├── Speed                     →
│   ├── 50%
│   ├── 100%
│   ├── 200%
│   └── Unlimited
├── ─────────────────────────
├── Region                    →
│   ├── ● PAL
│   └── ○ NTSC
├── ─────────────────────────
└── Configure...
```

### Media Menu
```
Media
├── Tape                      →
│   ├── Insert...
│   ├── Eject
│   ├── ─────────────
│   ├── Play
│   ├── Stop
│   ├── Rewind
│   └── Fast Forward
├── Drive 8                   →
│   ├── Insert...
│   └── Eject
├── Cartridge                 →
│   ├── Insert...
│   └── Remove
└── ─────────────────────────
```

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| Open file | Ctrl+O |
| Screenshot | Ctrl+P / F12 |
| Fullscreen | F11 |
| Pause | Pause / F9 |
| Soft reset | Ctrl+R |
| Hard reset | Ctrl+Shift+R |
| Save state | Ctrl+1 through Ctrl+9 |
| Load state | Alt+1 through Alt+9 |
| Tape play | Ctrl+F1 |
| Tape stop | Ctrl+F2 |
| Swap joystick ports | Ctrl+J |
| Release mouse | Escape |

## Automation Compatibility

Every UI action has a programmatic equivalent:

| UI Action | CLI | MCP | Rust API |
|-----------|-----|-----|----------|
| Click Play on tape | `tape play` | `tape_play` | `emulator.tape_play()` |
| Drag file to drive | `load --slot drive8 file.d64` | `insert_media` | `emulator.disk_insert()` |
| Press joystick fire | `joy 2 fire` | `joystick` | `emulator.input()` |
| Change speed | `--speed 200` | `set_speed` | `emulator.set_speed()` |

The UI is a visual representation of the same operations the headless modes provide.

## Web Frontend Specifics

WASM builds are **per-system**. Each system is a separate JS/WASM package:
- `emu-spectrum-wasm`
- `emu-c64-wasm`
- `emu-nes-wasm`
- `emu-amiga-wasm`

Embed only what you need. A Spectrum lesson page doesn't download C64 code.

### Launcher in Browser

The launcher becomes the initial HTML/JS UI before loading the WASM module:

```html
<div id="launcher">
  <h1>ZX Spectrum</h1>
  <select id="model">
    <option value="48k">48K</option>
    <option value="128k">128K</option>
  </select>
  <button id="start">Start</button>
</div>
<canvas id="screen" style="display:none"></canvas>

<script type="module">
  import init, { Emulator } from './emu-spectrum-wasm.js';
  
  document.getElementById('start').onclick = async () => {
    await init();
    const model = document.getElementById('model').value;
    const emu = await Emulator.create({ 
      model, 
      canvas: document.getElementById('screen') 
    });
    document.getElementById('launcher').style.display = 'none';
    document.getElementById('screen').style.display = 'block';
    emu.run();
  };
</script>
```

### Canvas Rendering

```javascript
const canvas = document.getElementById('screen');
const emu = await Emu198x.create({ system: 'c64', canvas });

// Frame loop
function frame() {
    emu.runFrame();
    requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
```

### File Input (No Drag-Drop on All Browsers)

```html
<input type="file" id="disk-input" accept=".d64,.g64">
<script>
document.getElementById('disk-input').onchange = async (e) => {
    const file = e.target.files[0];
    const buffer = await file.arrayBuffer();
    emu.diskInsert('drive8', new Uint8Array(buffer), file.name);
};
</script>
```

### Touch Controls

On-screen joystick for mobile:

```
┌─────────────────────────────────────┐
│                                     │
│          EMULATOR DISPLAY           │
│                                     │
├─────────────────────────────────────┤
│                                     │
│    ┌───┐                    ┌───┐   │
│    │ ↑ │                    │   │   │
│  ┌─┼───┼─┐                  │ ● │   │
│  │←│   │→│                  │   │   │
│  └─┼───┼─┘                  └───┘   │
│    │ ↓ │                    FIRE    │
│    └───┘                            │
│    D-PAD                            │
└─────────────────────────────────────┘
```

## Implementation Notes

### UI Framework (Native)

Recommended: **egui** (immediate mode, Rust native, cross-platform)

Alternatives:
- iced (Elm-like, more structured)
- gtk-rs (native look, more complex)
- Tauri (web tech in native shell)

### State Synchronisation

UI runs in main thread, emulator can run in separate thread:

```rust
// Emulator thread sends state updates
let (tx, rx) = channel();

std::thread::spawn(move || {
    loop {
        emulator.run_frame();
        tx.send(EmulatorState {
            frame: emulator.screenshot(),
            tape_position: emulator.tape_position(),
            disk_activity: emulator.disk_activity(),
        }).unwrap();
    }
});

// UI thread receives and renders
loop {
    if let Ok(state) = rx.try_recv() {
        ui.update_display(state.frame);
        ui.update_tape_position(state.tape_position);
        ui.update_disk_led(state.disk_activity);
    }
    ui.render();
}
```

### Input Latency

Input must be low-latency:
- Capture input in UI thread
- Queue for emulator thread
- Process at start of next frame
- Target: <16ms input-to-display
