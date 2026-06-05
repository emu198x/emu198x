# Drivability Assessment — Uniform Control Across All Machines

Assessment date: 2026-06-05. Scope: all 28 current machine binaries (`emu198x-*`,
excluding the non-machine `catalogue`, `native-video`, `shell`). This document
records how uniformly each machine can be **driven** — debugged, scripted, and
controlled over MCP — measured against the project goal:

> Drive every machine the same way, regardless of how we interact with it.

It anchors a parity campaign. The findings are data-backed (per-binary source
survey, method in the appendix); the roadmap at the end is the work plan.

## Bottom line

The architecture is sound and the shared abstractions are well-designed — there
is **nothing structural blocking uniformity**. The gaps are *wiring*, not
design. Three things break "drive everything the same way":

1. **The two best machines are the least uniform.** Amiga and Spectrum bypass
   the shared MCP registration (`register_common_tools`) and hand-build bespoke
   surfaces. The Amiga — a mouse-driven Workbench — **cannot be driven by
   keyboard or mouse over MCP at all**; its input is script- and UI-only.
2. **Capture recording is script-only.** Video and streaming-audio recording
   live in the shared script executor (work on all 28 via `--script`) but are
   **absent from MCP** on the 25 non-flagship machines.
3. **Debug verbs are already largely uniform** (corrected 2026-06-05 — an
   earlier draft of this doc was wrong here). 24 machines implement the shared
   `DebugTarget` via the `impl_{6502,z80,6809}_debug_target!` macros, so
   `register_debug_tools` is live, not inert. Spectrum + Amiga sit on a
   *deliberate* bespoke tier (see `knowledge/decisions/debug-surface-tiers.md`).
   The one genuine gap is Game Boy (SM83 has no debug macro yet).

## The shared seam (what's already right)

The control protocol is one vocabulary surfaced through two doors:

- **`ScriptStep`** (`emu198x-shell/src/script.rs`) is the canonical verb set.
  `HeadlessScript::execute()` handles most steps generically for *every*
  machine; the rest fall through to `ScriptError::SystemSpecificStep` for a
  per-binary handler.
- **MCP tools** are derived from the same steps. `register_common_tools()`
  (`mcp_tools.rs`) registers the universal set onto a machine; `register_debug_tools()`
  registers the `DebugTarget`-backed debug set.
- **`InputEvent`** (`host.rs`) already models every input class cleanly:
  `Key` (keyboard), `Button` (digital — joystick fire / console buttons),
  `Axis` (analogue — paddle / proportional), `PointerMotion` + `PointerButton`
  (mouse). Nothing structural blocks any input type.

Steps handled **universally by the shared executor** (work on all 28 via script):
`run_frames`, `run_ticks`, `reset`, `load_media`, `media_transport`, `input`,
`query`, `query_paths`, `wait_for_boot`, `wait_for_query_*`, `load_snapshot`,
`save_snapshot`, **`save_screenshot`**, **`save_audio_capture`**,
**`start/stop_audio_recording`**, **`start/stop_video_recording`**.

Steps that fall through to per-binary handlers (`SystemSpecificStep`):
`press_key`, `type_string`, `autoload_tape`, `load_basic_program`,
`memory_read`, `poke_byte`, `poke_word`, `disasm`, `step`, `run_until_pc`,
`query_cpu`, `query_ay`, `port_read`, `port_write`, `watch_memory_*`,
`watch_ay_*`, `set_machine`.

## Axis-by-axis findings

### 1. Screenshots — ✅ uniform

Shared `execute()` → `session.save_screenshot()` via `FrameSink`. Every machine
renders a framebuffer (render-end-of-frame is a project rule). Exposed via MCP
through `register_common_tools` (25 machines) and bespoke on Amiga/Spectrum.
Works on every machine, both doors. No gap.

### 2. Audio capture — ⚠️ script-uniform, MCP-partial

