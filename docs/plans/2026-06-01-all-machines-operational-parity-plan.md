---
title: "feat: All-machines operational parity — capture, script, MCP, borders"
type: feat
date: 2026-06-01
---

# All-Machines Operational Parity Plan

A sequenced plan to bring the 40 currently-extracted machines up to
the same operational level as Spectrum and Amiga: screenshot + audio
+ video capture, `--script` programmatic control, and MCP server
parity. Plus the parallel **borders** workstream — most chips
currently render active-area only, not the canonical TV/CRT border.

## Executive summary

After the 2026-05 → 2026-06-01 donor harvest, current Emu198x has
**42 machine crates** spanning most of the 1977-1995 home-computer
+ console scene. Only Spectrum and Amiga have full operational
parity (capture pipeline + script + MCP); the other 40 stop at
"`main.rs` runs ROM and writes a screenshot" (~200 LoC per crate).

This plan extends the existing `emu198x-shell` + `emu198x-mcp`
infrastructure (per the 2026-04-07 Spectrum completeness plan)
across the remaining 40. The cost per machine should be ~200-400
LoC of per-machine wiring, not ~4,400 LoC of duplicated runner code.

Plus the **borders** workstream — a chip-level concern, separate
from the runner architecture. Survey shows most chips define their
framebuffer at active-area dimensions only (TMS9918 256×192, NES
PPU 256×240, TIA 160×…) and don't render the canonical CRT border.
Real hardware had borders; user spotted this immediately when
inspecting screenshots from the harvest. ~12-15 chip crates affected.

The two workstreams are independent and can proceed in parallel.
Borders pay off the moment a screenshot is taken; operational
parity pays off in debuggability and in the ability to drive the
machine via script or MCP for regression testing.

## Goal: what "Spectrum-level operational parity" means

A machine is **operationally complete** when its runner provides:

1. **Capture**
   - PNG screenshot at any frame boundary
   - WAV audio capture (mono or stereo as appropriate)
   - Video record (frame sequence to file — format TBD per existing
     Spectrum convention)
2. **Script** (`--script PATH`)
   - Run-frames / run-until-pc / run-until-mem-change
   - Press-key / release-key / type-string (where keyboard exists)
   - Joystick / gamepad / button input (where input devices exist)
   - Memory read / write / search / scan
   - Snapshot / restore (where the machine supports it)
3. **MCP server** (`--mcp` or `--mcp-stdio`)
   - All script-level capabilities exposed as MCP tools
   - Per-chip `query()` paths surfaced (CPU regs, chip state)
   - Memory inspection + manipulation tools
   - Disassembly + step-instruction tools (where the CPU supports it)

Each capability has a working acceptance test (gated like the
existing boot smokes — `#[ignore]` until media is present).

## Architectural premise

**Leverage `emu198x-shell` rather than recreate.** It already
provides:

- The `System` trait (per the 2026-04-07 plan's "Addressing is
  `u64`, register access string-keyed with `u64` values" scope
  decision)
- Save-state framework
- Capture pipeline scaffolding
- Path resolution + media-kind enum

`emu198x-mcp` already provides the MCP server scaffolding.

What's needed per machine:

| Layer | Existing pattern (Spectrum) | Per-machine work |
|---|---|---|
| `System` trait impl | `crates/emu198x-spectrum/src/machine.rs` | Implement for each machine — ~200-300 LoC |
| Script handlers | `crates/emu198x-spectrum/src/script/` | Generic handlers in shell; per-machine input map only |
| MCP tool set | `crates/emu198x-spectrum/src/mcp/tools.rs` | Generic tools in shell; per-chip `query()` paths only |
| UI runner | `crates/emu198x-spectrum/src/ui/` | Probably wholly shareable — TBD pilot |
| `main.rs` | `crates/emu198x-spectrum/src/main.rs` | Stays per-machine but ~100 LoC after extraction |

If the pilot (see Phase 2 below) shows that some Spectrum-specific
bits in `emu198x-shell` need generalizing — e.g. the audio mixer
assumes Spectrum's AY + beeper pair, the input map assumes a
half-row matrix — that generalization happens before rollout, not
during it.

## Scope decisions

- **In scope**: all 42 machine crates. Yes including the ones that
  currently don't reach READY (PET, MTX, VIC-20, BBC Micro without
  BASIC) — operational parity is what lets us *debug* the boot path.
- **In scope**: borders for all chips where the real hardware had
  one and we're currently rendering active area only.
- **Out of scope**: render-fidelity work beyond borders (e.g. ANTIC
  scan-line accuracy, GTIA player/missile collisions, TIA HMOVE
  edge cases). Those stay as per-machine follow-ups in
  `docs/status/outstanding-work.md` and are made tractable by the
  operational-parity work, not subsumed by it.
- **Out of scope**: new machines. We have 42; ship parity on those.
- **Out of scope**: BIOS sourcing for ROM-pending machines (Jupiter
  Ace, Atom, Oric, SVI-328). Those stay as their own follow-ups.
  Operational parity does not require ROM presence.

