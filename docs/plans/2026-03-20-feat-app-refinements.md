> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "Unified App Refinements: Debugger Depth, Teaching Tools, and UX Polish"
type: feat
date: 2026-03-20
---

# Unified App Refinements

## Context

The unified app architecture (Phases 1–7) delivered the shell, debugger,
audio visualiser, and compatibility harness. This plan covers the next
layer: deeper debugging tools, Code198x teaching features, and the UX
basics that make the app feel finished.

These are ordered by a combination of impact (how many users/learners
benefit), effort (smallest wins first), and dependency (later items
build on earlier ones).

## Phase 8: Quick Wins (1–2 days)

Low-effort features where the infrastructure already exists.

### 8a: Speed Control UI

The `SetSpeed` command and frame-pacing logic are already in the emu
thread. Add a speed selector to the menu bar or status bar.

- Presets: 0.25x, 0.5x, 1x, 2x, 4x, Uncapped
- Keyboard shortcuts: `-` / `+` for slower / faster, `=` for 1x
- Display current speed in the status bar when not 1x
- Persist last-used speed in config.toml

### 8b: Frame Advance

Step one whole frame (not one instruction). Useful for watching
animation frame-by-frame or finding the right moment to inspect state.

- New command: `StepFrame` (runs `run_frame()` once while paused)
- Button in the debugger toolbar alongside Step Instruction
- Keyboard shortcut: F11

### 8c: Recent Files

Remember the last 10 opened ROMs per system.

- Store in config.toml under `[recent.{system_id}]`
- File > Recent submenu with clickable entries
- Clear Recent option

### 8d: Fullscreen Toggle

- Menu item: Display > Fullscreen
- Keyboard shortcut: F11 or Cmd+F (when not in debugger mode)
- Uses winit `set_fullscreen(Borderless)`

## Phase 9: Debugger Depth (3–5 days)

Standard debugger features that users expect.

### 9a: Watchpoints

Break on memory read or write at a specific address.

- `WatchpointKind`: Read, Write, ReadWrite
- New trait method: `set_watchpoint(cpu_index, addr, kind, enabled)`
- CPU cores check watchpoints during bus access (not just fetch)
- Debugger panel: watchpoint list alongside breakpoint list
- "Break on write to $D020" is the canonical use case

### 9b: Frame Advance with Count

Extend 8b: step N frames at once. Useful for "run 60 frames then pause"
workflows.

- Input field in the debugger: "Run N frames"
- New command: `StepFrames(count)`

### 9c: Hex Editor

Make the memory view editable.

- Click a byte to select it, type hex digits to change the value
- New command: `WriteMemory(cpu_index, addr, value)`
- New trait method: `debug_write(cpu_index, addr, value)`
- Highlight modified bytes in a different colour
- Undo via save state (stretch)

### 9d: Conditional Breakpoints

Break when a condition is true, not just when PC hits an address.

- Condition types: `Register(name) == value`, `Memory(addr) == value`,
  `Register(name) changed`
- Evaluated each instruction step — must be fast
- UI: condition field on each breakpoint entry
- Implementation: lightweight expression evaluator in the emu thread

## Phase 10: Teaching Tools (3–5 days)

Features that directly support Code198x lessons.

### 10a: Trace Logging

Dump CPU execution to a file: one line per instruction with PC, opcode
bytes, mnemonic, and register state.

- New command: `StartTrace(path)`, `StopTrace`
- Format: `PC  BYTES  MNEMONIC  A=xx X=xx Y=xx SP=xx P=xx` (per CPU arch)
- Uses the existing disassembler for mnemonics
- Configurable: registers only, or registers + flags
- Output to file (can be large — millions of lines)
- Teaching angle: "trace your program and find where it goes wrong"

### 10b: Symbol Loading

Load label files from assemblers so the disassembly shows symbolic names.