- **Content is real everywhere:** every machine drives the audio sink, so
  captures are not empty (even near-silent machines like PET / ZX81).
- **Snapshot capture** (`save_audio_capture`): universal — shared executor and
  in `register_common_tools`. Both doors.
- **Streaming recording** (`start/stop_audio_recording`): in shared `execute()`
  (script-universal) but **not in `register_common_tools`**. Only Amiga +
  Spectrum expose it via MCP. The other 25 are script-only.

### 3. Video capture — ⚠️ script-uniform, MCP flagship-only

`start/stop_video_recording` are in shared `execute()` (script works on all 28)
but **absent from `register_common_tools`**. Only Amiga + Spectrum expose video
recording via MCP. 23+ machines can record video via script but not MCP — a
clean script/MCP parity break.

### 4. Media injection — ✅ largely uniform

- `load_media` + `media_transport` in shared `execute()` *and*
  `register_common_tools` — both doors on the common machines. Amiga uses
  bespoke `insert_media` / `eject_media` (also MCP).
- **Types are appropriate per machine:** consoles → `Cartridge`; tape micros →
  `Tape` (ZX80/81, Electron); multi-media → C64 (Cart+Disk+Tape), Spectrum
  (Disk+Snapshot+Tape), Dragon (Cart+Disk+Tape+Snapshot), Amiga (Disk).
- Minor: convenience verbs `autoload_tape` / `load_basic_program` are
  `SystemSpecificStep`; only some binaries (Spectrum) implement them. Generic
  `load_media` works regardless.

### 5. Input injection — ⚠️ the biggest divergence

**Plumbing:** raw `input` is shared-universal (script) and in
`register_common_tools` (25 machines, MCP). But:

- **Amiga registers no input/keyboard/mouse/type MCP tool at all.** The most
  input-dependent machine in the fleet cannot be driven by mouse or keyboard
  over MCP — only via script (`input` events) and the interactive UI.
- **Spectrum** has a rich *bespoke* input MCP surface (`input`, `press_key`,
  `type_string`) — working, but not the shared one.

**Consumption by device** (which machines route each `InputEvent` to silicon):

| Device | Variant | Consumes it | Notable gaps |
|--------|---------|-------------|--------------|
| Keyboard | `Key` | ~26 machines | atari-5200/7800 keypad/console partial (joystick wired 2026-06-05) |
| Joystick | `Button` | 2600, coleco, amiga, c64, dragon, game-boy, nes, sms, sg-1000, spectrum, **atari-5200**, **atari-7800**, **atari-800xl**, **msx**, **vic-20**, **bbc** (fire) (~16) | svi-328, einstein, aquarius, mtx, sord-m5 — blocked on primary source (boot-ROM trace); oric — no native port (Telestrat only) |
| Paddle / analogue | `Axis` | dragon, spectrum, **atari-5200** (POKEY pots, 2026-06-05), **bbc** (μPD7002 ADC, 2026-06-05) | atari-2600 (signature paddles), **c64 (SID POTX/POTY)**, atari-800xl, coleco — **absent** |
| Mouse | `Pointer*` | amiga **only** (correct machine) | MCP-undrivable; c64 1351 minor |

**Atari 5200 and 7800 consume no input at all** — they boot, run cartridges, and
produce audio, but cannot be controlled. Confirm, but the survey shows no
`Key`/`Button`/`Axis`/`Pointer` handling in either.

**Paddle hardware note (corrects an earlier omission):** the Commodore control
ports carry analogue POT lines. The **C64 reads paddles through the SID
(POTX/POTY)** — paddle-capable, just an uncommon peripheral — and the model
already exists in `mos-sid-6581`. The **Amiga reads proportional controllers
through Paula (POT0/POT1)**, modelled in `commodore-paula-8364`. VIC-20 POT
handling is **not** modelled in `mos-vic-i`. So C64 + Amiga paddle/analogue
support is close (the chip seam exists); it needs `Axis` consumers wired.

