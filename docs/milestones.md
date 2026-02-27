# Milestones (v1-focused, capture-first)

## Purpose

This roadmap is optimised to ship **Code Like It’s 198x v1** with:

- Undeniable technical credibility  
- High-quality, reproducible captured artefacts (images, video, audio)  
- At least one complete, teachable path per core system  

The emulators are **first-class projects**, but during v1 they are treated as  
**instrumentation and content-production engines**, not general-purpose player emulators.

Emulator work beyond v1 is explicitly valuable, but **non-blocking**.

---

## Core Principles

- **Capture first**: emulator features must enable or improve observable, repeatable artefacts.
- **Demonstrable > complete**: systems are “done” when they can teach and show truth, not when they run most commercial software.
- **Observability is non-negotiable**: internal state must be inspectable.
- **Determinism beats convenience**: repeatable runs matter more than realism in v1.

---

## Status Overview

| Track   | Description                            | Status      |
|---------|----------------------------------------|-------------|
| Track A | Foundation & Instrumentation           | In progress |
| Track B | System Demonstrability (v1)            | Not started |
| Track C | Completeness & Compatibility (Post-v1) | Sealed      |

---

## Track A: Foundation & Instrumentation (v1-blocking)

These milestones define the project’s **differentiator**. They are required for v1.

### M1: Project Scaffolding ✅

Rust workspace with clear crate boundaries.

**Verification:**

- `cargo build` succeeds  
- `cargo test` runs  

---

### M2: Z80 CPU Core ✅

Cycle-accurate, observable Z80 implementation.

**Verification:**

- ZEXDOC + ZEXALL pass  
- `tick()` advances one T-state  
- Registers, flags, timing exposed  

---

### M3: 6502 CPU Core ✅

Per-cycle 6502 with illegal opcode coverage.

**Verification:**

- Klaus Dormann tests pass  
- Decimal mode correct  
- `tick()` advances one CPU cycle  
- Observable CPU state  

---

### M4: 68000 CPU Core ✅

Foundational 68000 implementation for Amiga.

**Verification:**

- Core instruction execution  
- Address modes verified  
- Exceptions handled  
- Observable CPU state  

---

### M37: Observability & Capture Infrastructure ✅

First-class introspection across all systems.

**Verification:**

- Query CPU registers  
- Query memory ranges  
- Query video and audio chip state  
- Breakpoints  
- Step-by-tick execution  

---

### M38: Control Server (MCP) ✅

Programmatic control of all emulators.

**Verification:**

- Boot/reset  
- Media insertion  
- Run/pause/step  
- Screenshot capture  
- Input injection  

---

### M39: Headless Scripting ✅

Automation without a GUI. All four systems accept `--script <file.json>` which reads
a JSON array of simplified RPC requests, dispatches each through the MCP server, writes
JSON-line responses to stdout, and saves screenshots/audio to disk via `save_path` params.

**Verification:**

- ✅ JSON command protocol (`ScriptStep` struct, sequential ID assignment)
- ✅ Batch execution (`run_script()` on all four MCP servers)
- ✅ Deterministic capture of video/audio (`save_path` decodes base64 to disk)
- ✅ Integration with Code Like It’s 198x pipeline (headless, no window)

---

## Track B: System Demonstrability (v1-blocking)

A system is **Demonstrable** when it can:

- Boot deterministically  
- Run a known-good or purpose-built program  
- Produce stable video and audio  
- Expose internal state for inspection  
- Support scripted, repeatable capture  

Broad commercial compatibility is **explicitly not required** for v1.

---

### ZX Spectrum — Demonstrable (v1)

#### Required for v1

- Memory map (48K)
- ULA basic video (bitmap + attributes)
- Accurate contention timing
- 1-bit beeper audio

#### Optional for v1

- Keyboard input
- Tape loading (instrumented)

#### Explicitly deferred

- 128K models
- AY sound
- Broad TOSEC compatibility targets

**v1 Exit Criteria:**

- One timing-sensitive visual demo
- One hero screenshot
- One audio capture
- One complete lesson draft

---

### Commodore 64 — Demonstrable (v1)

#### Required for v1

- Memory map and banking
- VIC-II basic video
- Badline timing
- SID audio (core features)

#### Optional for v1

- Hardware sprites
- CIA timers

#### Explicitly deferred

- Full 1541 drive emulation
- Broad D64 compatibility targets

**v1 Exit Criteria:**

- Clear visual explanation of badlines
- Recognisable SID audio example
- One hero visual
- One complete lesson draft

---

### NES/Famicom — Demonstrable (v1)

#### Required for v1

- Memory map
- PPU background rendering
- PPU sprites
- Controller input
- Mapper 0 (NROM)

#### Optional for v1

- Mapper 1 (MMC1)

#### Explicitly deferred

- MMC3 and IRQ-heavy mappers
- Broad compatibility metrics

**v1 Exit Criteria:**

- One pipeline-focused visual demo
- One captured sprite/timing example
- One complete lesson draft

---

### Amiga — Demonstrable (v1)

#### Required for v1

- Memory map
- DMA + Copper basics
- Bitplane video
- Paula audio

#### Optional for v1

- Simplified disk loading

#### Explicitly deferred

- Full floppy realism (MFM edge cases)
- Broad game compatibility

**v1 Exit Criteria:**

- One Copper/Blitter visual demo
- One audio DMA example
- One hero capture
- One complete lesson draft

---

## Track C: Completeness & Compatibility (Post-v1, sealed)

These milestones remain valuable but **cannot block v1 launch**.

Includes (non-exhaustive):

- Spectrum 128K and AY audio
- Full tape/disk realism
- NES mapper expansion
- Amiga full chipset fidelity
- Broad TOSEC compatibility targets
- Polished emulator UI
- Web frontend via WASM

Work in this track resumes **only after**:

- Code Like It’s 198x v1 ships
- At least one lesson per system is public

---

## Stop Clause (Important)

> Emulator development may continue after v1, but **Code Like It’s 198x lessons and site content become the primary driver of further emulator work**.

This clause exists to be argued with — and noticed when broken.

---

## Final Note

This roadmap does not reduce ambition.

It makes ambition **pay rent** by producing artefacts, lessons, and public proof before pursuing completeness.
