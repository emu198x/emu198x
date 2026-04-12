# Brainstorm: Cross-system Shell Requirements

**Date:** 2026-04-07
**Status:** Decided 2026-04-07 — see "Decisions" section below
**Related:** [docs/plans/2026-04-07-feat-spectrum-completeness-plan.md](../plans/2026-04-07-feat-spectrum-completeness-plan.md), [wiki/decisions/product-roadmap.md](../../wiki/decisions/product-roadmap.md)

## Decisions (2026-04-07)

The questions at the bottom of this doc were worked through in conversation and produced the following commitments. The body of the doc is preserved as the analysis that informed them.

**Trait shape:**
- **Addressing is `u64`** throughout the trait. No retro system we'd plausibly emulate needs more than 32 bits, but `u64` is free future-proofing and costs nothing at runtime on 64-bit hosts.
- **Register access is string-keyed**, returning `u64`. Accommodates 68000 32-bit registers, N64 64-bit registers, anything smaller.
- **Framebuffer is borrowed `&[u8]` + `PixelFormat` + optional palette.** Systems expose their native format; frontends convert once.
- **Input is a union `InputEvent` enum** with per-system key code resolution via `key_name_to_code(name: &str)`.
- **Frame timing is `frame_duration_us` (time-based)**, not `frames_per_second`.
- **State hash v1 is postcard-serialise-then-hash.** Every chip and machine already has serde derives, so `state_hash = hash(postcard::to_vec(self))` is a one-liner per system. Postcard's serialisation is deterministic. Phase −1.4 (RZX replay) needs this in the first few sessions and the decision is cheap. A structural (per-chip combined) upgrade is possible later behind the same `state_hash()` method signature without changing any callers.
- **Media kind enum** is `Tape / Disk / Cartridge / Optical / Snapshot`. `Optical` added for CDTV, CD32, and future console support (CD-ROM, GD-ROM, DVD, LaserDisc, Blu-ray — all share a variant; format determined by the file bytes). Hard disks may need a future addition.

**Speed control:**
- **Lives in the shell's frontend layer, not in the `System` trait.** The trait exposes `run_frame()` which always runs one frame at native speed.
- **Audio-preserving time-stretching is a day-one requirement**, not a "maybe later" polish item. Pitch is preserved at all speeds via a pitch-preserving resampler (probably `rubato` crate).
- Preset ratios: 25%, 50%, 100%, 200%, 400%, plus "unlocked" (turbo) and hold-to-turbo hotkey.
- Headless capture always runs at 100% (deterministic).
- MCP has no speed concept — agents call `run_frames(n)` directly.

**Crate naming:**
- **`emu198x-launcher`** (future) for the unified system picker
- **`emu198x-mcp`** as a separate sibling crate to `emu198x-shell`, not a module inside it
- Both extend the `emu198x-{role}` cross-project shell category

**C64 scope:**
- **Full 1541 CPU emulation is the target.** Second MOS 6502 per drive, IEC bit-level bus, G64 disk images, VICE-level compatibility with fastloaders and protected software.
- **Simple 1541 (ROM-hook intercept) is an acceptable fallback** if the timeline forces it. Architecture should support either without a ground-up rewrite — simple is a subset of full.
- **SID implementation:** still open. Either rewrite cycle-accurately from scratch (the Z80 treatment), port/wrap reSID (C++ dependency), or adopt an existing Rust SID crate. Decision deferred until C64 fresh-start brainstorm.

**NES scope:**
- **Mapper coverage:** NROM, MMC1, MMC3 as the minimum for October. More added as time permits. Covers the majority of the Code198x curriculum.
- **PAL vs NTSC default:** NTSC (the common curriculum target).
- **FDS:** deferred, not in October scope.

**Amiga scope:**
- **Chipset target:** OCS, ECS, and AGA — all three.
- **CPU accuracy:** full cycle-perfect 68000. Signal-level, half-cycle state machine, Tom Harte 68000 test suite as the accuracy harness. Same bar as the Z80.
- **6502 accuracy:** same full cycle-perfect bar, for C64 and NES.
- **Timeline:** Amiga is allowed to slip past October if full accuracy forces it. Acknowledged that 68000 tick-level is the biggest engineering commitment in the project and probably requires months of focused work. Shipping Amiga in October at full accuracy is the ideal, slipping with accuracy intact is the second preference, dropping accuracy is not on the table.
- **CD-ROM:** yes, because CDTV and especially CD32 are canonical ECS/AGA machines and CD32 is part of the AGA target.