## Phase 1 — Borders (parallel, chip-level)

Pre-work survey + per-chip border fix. Independent of operational
parity; proceeds in parallel.

### 1.1 Survey

Document per chip whether it renders active-area-only or full TV
output, with real-hardware border dimensions. Output:
`docs/plans/2026-06-01-borders-survey.md` (one table row per chip).

### 1.2 Fix list (chip → real border dimensions)

Working hypothesis from the LoC survey already done:

| Chip | Current FB | Border (TBC) | Real frame |
|---|---|---|---|
| `atari-tia` | 160 × ~210 | yes (substantial overscan) | ~228 × 262 colour clocks |
| `atari-gtia` | 320 × 240 | already includes some? | need to confirm |
| `atari-maria` | 320 × 240 | already includes some? | need to confirm |
| `ti-tms9918` | 256 × 192 | yes | 342 × 262 (NTSC) / 342 × 313 (PAL) |
| `sega-vdp` | 256 × 192 | yes | 342 × 262 / 342 × 313 |
| `ricoh-ppu-2c02` | 256 × 240 | overscan top/bottom | 256 × 240 active, ~256 × 224 TV-visible |
| `motorola-6845` (PET) | 320 × 200 | yes | 400 × 312 (PET 80-col) |
| `motorola-vdg-6847` (Atom, Dragon) | 256 × 192 | yes | 256 × 192 active + border |
| `sinclair-zx81-ula` | 320 × 240 | **already has 32×24 border** ✓ | — |
| Spectrum ULA family | — | already has border ✓ | — |
| `commodore-vic-i` (inline in VIC-20) | 176 × 184 | yes | larger |
| `commodore-paula-8364` / OCS denise | — | overscan-aware ✓ | — |

Numbers above are working estimates — must be sourced from each
chip's datasheet in `~/Projects/198x/reference/by-system/` before
implementation.

### 1.3 Per-chip work

For each chip with `border == yes`:
1. Bump `FB_WIDTH` / `FB_HEIGHT` constants to include border.
2. Extend renderer to draw border colour in border regions.
3. Adjust active-area pixel placement so existing screenshots stay
   sensible (or accept that they'll shift and update the gated
   smoke assertions).
4. If the chip has a programmable border colour (Spectrum BORDER,
   TIA COLUBK, Atari COLBK), wire it.

### 1.4 Per-machine integration

Each machine that consumes the chip picks up the new framebuffer
dimensions automatically. Where the machine has its own renderer
wrapper (Atari 2600 / 5200 etc.), the wrapper needs the matching
update.

### 1.5 Acceptance

- Visual: TOSEC boot screenshot for each machine shows border
- Test: `cargo test --workspace` stays green (framebuffer-size
  assertions in unit tests update accordingly)

## Phase 2 — Pilot the runtime extension

Pick 2 representative machines, take them to full operational
parity, validate the architecture survives contact with non-
Spectrum / non-Amiga shapes. **This is a gating phase** — if the
architecture needs reshaping, better to discover it on 2 machines
than 40.

### 2.1 Pilot selection

Candidates by criterion:

- **Need a 6502-based machine** (Z80 covered by Spectrum; 68000
  covered by Amiga) → pick **NES** (mature, real software runs,
  cycle-accurate)
- **Need a TMS9918-based machine** (shared chip across 6 systems)
  → pick **MSX** (boot-to-BASIC, easy to verify, unblocks
  ColecoVision / SG-1000 / SMS / MTX / Aquarius downstream)

### 2.2 Per-pilot work

For each pilot machine:
1. Implement `System` trait for the machine (~200-300 LoC)
2. Wire script handlers via shell-generic + per-machine input map
3. Wire MCP tools via shell-generic + per-chip `query()` paths
4. Wire capture pipeline (screenshot + WAV + video record)
5. Acceptance test: drive the machine via script (load ROM, press
   keys, capture state); drive same flow via MCP

### 2.3 Architecture review checkpoint

After both pilots: review for shape regressions, generalize any
Spectrum-specific assumptions in `emu198x-shell` if surfaced, log
the deltas as decision records.

## Phase 3 — Tier rollout

After the pilot validates the architecture, roll forward by tier.
Each tier completes before the next begins.

### 3.1 Boot-and-play tier

Machines where real software runs end-to-end today. The pay-off is
immediate (better debuggability for the most-used systems):

- C64, Atari 2600, ColecoVision, SG-1000, Sega Master System
- Mattel Aquarius, Acorn Electron, Dragon-32
- Spectrum variants (16k, +2a, +2b, +3, Timex 2048/2068,
  Pentagon 128, Scorpion ZS256) — should mostly inherit from
  Spectrum 48/128 with thin per-variant wiring
- Game Boy, NES (latter already piloted)

### 3.2 ROM-loads + render-pending tier