### 6. Debug verbs — ✅ largely uniform, one genuine gap (corrected)

> **Correction (2026-06-05):** an earlier draft claimed "zero machines implement
> `DebugTarget`; ~17 are registered-but-inert." That was a grep false-negative —
> the impl is generated inside the `impl_*_debug_target!` macros (invoked in the
> *runtime* crate) and the verbs come from the shared `register_debug_tools`, so
> neither shows up as a token in the binary `src`. The real picture is below.

`MachineCore::debug_target()` defaults to `None`, but **24 machines override it**
via `debug_target_hooks!` + an `impl_{6502,z80,6809}_debug_target!` invocation
(13 Z80, 13 6502, 2 6809). So the shared suite (`memory_read`, `poke`, `disasm`,
`step`, `run_until_pc`, `cpu_state`, `io_trace` on Z80) works **uniformly and
identically** on all of them. Three groups:

- **Shared tier — working, uniform (24):** acorn-atom, bbc-micro, electron,
  atari-2600/5200/7800/800xl, colecovision, **c64**, commodore-pet, vic-20,
  **dragon**, jupiter-ace, mattel-aquarius, memotech-mtx, msx, oric-atmos,
  sega-master-system, sega-sg-1000, zx80, zx81, sord-m5, svi-328, einstein.
  (C64 and Dragon are on this tier — an earlier draft wrongly called them
  "absent".)
- **Bespoke tier — richer superset, deliberate (2):** Spectrum, Amiga. They
  hand-build a broader MCP debug surface (Spectrum: AY/tape/snapshots; Amiga:
  copper/blitter/chipset/exec/libraries). This asymmetry is a **binding
  decision** — `knowledge/decisions/debug-surface-tiers.md` — not cruft. Amiga's
  shared-tier opt-in is explicitly deferred until the first 68000 sibling system
  (Atari ST / Mega Drive / Neo Geo / X68000) pays for `impl_68000_debug_target!`.
- **Genuine gap (1):** Game Boy. SM83 has no `impl_sm83_debug_target!` macro yet,
  so it has no debug verbs. The only real debug hole in the fleet.

## Combined parity matrix

| | Capture (shot) | Capture (record) | Media | Keyboard | Joystick | Paddle | Mouse | Debug |
|---|---|---|---|---|---|---|---|---|
| **Script** | ✅ all | ✅ all | ✅ all | ✅ ~26 | ⚠️ ~10 | ❌ 2 | ⚠️ amiga | ✅ 24 shared + 2 bespoke; GB gap |
| **MCP** | ✅ 27 | ❌ flagships only | ✅ all | ⚠️ 25 + Spectrum, **not Amiga** | ⚠️ ~10 | ❌ 2 | ❌ **not Amiga** | ✅ 24 shared + 2 bespoke; GB gap |

## Root causes (all the same shape)

Every gap is "a capability the shell already supports, not surfaced uniformly":

1. Recording steps exist in `execute()` but not in `register_common_tools`.
2. Amiga + Spectrum hand-build MCP surfaces instead of calling the shared
   registration, so they diverge (Amiga *loses* input; both *gain* recording).
3. Joystick/paddle/mouse `InputEvent` consumers are wired ad-hoc per machine.

(The debug surface is *not* a root-cause gap — 24 machines already share
`DebugTarget`; the flagship bespoke tier is a deliberate decision, and Game Boy
is a single missing macro member.)

The fix in every case is to **push capability onto the shared registration and
wire the missing consumers** — not new architecture.

## Parity roadmap (ranked by impact-per-effort)

1. **Add `start/stop_audio_recording` + `start/stop_video_recording` to
   `register_common_tools`.** One shell change → recording becomes MCP-uniform
   across all 25 common machines instantly. Highest ratio.
2. **Bring Amiga (and Spectrum) onto `register_common_tools`.** End the
   snowflake surfaces; Amiga gains the shared `input` tool so it is keyboard-
   *and* mouse-drivable over MCP. Keep their bespoke extras on top.