- Formats: VICE .sym (addr label), ca65 .dbg, RGBDS .sym, vasm .sym
- Common format: one `address label` pair per line
- Loaded per-ROM (matched by hash or filename)
- Disassembly view replaces addresses with labels
- Branch targets show label names
- Teaching angle: "here's MAIN_LOOP, here's DRAW_SPRITE"

### 10c: Cheat / RAM Search

Find memory addresses by value, then narrow by change.

- Step 1: search for a known value (e.g. lives = 3)
- Step 2: change the value in-game, search for the new value
- Step 3: repeat until one address remains
- Bookmark found addresses for quick editing
- Teaching angle: "how does the game store your score?"
- Implementation: snapshot the full address space, diff on each search

### 10d: Code Annotations

User-defined labels and comments on addresses, persisted per-ROM.

- Stored at `~/.emu198x/annotations/{rom_hash}.toml`
- Format: `[labels] 0x8000 = "main" [comments] 0x8010 = "wait for vblank"`
- Disassembly view shows annotations inline
- Right-click an address to add/edit label or comment
- Builds on symbol loading (10b) — loaded symbols are read-only,
  annotations are user-editable

## Phase 11: Input & Recording (2–3 days)

### 11a: Gamepad Support

Physical USB/Bluetooth controllers.

- Use `gilrs` crate for cross-platform gamepad input
- Map gamepad axes/buttons to input ports via the binding system
- Auto-detect connected controllers
- Show connected gamepads in the Input Config panel

### 11b: Video Capture

Record gameplay as video.

- Approach 1: dump PNG frames + WAV audio, user runs ffmpeg
- Approach 2: pipe directly to ffmpeg subprocess (if available)
- Menu: Capture > Start Video Recording / Stop
- Status bar shows recording indicator
- Teaching angle: record demos for lesson content

## Phase 12: Performance & Polish (2–3 days)

### 12a: Profiler

Show which addresses consume the most CPU time.

- Sample the PC every instruction step
- Accumulate hit counts in a HashMap<u32, u64>
- Display as a sorted list: address, hit count, percentage
- Optionally overlay on the disassembly view (heat map)
- Teaching angle: "your sprite routine takes 40% of the frame"

### 12b: Multi-Window

Detach debugger, audio, and chip viewer panels into separate OS windows.

- egui supports multiple viewports in 0.33+
- Each panel gets a "Pop Out" button
- Useful on multi-monitor setups

## Suggested Implementation Order

| Order | Item | Effort | Depends on |
|-------|------|--------|------------|
| 1 | 8a Speed control | Small | — |
| 2 | 8b Frame advance | Small | — |
| 3 | 8c Recent files | Small | — |
| 4 | 8d Fullscreen | Small | — |
| 5 | 9a Watchpoints | Medium | — |
| 6 | 10a Trace logging | Medium | — |
| 7 | 10b Symbol loading | Medium | — |
| 8 | 9c Hex editor | Medium | — |
| 9 | 10c Cheat search | Medium | — |
| 10 | 10d Annotations | Medium | 10b |
| 11 | 9b Frame advance N | Small | 8b |
| 12 | 9d Conditional breakpoints | Large | 9a |
| 13 | 11a Gamepad | Medium | — |
| 14 | 11b Video capture | Medium | — |
| 15 | 12a Profiler | Medium | — |
| 16 | 12b Multi-window | Large | — |

Items 1–4 are quick wins that can land in a single session. Items 5–9
form the core debugging/teaching toolkit. Items 10–16 are polish that
can be deferred or done incrementally.

## Non-Goals

- **Netplay** — out of scope for the foreseeable future
- **TAS tools** — input recording/playback is interesting but niche;
  defer until save states and rewind are solid
- **Plugin system** — static linking is simpler and faster; revisit
  only if binary size becomes a problem at 50+ systems
- **IDE integration** — the MCP server already provides this for
  tooling; the app UI doesn't need to duplicate it
