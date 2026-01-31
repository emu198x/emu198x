# Code Like It's 198x Integration

## Overview

This document describes how Emu198x integrates with the Code Like It's 198x educational platform.

## Use Cases

### 1. Content Creation

Generate screenshots, GIFs, and videos for lessons.

```bash
emu198x-cli \
  --system c64 \
  --region pal \
  --headless \
  --load lesson-05.prg \
  --run-frames 120 \
  --screenshot lesson-05-result.png
```

### 2. Student Code Execution

Run and verify student-submitted code.

```javascript
const emu = await Emu198x.create({
  system: 'c64',
  canvas: document.getElementById('screen'),
  audio: true
});

await emu.loadPRG(studentCode);
await emu.runFrames(60);

const screenshot = emu.screenshot();
const passed = compareImages(screenshot, expectedOutput);
```

### 3. Interactive Examples

Embed emulators in browser for hands-on learning.

```html
<div id="emulator"></div>
<script>
  const emu = await Emu198x.create({
    system: 'spectrum',
    canvas: document.getElementById('emulator')
  });
  
  // Student can modify and run code
  document.getElementById('run').onclick = async () => {
    const code = editor.getValue();
    const binary = await assemble(code);
    await emu.inject(0x8000, binary);
    await emu.run();
  };
</script>
```

## Headless CLI

### Command Format

```bash
emu198x-cli [OPTIONS] [SCRIPT]
```

### Options

| Option | Description |
|--------|-------------|
| `--system` | Target system (c64, spectrum, nes, amiga) |
| `--region` | PAL or NTSC |
| `--headless` | No GUI |
| `--firmware` | Path to firmware/ROM |
| `--load` | File to load (PRG, TAP, D64, etc.) |
| `--inject` | Binary to inject at address |
| `--run-frames` | Run N frames |
| `--run-until` | Run until address/condition |
| `--screenshot` | Save screenshot |
| `--record` | Record video |
| `--script` | JSON script file |

### Script Format

```json
{
  "system": "c64",
  "region": "pal",
  "firmware": {
    "kernal": "kernal.bin",
    "basic": "basic.bin",
    "chargen": "chargen.bin"
  },
  "steps": [
    {"action": "load", "path": "game.d64", "slot": "drive8"},
    {"action": "type", "text": "LOAD \"*\",8,1\n"},
    {"action": "run_frames", "count": 300},
    {"action": "type", "text": "RUN\n"},
    {"action": "run_frames", "count": 60},
    {"action": "screenshot", "path": "output.png"}
  ]
}
```

## WASM API

### Initialisation

```javascript
import { Emu198x } from 'emu198x-wasm';

const emu = await Emu198x.create({
  system: 'c64',           // Required
  canvas: canvasElement,   // Required for video
  audioContext: ctx,       // Optional, creates if not provided
  firmware: {              // Required for some systems
    kernal: kernalArrayBuffer,
    basic: basicArrayBuffer,
    chargen: chargenArrayBuffer
  }
});
```

### Execution

```javascript
// Run continuously (call in requestAnimationFrame)
emu.runFrame();

// Run specific number of frames
await emu.runFrames(60);

// Run until condition
await emu.runUntil({ address: 0xC000 });
await emu.runUntil({ scanline: 100 });

// Step
emu.step();           // One instruction
emu.stepCycle();      // One CPU cycle
emu.stepTick();       // One crystal tick

// Pause/resume
emu.pause();
emu.resume();
```

### Memory Access

```javascript
// Read
const value = emu.peek(0xD020);
const bytes = emu.readMemory(0x0400, 1000);

// Write
emu.poke(0xD020, 0);
emu.writeMemory(0xC000, new Uint8Array([0xA9, 0x00, 0x8D, 0x20, 0xD0]));

// Load PRG (with load address)
emu.loadPRG(prgArrayBuffer);

// Inject at specific address
emu.inject(0xC000, binaryArrayBuffer);
```

### Registers

```javascript
const regs = emu.registers();
// { pc: 0xC000, a: 0, x: 0, y: 0, sp: 0xFF, flags: { n, v, b, d, i, z, c } }

emu.setRegister('pc', 0xC000);
emu.setRegister('a', 0x42);
```

### Input