3. **Wire input consumption for the dead/partial machines:** ✅ Atari
   5200 + 7800 (were input-dead) wired 2026-06-05 — 7800 digital joystick +
   console, 5200 analogue `Axis` + digital + fire. ✅ atari-800xl, **msx**
   (PSG port A), **vic-20** (both VIAs — right on VIA #2 PB7) wired 2026-06-05.
   Remaining digital `Button` machines, re-scoped after a reference pass:
   - **oric-atmos** — *reclassified, not natively capable.* The Atmos has **no
     joystick port**; the Atari-pinout twin ports belong to the **Telestrat**
     (second VIA). Wiring the Atmos would mean modelling one of several
     incompatible third-party interfaces (IJK, etc.) — a product decision, not
     a pin-exposure. Deferred pending a chosen interface.
   - **svi-328, einstein, aquarius, mtx, sord-m5** — *blocked on a primary
     source.* No joystick pin map in `reference/by-system/`, and these are
     donor-sourced machines whose I/O maps the standing note flags as
     possibly-wrong. Each needs a boot-ROM `IN`/`OUT` trace (and the BIOS ROM)
     to confirm the port bits before wiring — heavier than the MSX/VIC-20
     "expose existing pins" shape.
   - **bbc-micro** — ✅ *wired 2026-06-05.* Fire on System VIA PB4/PB5 (active
     low, `Button`); analogue X/Y via a newly-modelled μPD7002 ADC at
     `$FEC0-$FEDF` (channels 0+1 = stick 1, 2+3 = stick 2, 12-bit, `Axis`), with
     end-of-conversion wired to System VIA CB1. ADC modelled inline from the
     `BBCMicro_MiSTer` `upd7002.vhd` reference core.

   Then `Axis` for paddle machines (Atari 2600 paddles, C64 SID paddles) where
   the chip seam already exists.
4. **Debug surface — mostly already done.** 24 machines share `DebugTarget`. The
   only open items are (a) Game Boy: add `impl_sm83_debug_target!` (a new macro
   family member for a shipped single-CPU machine), and (b) Amiga: deferred by
   `debug-surface-tiers.md` until the first 68000 sibling builds
   `impl_68000_debug_target!`. Do **not** fold Amiga in before that path exists —
   that is a listed drift trigger.

Items 1–2 are small, fleet-wide shell wins. Item 3 is per-machine wiring (one
commit each, same cadence as the keyboard campaign). Item 4 is now narrow (Game
Boy macro + the decision-gated Amiga wait), not a fleet campaign.

## Definition of "full parity"

Achieved when, for any machine, an agent can — through *either* `--script` or
`--mcp`, with identical verb names — boot it, inject every input device the real
hardware had, load its media types, capture screenshot/audio/video, and inspect
memory/CPU/disassembly. Debug inspection is met by the shared `DebugTarget` tier
(24 machines) **or** an equivalent-or-richer bespoke surface (Spectrum, Amiga);
both satisfy the floor. Per-machine *extras* (Amiga copper list, Spectrum AY
watch) remain legitimate additions on top.

## Appendix — method

Per-binary source survey under `crates/`. Capture/media/input dispatch read from
`emu198x-shell/src/script.rs` (`execute()`), `mcp_tools.rs`
(`register_common_tools` / `register_debug_tools`), `host.rs` (`InputEvent`),
`debug.rs` (`DebugTarget`), `machine.rs` (`debug_target` default). Input
consumption detected by `InputEvent::{Key,Button,Axis,PointerMotion,PointerButton}`
matches across `runtime-*` / `machine-*`. Audio-sink usage by `audio_sink` /
`AudioPacket`. Media types from declared `MediaKind` per machine. Debug state
from `register_debug_tools` callers cross-referenced against `impl DebugTarget`
(none found).