Machines that boot but display is incomplete. Operational parity
is exactly what's needed to debug them:

- Atari 5200, 7800, 800XL, MTX, VIC-20, PET, BBC Micro
- Tatung Einstein (VDP-init only — needs WD1770), Sord M5 (needs
  Z80 CTC)
- ZX80, ZX81

### 3.3 Awaiting-ROM tier

Machine compiles, runner runs but ROM not sourced. Operational
parity lands first; ROM sourcing follows as the per-machine
follow-up:

- Jupiter Ace, Acorn Atom, Oric Atmos, Spectravideo SVI-328

### 3.4 Per-machine cost estimate

Boot-and-play tier (~14 machines): ~250 LoC × 14 = ~3,500 LoC
ROM-loads tier (~11 machines): ~250 LoC × 11 = ~2,750 LoC
Awaiting-ROM tier (~4 machines): ~250 LoC × 4 = ~1,000 LoC

Plus per-tier review + commit overhead. Realistic sessions:
- Phase 1 (borders): 2-3 sessions
- Phase 2 (pilot): 1-2 sessions
- Phase 3.1: 3-4 sessions
- Phase 3.2: 2-3 sessions
- Phase 3.3: 1-2 sessions

Total: ~10-14 sessions for full parity across 40 machines.

## Risks and gotchas

- **`emu198x-shell` may have hidden Spectrum-specific assumptions.**
  Phase 2's whole point is to surface these before scaling. Don't
  start Phase 3 until the pilots are clean.
- **MCP tool surface area.** Spectrum exposes ~30 MCP tools. Some
  are chip-specific (Spectrum AY, Amiga Paula). The shared shell
  should expose only the generic ones; per-machine MCP modules
  add the chip-specific tools.
- **Borders + per-machine renderer interaction.** Some machines
  wrap the chip framebuffer in their own pipeline (Atari 5200's
  ANTIC + GTIA composite; PET's CRTC + char-ROM render). Border
  changes need testing per machine, not just per chip.
- **Cycle-accuracy regressions during border work.** Adding border
  rendering means more pixels per scanline — performance hit.
  Should be invisible in headless mode but worth a benchmark on
  Spectrum (already at full parity) before / after.
- **Spectrum + Amiga rebaselining.** When shell-generic capabilities
  expand, Spectrum and Amiga should keep working. Run their
  existing acceptance tests after each shell extension.
- **Don't get sucked into per-machine render fidelity during
  parity work.** That's the *next* thing the parity unlocks, not
  this work. Resist scope creep — file as
  `docs/status/outstanding-work.md` entries instead.

## Acceptance per machine

A machine is **done** for the purposes of this plan when:

1. `cargo run -p emu198x-{machine} -- --screenshot foo.png` produces
   a screenshot **with border**.
2. `cargo run -p emu198x-{machine} -- --capture-audio foo.wav --frames 600`
   produces audio at the canonical sample rate.
3. `cargo run -p emu198x-{machine} -- --record-video foo.{ext} --frames 600`
   produces video.
4. `cargo run -p emu198x-{machine} -- --script test.script` runs a
   smoke script (canonical example per machine: press a key, run
   N frames, screenshot, verify).
5. `cargo run -p emu198x-{machine} -- --mcp-stdio` launches an MCP
   server whose tool list includes:
   - `query_cpu`, `query_chipset` (or similar — exact tool names
     match the per-machine surface)
   - `memory_read`, `memory_write`, `memory_scan`
   - `screenshot`, `start_video_recording`, `stop_video_recording`
   - `start_audio_recording`, `stop_audio_recording`
   - `step`, `run_frames`, `run_until_pc`
   - (per-machine extensions: `press_key`, `type_string`,
     `load_media`, etc.)
6. An MCP-driven gated test exists at
   `crates/machine-{machine}/tests/mcp_smoke.rs` (or similar) that
   exercises the surface end-to-end.

A machine is **partially done** if 1-3 pass but 4-6 don't — useful
as a checkpoint, not as a stopping point.

## Tracking

This plan does not maintain a per-machine status table here — that
lives in `docs/status/outstanding-work.md` § operational-parity
(to be added once the plan starts moving). Update outstanding-work
as each machine lands, not this document.

## Related decisions and prior work

- `docs/plans/2026-04-07-feat-spectrum-completeness-plan.md` —
  introduced `emu198x-shell` and the System trait, with the
  October must-haves (headless, capture, scripting, MCP) already
  landed for Spectrum
- `knowledge/decisions/crate-naming.md` — `emu198x-*` is for
  cross-project infrastructure; `common-*` for system-family
  runtime; `machine-*` for the machine itself
- `knowledge/decisions/aga-donor-reference-only.md` and
  `knowledge/decisions/older-reference-only.md` — both archive
  codebases reference-only; no more donor harvesting
- `docs/status/outstanding-work.md` — per-machine state today;
  this plan adds operational-parity as a tracked dimension