```javascript
// Keyboard
emu.keyDown('A');
emu.keyUp('A');
emu.typeText('HELLO\n');

// Joystick
emu.setJoystick(2, { up: false, down: false, left: true, right: false, fire: true });
```

### Breakpoints

```javascript
const bp = emu.addBreakpoint({ address: 0xC000 });
const bp2 = emu.addBreakpoint({ address: 0xD020, type: 'write' });

emu.onBreakpoint((bp, state) => {
  console.log('Hit breakpoint at', state.pc);
});

emu.removeBreakpoint(bp.id);
```

### Capture

```javascript
// Screenshot as ImageData
const imageData = emu.screenshot();

// Screenshot as PNG blob
const blob = await emu.screenshotPNG();

// Video recording
emu.startRecording({ audio: true });
// ... run emulator ...
const videoBlob = await emu.stopRecording();
```

### State

```javascript
// Save state (returns Uint8Array)
const state = emu.saveState();

// Load state
emu.loadState(state);
```

## Pixel Aspect Ratios

For authentic display, apply correct aspect ratio:

| System | Native | Display | CSS |
|--------|--------|---------|-----|
| Spectrum | 256×192 | 4:3 | `aspect-ratio: 4/3` |
| C64 PAL | 320×200 | 4:3 | `aspect-ratio: 4/3` |
| C64 NTSC | 320×200 | 4:3 | `aspect-ratio: 4/3` |
| NES | 256×240 | 4:3 | `aspect-ratio: 4/3` |
| Amiga | 320×256 | 4:3 | `aspect-ratio: 4/3` |

## Lesson Verification

### Structure

Each lesson can define verification criteria:

```yaml
lesson: 05-border-colours
system: c64
verification:
  type: screenshot
  setup:
    - load: lesson-05-template.prg
    - inject_at: 0xC000
  run:
    frames: 60
  check:
    - type: memory
      address: 0xD020
      expected: 0x00  # Border should be black
    - type: screenshot
      compare: expected-05.png
      tolerance: 0.01  # 1% pixel difference allowed
```

### Verification Types

#### Memory Check
```yaml
- type: memory
  address: 0xD020
  expected: 0x00
```

#### Register Check
```yaml
- type: register
  register: a
  expected: 0x42
```

#### Screenshot Comparison
```yaml
- type: screenshot
  compare: expected.png
  tolerance: 0.01
```

#### Execution Check
```yaml
- type: execution
  address: 0xC100  # Code must reach this address
  timeout_frames: 600
```

## Rachel Integration

The emulators support networking for Rachel (card game):

### Protocol

Rachel Universal Binary Protocol (RUBP):
- Fixed 64-byte messages
- Simple request/response
- No encryption (handled by transport layer)

### Network Interface

```javascript
const emu = await Emu198x.create({
  system: 'c64',
  network: {
    type: 'tcp',
    host: 'rachel.server.example',
    port: 9000
  }
});
```

The emulator exposes a virtual modem/serial interface that the retro code talks to. The host translates to TCP.

### Multiplayer Demonstration

```javascript
// Create multiple emulator instances
const player1 = await Emu198x.create({ system: 'c64' });
const player2 = await Emu198x.create({ system: 'spectrum' });

// Connect both to same game server
await player1.connect('rachel://game.server/room/123');
await player2.connect('rachel://game.server/room/123');

// Run both in sync
function gameLoop() {
  player1.runFrame();
  player2.runFrame();
  requestAnimationFrame(gameLoop);
}
```

## BIOS/Firmware Requirements

### Legal Considerations

| System | BIOS Required | Notes |
|--------|---------------|-------|
| Spectrum | ROM required | Not freely distributable |
| C64 | ROMs required | Not freely distributable |
| NES | No BIOS | Cartridge contains all code |
| Amiga | Kickstart required | Not freely distributable |

For browser embedding:
- NES can run without user-supplied files
- Other systems require user to provide firmware
- Consider AROS (Amiga) as open-source alternative

### Firmware Loading in Browser

```javascript
// User uploads firmware
const fileInput = document.getElementById('firmware');
fileInput.onchange = async (e) => {
  const file = e.target.files[0];
  const buffer = await file.arrayBuffer();
  emu.loadFirmware('kernal', buffer);
};
```