**ROM distribution:**
- **Spectrum:** Amstrad-blessed ROMs ship with the emulator.
- **C64:** user-supplied. A fallback open-source Kernal/BASIC implementation ships to cover the first-run experience (e.g. the MEGA65 project's OpenROMs). Real ROMs required for full compatibility.
- **Amiga:** user-supplied. AROS Kickstart replacement ships as a fallback. Real Kickstart (via Amiga Forever or other licensed channel) required for full compatibility.
- First-run UX must handle "user ROM not found" gracefully, offer the fallback, and point at where to obtain real ROMs.

## Decisions still open

These were flagged during the conversation but left for their own system-specific brainstorms:

- **SID implementation approach** — defer to C64 fresh-start brainstorm. **Licensing angle to be aware of:** reSID is GPL-licensed, and the two Rust ports I know of (`resid` and `resid-rs`) inherit that license. Adopting any reSID-derived code would take the C64 binary (at minimum) GPL, which is a departure from the project's MIT default. The three paths to weigh in the C64 brainstorm: (a) port/wrap reSID and accept GPL on the C64 binary, (b) find or write an MIT SID implementation, (c) fresh-start from datasheet + hardware testing matching the Z80 identity, which takes months but keeps MIT and the analog filter is the hard part.
- **G64 disk image support level** for the 1541 — defer
- **Hard disk / IDE / SCSI media kind** — may need adding when we tackle Amiga's optional HDD support or Archimedes. Non-breaking enum addition, no current consumer, safe to defer.
- **First-run UX for fallback ROMs** — the actual user experience ("ROM not found, use AROS fallback?") defers to Phase 6 or the native frontend track. Phase 0.2 does include a small architectural hook: the `System::new()` constructor path accepts a `RomSource` sentinel (filesystem path or `Fallback`) so the config file can say `kickstart = "fallback"` and the shell loads the bundled open-source ROM with no special casing in each system's init.

---


## Why this doc exists

The Phase 0.2/0.3 work extracts `emu198x-shell` as the cross-project infrastructure layer and defines the `System` trait that every system implements. The trait is simultaneously the SDL frontend API, the headless runner API, the capture pipeline surface, and the MCP tool surface.

If the trait is designed against Spectrum alone, it will bake in Spectrum assumptions — fixed 50 Hz frame rate, 8-row keyboard matrix, one audio channel, palette-indexed framebuffer — and each subsequent system will either contort itself to fit or force the trait to be rewritten.

The [product roadmap](../../wiki/decisions/product-roadmap.md) commits to **Spectrum → C64 → NES → Amiga** by October 2026. All four systems need to fit the same trait. This doc surveys what each brings to the design and surfaces the questions that must be answered before Phase 0.3 ships.

**Scope:** this is not a spec for C64/NES/Amiga emulation. It's an analysis of what each *requires from the shell* so the trait generalises cleanly. Each system will need its own detailed brainstorm when we get to it.

## The four systems at a glance

| System   | CPU         | Clock       | Video        | Audio           | Media                    | Input                     | Frame rate |
|----------|-------------|-------------|--------------|-----------------|--------------------------|---------------------------|------------|
| Spectrum | Z80         | 3.5 MHz     | ULA          | Beeper + AY     | Tape, disk, snapshot     | Keyboard + Kempston       | 50 Hz PAL  |
| C64      | MOS 6510    | 1 MHz       | VIC-II       | SID             | Tape, disk, cartridge    | Keyboard + 2 joysticks    | 50/60 Hz   |
| NES      | Ricoh 2A03  | 1.79 MHz    | PPU 2C02     | APU (on CPU)    | Cartridge (+ FDS)        | 2 controllers             | 60 Hz NTSC |
| Amiga    | 68000       | 7.09 MHz    | Denise+Agnus | Paula 4-ch      | Floppy                   | Keyboard + mouse + sticks | 50/60 Hz   |

Each row above is already a constraint on the shell trait. There is no single "frame rate," no single "audio shape," no single "input event," no single "media type."

## System-by-system

### Commodore 64

**Chips:**
- **MOS 6510** — 6502 variant with an extra I/O port on zero page. Simpler than Z80 in register count, harder in cycle-exact read/write model (RMW instructions have a "false read" cycle, page-boundary crosses add a cycle, jump indirect has a bug).
- **VIC-II (6569/6567)** — video chip. Runs the clock, asserts the BA ("bus available") line low on badlines and sprite DMA, which halts the 6510 mid-M-cycle after up to three completed write cycles. Every 8 scanlines is a "badline" where VIC-II fetches character data and steals ~40 CPU cycles.
- **SID (6581/8580)** — sound. Three voices with ring modulation, sync, and a four-pole analog filter. The filter is analog and notorious: different batch revisions sound different, reSID's digital model is the reference, but it's a C++ codebase. Writing a cycle-accurate SID from scratch is a well-known rabbit hole.
- **CIA 6526 × 2** — two copies of the complex interface adapter. Timers, serial I/O, port I/O, interrupt sources. CIA1 handles keyboard scanning and joystick 1. CIA2 handles the IEC bus (disk drive) and joystick 2. They have documented quirks ("CIA delay bug") that software depends on.

**Run loop shape:**
VIC-II drives the master clock and arbitrates bus access via BA. This is very close to the Spectrum ULA-drives model in principle — "timing chip owns the clock, CPU is subordinate" — but the *mechanism* is different: the Spectrum withholds the clock edge itself, the C64 asserts a line that halts the CPU via the RDY pin. From a loop perspective they're the same: "tick the video chip, check if the CPU is allowed to tick, tick the CPU if yes."

The system-specific-run-loops decision already says each system writes its own loop. This confirms it — C64 needs `vic_ii.tick()` → `check_ba()` → `cpu.tick_if_ready()`.

**What stresses the shell trait:**

1. **Two joystick ports.** Spectrum has one Kempston. C64 has two independent joysticks wired to CIA1 port A and CIA1 port B (which is also the keyboard scan matrix — reading joystick 2 while a key is pressed causes interference on real hardware). Trait needs to model N joystick ports, not one.
2. **Cartridges as a media category.** Spectrum has tape+disk+snapshot; C64 has tape+disk+cartridge (plus snapshot). Cartridges override ROM/RAM via the GAME/EXROM lines and carry their own state for save states (cart-internal RAM, banking registers). A generic `insert_media(kind, data)` needs to accept cartridges as a distinct kind.
3. **IEC bus is a peripheral bus of its own.** The 1541 floppy drive is a whole separate 6502 computer connected via a four-wire serial bus. "Insert a disk" has two levels:
   - **Simple**: intercept the IEC protocol at the ROM-hook level, serve files from the D64 image directly. Most C64 emulators default to this. Fast, compatible with most software.
   - **Accurate**: emulate a second 6502 running the 1541 ROM, communicate via bit-level IEC timing. Required for fastloader-dependent software and demos that exploit drive-side CPU tricks. Expensive.
   - **Open question for us**: which level for October? Probably simple to start, with the trait designed to allow upgrading later.
4. **SID audio.** Three voices + filter + ring mod + sync is not "AY with different numbers." It's a different synthesis model. The trait's audio surface needs to be generic enough that "emit N samples for this frame" is all the shell sees, with the synthesis details hidden inside the system.
5. **Colour RAM is a separate chip.** 1 KB of static RAM at `$D800`, only the low 4 bits are wired. This is a C64 implementation detail but it means memory-read in MCP's `read_memory($D800, 16)` returns different upper-nibble behaviour on real hardware. Probably invisible to the shell trait.

**What's NOT a problem:**
- CPU instruction set: 6502 is simpler than Z80 to implement to cycle accuracy, and the [product roadmap](../../wiki/decisions/product-roadmap.md) says "6502 tick core exists" from the old codebase.
- VIC-II timing is well-documented (see also CodeBase64, VIC-II writeups from the demoscene).

**Open questions for the user:**
1. 1541 emulation level for October — simple or accurate?
2. SID implementation plan — port reSID, rewrite from scratch, or license a reference impl?
3. Is the old-codebase 6502 tick core in a usable state, or does it need the same fresh-start treatment the Z80 got?

---

### Nintendo Entertainment System

**Chips:**
- **Ricoh 2A03 (NTSC) / 2A07 (PAL)** — a 6502 core with the APU, DMA controller, and controller I/O registers on the same die. The CPU and APU are one chip, one crate.
- **PPU 2C02 (NTSC) / 2C07 (PAL)** — picture processing unit. Has its own 2 KB VRAM, 256 bytes of OAM (sprite RAM), and communicates with the CPU via 8 memory-mapped registers (`$2000-$2007`). Runs at 3× CPU clock on NTSC (5.37 MHz PPU, 1.79 MHz CPU) and 3.2× on PAL.
- **Cartridge + mapper** — the only medium. A NES cartridge contains PRG ROM (CPU-visible), CHR ROM or RAM (PPU-visible), and a *mapper* — a custom chip that implements banking, IRQs, and sometimes extra sound channels. Hundreds of mapper variants exist; mapper coverage is the primary differentiator between NES emulators.

**Run loop shape:**
Different from both Spectrum and C64. There is no "bus arbitration" or "clock gating" — the CPU and PPU run on synchronised clocks with a fixed 1:3 ratio. Each CPU cycle produces 3 PPU cycles; the loop is something like:

```
for _ in 0..cpu_cycles_per_frame {
    ppu.tick(); ppu.tick(); ppu.tick();
    cpu.tick();
    handle_bus();
}
```

Interrupts come from two sources: PPU VBlank NMI (once per frame, starts the game loop) and APU frame IRQ (audio timing). DMA stalls the CPU for 513 cycles during sprite OAM uploads (`$4014` write), which is the NES's version of bus contention.

**What stresses the shell trait:**

1. **Mapper diversity.** A NES system isn't really "the NES" — it's "the NES plus the mapper chip soldered to *this* cartridge." Different mappers have different save state needs (MMC3 has an IRQ counter, MMC5 has fill-mode tiles, etc.). The System trait needs to serialise enough state that *any* mapper's state survives round-trip, which likely means each mapper implementation handles its own serde and the trait just calls through.
2. **Cartridge is the only media.** No tape, no disk (except FDS, which is a special case with its own Ricoh RAM adapter and disk format). The trait's media concept needs to accept "cartridge" naturally and NES needs to declare that's the only kind it accepts.
3. **The APU is on the CPU.** Audio generation is part of the CPU crate, not a separate sibling chip. This is different from every other system — Spectrum has `gi-ay-3-8912`, C64 will have a `mos-sid` crate, but NES has APU inside `ricoh-2a03`. Trait surface doesn't care (still "emit audio samples"), but the crate structure is different.
4. **Controllers are bit-banged.** The NES controller is a shift register. The CPU writes `$4016` to latch state, then reads bits one at a time from `$4016` / `$4017`. Input injection needs to inject at the "button state" level, not at the "bit read" level — the system translates abstract button events into the shift-register behaviour.
5. **DMC channel can corrupt controller reads.** The DMC (sample) channel of the APU does DMA that can glitch controller reads on certain cartridges. Real hardware quirk, some games are sensitive. Not a trait concern but worth knowing.

**What's NOT a problem:**
- PPU timing is well-documented and Mesen is an open reference.
- The 6502 core is shared with C64.

**Open questions for the user:**
1. Mapper coverage target for October — just MMC1/MMC3/NROM (covers most of the curriculum)? Or broader?
2. FDS support — yes, no, later?
3. PAL vs NTSC default?

---

### Commodore Amiga

**Chips (OCS, the simplest chipset):**
- **Motorola 68000** — 16/32-bit CPU, 16-bit data bus, 24-bit address bus. 16 registers (D0-D7 data, A0-A7 address), variable-length instructions, supervisor/user modes, vector-based exceptions. Per the roadmap, "68000 tick-level conversion strategy" is listed as "the largest single risk."
- **Agnus (8361/8371)** — memory controller, DMA arbiter, blitter, copper, interrupts. Owns the chip RAM bus. Allocates DMA slots in each scanline across six consumers (display fetch, sprites, audio, disk, blitter, copper). What's left after DMA is CPU time.
- **Denise (8362)** — video output. Generates the display from the data Agnus fetches. Handles sprites, playfields, colour lookup.
- **Paula (8364)** — audio (4 channels, 8-bit PCM, stereo, DMA-driven), floppy disk controller (reads MFM bitstreams directly), serial port, mouse/joystick port.
- **CIA 8520 × 2** — similar to C64's 6526 but different register map and timing. Handle keyboard, parallel port, and miscellaneous I/O.

**Run loop shape:**
Agnus drives the clock and arbitrates the chip RAM bus. This is the most complex run loop of the four systems because there are **six DMA consumers** competing for bus slots, not one. A scanline is 227 colour clocks (~455 CPU cycles), and Agnus assigns each clock to exactly one consumer using a fixed priority scheme. The CPU gets whatever slots are left after DMA.

Conceptually:
```
for each colour clock in scanline:
    slot = agnus.arbitrate();  // picks highest-priority active DMA
    if slot == CPU:
        cpu.tick_if_aligned();
    else:
        dma_consumer[slot].tick();
    denise.render_pixel();
```

The **copper** is a second "program" that executes in parallel with everything else. It's a tiny coprocessor that runs a display list (the copper list) written by the CPU, and can write chipset registers at specific screen positions — enabling raster bars, per-scanline palette changes, and a lot of Amiga's signature visual tricks. Most demos are copper-driven.

The **blitter** is a programmable 2D graphics engine with its own state machine. It reads up to 3 source channels, applies a 256-term boolean function, and writes a destination channel. Used for everything from scrolling to collision detection. It's a significant DMA consumer and contributes to CPU starvation.

**What stresses the shell trait:**

1. **68000 is not Z80.** Every assumption about "16-bit address space", "8-bit registers", "one register file" breaks. `System::read_memory` needs at least `u32` addressing. Register access via `get_register(name: string)` needs to accept `D0..D7`, `A0..A7`, `PC`, `SR`, `USP`, `SSP`. The MCP register enum is system-specific; the trait should define `get_register(name: &str) -> Option<u64>` and let each system parse the string.
2. **Chip RAM vs Fast RAM.** Amiga has two memory banks: chip RAM (accessible by the custom chipset, but contended with DMA) and fast RAM (CPU-only, uncontended). The save state needs to serialise both. The MCP `read_memory` needs to know which bank — probably by using real 24-bit addresses where `$000000-$1FFFFF` is chip and `$200000+` is fast.
3. **Floppy is MFM, not flat sectors.** Amiga ADF images are 880 KB flat sector dumps; that's the easy path. Some software uses custom MFM encoding (copy protection, Rob Northen protected disks), and proper emulation means decoding the raw MFM bitstream. For October: ADF only, flag the limitation.
4. **Copper state is significant.** The copper has its own PC, two data registers, and a running interpretation of the copper list in chip RAM. Save state has to include copper state. Trait-wise this is just "serialise the system" so no new surface, but it's a non-trivial chunk of state.
5. **Audio is stereo and 4-channel.** Paula mixes 4 channels into stereo output; each channel has independent period, volume, and sample. `audio_samples(out: &mut [f32])` in the trait needs to accept an interleaved stereo buffer, not just mono. Or the trait specifies "channels per frame" and each system fills accordingly.
6. **Mouse is first-class input.** Spectrum has optional Kempston Mouse; Amiga has *expected* mouse, and most software won't work without it. The trait's InputEvent needs `MouseMove { dx, dy }` and `MouseButton { button, pressed }` as fundamental events.
7. **Kickstart ROM version matters.** Amiga software targets specific Kickstart versions (1.3, 2.04, 3.1) and behaves differently on each. This is like model identity but within one system family. `model_id()` returning `"a500-kickstart-1.3"` vs `"a1200-kickstart-3.1"` might be right, or it might need more structure.
8. **Multiple floppy drives.** Up to four via Paula. Multi-disk games are common. Media insertion needs a drive index.

**Accuracy challenges specific to Amiga:**
- 68000 tick-level is hard. The CPU has variable-length instructions and variable memory access patterns. Most 68000 emulators are instruction-level, not cycle-level. The roadmap calls this the biggest risk; it may warrant starting with a coarse model and refining.
- Multi-consumer bus arbitration is a mini-scheduler, not a gate check.
- OCS/ECS/AGA chipset revisions — OCS is the obvious target (A500) but AGA (A1200) is the post-1992 standard. Start with OCS.
- Blitter cycle accuracy matters for demos.
- Copper timing is sensitive — writing to a chipset register at the wrong horizontal position causes visible glitches, and demos exploit this.

**What's NOT a problem initially:**
- Harddrive, CD-ROM, ethernet, graphics cards — all post-OCS extensions, out of scope.
- Workbench UI — irrelevant to the emulator level, it's just software on the Amiga.

**Open questions for the user:**
1. Chipset target — OCS only, or ECS too? AGA is definitely out for October.
2. Model target — A500 only, or A500 + A1200?
3. 68000 accuracy level — tick-level, M-cycle-level, or instruction-level? Per the roadmap this is "the largest single risk."
4. Kickstart ROMs — legal distribution model? Amiga Forever licenses them; we may not be able to ship them.
5. 1541 was the "should we emulate a whole second CPU" question for C64. Amiga's drives are Paula-side — one CPU to worry about. But are we emulating the MFM decoder, or just reading ADF files?

---

## What the union implies for the `System` trait

Pulling together the constraints from all four systems:

### 1. Frame timing is not a constant

Each system has its own frame rate (50 Hz PAL, 60 Hz NTSC) and its own notion of "cycles per frame" (Spectrum Z80 T-states, C64 PHI2 cycles, NES CPU cycles, Amiga 68000 cycles). The trait should expose:

```rust
fn frame_duration_us(&self) -> u32;  // microseconds per frame
fn cycles_per_frame(&self) -> u64;   // whatever the system calls a "cycle"
```

Callers use `frame_duration_us` for real-time pacing and `cycles_per_frame` for replay/scripting precision.

### 2. Framebuffer format must be self-describing

Spectrum (palette u8), Timex hi-res (wider palette u8), C64 (16-colour palette), NES (64-colour palette, sprite-aware), Amiga (up to 4096 colours with HAM mode, per-line palette). The trait returns a `FrameView` struct:

```rust
pub struct FrameView<'a> {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub palette: Option<&'a [u32]>,  // RGBA palette for indexed formats
    pub pixels: &'a [u8],
}

pub enum PixelFormat {
    Palette8,         // 1 byte per pixel, indexes into palette
    Rgba8888,         // 4 bytes per pixel
    // Amiga HAM might need a custom variant
}
```

Callers (SDL blitter, PNG encoder, MCP `capture_screenshot`) convert to their target format based on `FrameView.format`.

### 3. Audio shape must be generic

Beeper (1 ch), AY (1 ch — though mono of 3 voices), SID (1 ch), NES APU (1 ch), Amiga Paula (2 ch stereo, 4 internal voices). The trait:

```rust
fn audio_channels(&self) -> u8;     // 1 or 2
fn audio_sample_rate(&self) -> u32; // 44100 typically
fn audio_samples(&mut self, out: &mut [f32]); // interleaved if stereo
```

The SDL frontend queries channel count once and sets up its audio device accordingly. Headless capture writes to WAV with matching channel count.

### 4. Input is system-defined but expressable as a union

Each system declares its own event set, but the shell provides a common envelope:

```rust
pub enum InputEvent {
    KeyDown(u32),   // system-defined key code
    KeyUp(u32),
    JoystickState { port: u8, state: u16 },  // bitfield
    MouseMove { dx: i16, dy: i16 },
    MouseButton { button: u8, pressed: bool },
    Paddle { port: u8, position: u8 },
    // ...
}

fn inject_input(&mut self, event: InputEvent);
fn key_name_to_code(&self, name: &str) -> Option<u32>;  // for MCP
```

The frontend owns translation from SDL events to `InputEvent`. The system owns translation from `InputEvent` to its internal state (matrix pokes, CIA register writes, NES shift register state, whatever).

### 5. Media is a sum type, not a method per kind

```rust
pub enum MediaKind {
    Tape,
    Disk,       // TRD, D64, DSK, ADF, FDS — system determines format
    Cartridge,  // NES cart, C64 cart, Spectrum Interface 2
    Optical,    // ISO, BIN/CUE, CHD, GDI — CDTV, CD32, future consoles
    Snapshot,   // .z80, .sna, .vsf, save states
}

fn accepted_media(&self) -> &'static [MediaKind];
fn insert_media(&mut self, kind: MediaKind, slot: u8, data: Vec<u8>) -> Result<(), String>;
fn eject_media(&mut self, kind: MediaKind, slot: u8);
```

`slot` lets Amiga have drive 0-3, C64 have device 8-11, NES have no slot distinction. Systems that don't accept a media kind just return an error.

**On `Optical` specifically:** the variant covers every optical-disc medium we might plausibly emulate — CD-ROM (CDTV, CD32, PC Engine CD, Mega CD, PS1, Saturn), GD-ROM (Dreamcast), DVD (PS2, GameCube mini-DVD, Xbox), LaserDisc (arcade ports like Dragon's Lair), Blu-ray (PS3+). One variant, format determined by magic bytes or file extension. This matches the symmetry of the other kinds: `Tape` covers cassette / DAT / whatever; `Disk` covers floppy / hard disk / ZIP; `Cartridge` covers ROM / flash / battery-backed. `Optical` is the same abstraction level.

Near-term usage: CDTV (1991, A500-based) and CD32 (1993, A1200-based AGA) both had CD-ROM drives. Since the Amiga target is OCS + ECS + AGA, CD32 is in scope and `Optical` is a real media type on day one. CD Audio (Red Book tracks) is an Amiga-specific audio source and belongs in the Amiga plan, not the shell trait.

### 6. Memory access is 64-bit addressable

All retro systems on the plausible roadmap fit in 32-bit (max: Amiga 4000 with 128 MB fast RAM = 27 bits), but the trait declares `u64` addressing anyway. Runtime cost is zero on 64-bit hosts, it accommodates any future banking scheme that might want to encode bank numbers in high bits, and it's JSON-serialisation-friendly for MCP tool calls. Systems with 16-bit address spaces just ignore the high bits or reject out-of-range addresses.

```rust
fn read_memory(&self, addr: u64, len: u64) -> Vec<u8>;
fn write_memory(&mut self, addr: u64, data: &[u8]) -> Result<(), String>;
```

### 7. Register access is string-keyed, values are `u64`

Each system has a different register file. Strings let MCP talk to any of them without a compile-time register enum, and `u64` return values accommodate everything from Z80's 16-bit pairs through 68000's 32-bit D/A registers up to hypothetical 64-bit systems.

```rust
fn get_register(&self, name: &str) -> Option<u64>;
fn set_register(&mut self, name: &str, value: u64) -> Result<(), String>;
fn registers(&self) -> &'static [&'static str];  // for introspection
```

Spectrum returns `["AF", "BC", "DE", "HL", "IX", "IY", "SP", "PC", ...]`, NES returns `["A", "X", "Y", "SP", "PC", "P"]`, Amiga returns `["D0".."D7", "A0".."A7", "PC", "SR", "USP", "SSP"]`. MCP serialises the current register set as a JSON object keyed by these names. Values above 2^53 are serialised as strings per JSON numeric precision rules, but that's a hypothetical — no retro system actually hits that.

### 8. Deterministic state hash is required

For RZX replay, save-state round-trip testing, and differential testing, the trait needs a deterministic `state_hash()` that returns the same value for identical state across processes. Simplest implementation: hash the serde-postcard serialisation of the whole system.

```rust
fn state_hash(&self) -> u64;
```

### 9. The peripheral bus lives in `{system}-common`, not `emu198x-shell`

Peripherals are system-specific (Kempston for Spectrum, CIA for C64, NES mapper-as-peripheral is debatable, Amiga CIAs are different). The peripheral bus trait from plan Phase 0.7 stays inside `common-sinclair-zx-spectrum`. C64 will have its own, NES won't really need one, Amiga will have its own.

### 10. Run loops stay per-system

The system-specific-run-loops decision is upheld. The trait exposes `run_frame()` and lets each system implement it however its hardware actually works. The shell never dictates the tick pattern.

### 11. Speed control is a frontend concern, not a trait concern

The `System` trait exposes `run_frame()` which always runs exactly one frame of emulation at native speed. The trait has no concept of "50%" or "turbo." Speed is implemented by the shell's frontend-facing layer (SDL main loop, headless runner) deciding how often to call `run_frame()` and what to do with the resulting frames and audio.

**Audio-preserving time stretching is a day-one requirement.** At 50% speed, playing 50% of the audio samples at normal rate would halve the pitch (unintentional Chipmunks). The shell uses a pitch-preserving resampler — probably the `rubato` crate — to stretch or compress audio to match the user's chosen display speed without changing pitch. At 100% it's a passthrough; at other ratios it's real DSP work but the crate handles it.

**Preset ratios:** 25%, 50%, 100%, 200%, 400%, plus "unlocked" (run as fast as the host CPU allows — typical tape loading acceleration) and a hold-to-turbo hotkey. Configurable via TOML or CLI.

**Headless capture always runs at 100%** — captures are supposed to match real hardware and speed control would defeat their purpose.

**MCP has no speed concept** — agents call `run_frames(n)` which runs N frames of emulation as fast as the CPU allows. Agents don't care about wall clock time.

**The SDL frontend owns the pacer:**

```
loop {
    let frames_this_tick = speed.frames_for_wall_time(elapsed);
    for _ in 0..frames_this_tick {
        system.run_frame();
        // collect frame and audio
    }
    let stretched_audio = resampler.process(audio, speed.ratio());
    audio_queue.push(stretched_audio);
    window.draw(latest_frame);
    sleep_until_next_tick();
}
```

None of that is in the trait. The trait just calls `run_frame()` and hands back a frame's worth of work. The shell-frontend-layer wraps it with speed logic.

---

## Risks and unknowns

**Top risks** (in roughly descending order of severity):

1. **68000 tick-level accuracy is the hardest thing on the roadmap.** If Amiga slips to instruction-level, that's fine for many games but will break demos and timing-sensitive software. The roadmap explicitly flags this as "largest single risk" and names Amiga as the cut candidate. The trait shouldn't care (it still exposes `run_frame`) but the *plan* for Amiga will need a separate conversation.
2. **SID emulation.** reSID is C++. Writing a fresh SID is the C64 equivalent of the Z80 fresh-start — lots of work, high accuracy bar, analog filter is the hard part. Alternatively: find a Rust SID crate (`resid-rs` exists, quality unknown to me).
3. **Mapper coverage for NES.** The difference between "can run Super Mario Bros and Zelda" (NROM, MMC1) and "can run the whole library" (hundreds of mappers) is large. Decision needed on the October target.
4. **IEC bus vs 1541 CPU emulation.** Binary choice with big compatibility implications. Most emulators default to intercept-at-ROM-hooks; accurate emulation is for fastloaders and demos.
5. **Input event envelope design.** The enum above is a guess. Paddles, light guns, rotary controllers, and other weird inputs could force revisions.

**Things I assumed and may be wrong about:**

1. I assumed all four systems can use a `u32` address space. Amiga is 24-bit so that's fine. Future system with banking (e.g. CPC with Gate Array paging) may need `read_memory_banked(addr, bank, len)` or similar.
2. I assumed audio is always `f32` samples. Some systems have natural fixed-point representations that might warrant a different primitive. PCM `f32` is probably still right because every frontend (SDL, WAV, MP3) consumes it.
3. I assumed the palette for indexed framebuffers fits in `&[u32]` (RGBA8). Amiga HAM mode encodes each pixel as "delta from previous" which isn't a palette lookup — it's algorithmic. HAM may need a `PixelFormat::HAM` variant that's processed at blit time.
4. I assumed mapper state serialises cleanly via serde. Most NES mappers have simple state (bank registers, IRQ counters) so this should work, but exotic mappers (MMC5, VRC6+audio) might stress it.

## Questions (answered 2026-04-07)

These are the questions as originally posed, each annotated with the decision. See the "Decisions" section at the top for the consolidated summary.

1. **Frame rate exposure** — `frame_duration_us` (time-based). ✓
2. **Framebuffer handoff** — borrowed `&[u8]` + `PixelFormat` + optional palette. ✓
3. **Input event granularity** — both: `u32` code in the event, with `key_name_to_code(name: &str) -> Option<u32>` on the trait for MCP. ✓
4. **Register access via strings** — yes, string-keyed with `u64` values. ✓
5. **State hashing implementation** — open, prefer structural (per-chip) when we get there but postcard-serialise-then-hash is an acceptable v1.
6. **Media kind enum** — added `Optical` for CDTV/CD32 and future consoles. Final set: `Tape / Disk / Cartridge / Optical / Snapshot`. `Optical` deliberately covers CD / DVD / GD-ROM / Blu-ray / LaserDisc — one variant, format determined by file bytes. Hard disk deferred (may need adding for Amiga HDD or Archimedes).
7. **Cross-system crate naming** — `emu198x-launcher` for the future launcher, `emu198x-mcp` as a separate sibling crate (not a module inside `emu198x-shell`). ✓
8. **1541 emulation level** — full second-CPU ideal, simple ROM-hook acceptable if the timeline forces it. Architecture supports either. ✓
9. **NES mapper coverage** — NROM + MMC1 + MMC3 minimum, broader if feasible. ✓
10. **Amiga chipset + 68000 accuracy** — OCS + ECS + AGA, all three. Full cycle-perfect 68000 matching the Z80 accuracy bar. Amiga allowed to slip past October if required. Same full-accuracy bar for 6502. ✓
11. **Kickstart / Kernal ROM distribution** — user-supplied, with open-source fallbacks (AROS Kickstart for Amiga, OpenROMs-style for C64) shipped with the emulator. Real ROMs needed for full compatibility. ✓

## New risks accepted alongside these decisions

Recording these here so the plan reads honestly about scope:

**68000 full cycle-perfect is the biggest single engineering commitment in the whole project.** Z80 fresh-start was ~3,700 lines; 68000 fresh-start is probably 8-12k lines at a guess, 4-8× the Z80 effort, and needs its own Tom Harte-equivalent test suite (which does exist for 68000). This is the reason Amiga is allowed to slip past October. Accuracy is non-negotiable per project identity; timeline is flexible.

**Full 1541 doubles C64 implementation scope.** A second 6502 instance per drive, 1541 ROM as a separate asset, IEC bit-level bus, G64 disk image support. Not a trait concern but a C64 timeline concern. If the C64 October ship date comes under pressure, simple 1541 is the fallback.

**Audio-preserving time-stretching means a DSP dependency.** `rubato` or equivalent. Not a huge crate but a new one, with non-trivial CPU cost when engaged. The trait stays clean but the shell's frontend layer has real work to do.

## What happens after this doc is resolved

1. User answers the questions above (or marks "defer" on any that aren't load-bearing yet).
2. Phase 0.3 ships the `System` trait with the shape informed by those answers.
3. `common-sinclair-zx-spectrum` implements `System` for the Spectrum family via the `SpectrumSystem` extension.
4. When C64 work begins, it has a trait to implement against that was designed knowing C64's constraints, not retrofitted.
5. The `System` trait is versioned — if C64 or NES exposes something that doesn't fit, the trait grows *with* their arrival rather than being perpetually Spectrum-flavoured.

This doc is itself a working artefact — when we get to the C64 and NES and Amiga fresh-starts, it should be consulted and updated if any of the assumptions above turned out wrong.
