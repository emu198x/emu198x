# Decision: NES architecture review — tighten the seams, not the spine

**Date:** 2026-05-20
**Status:** In progress — Seams 1 (partial), 2, 4 landed 2026-05-20/21

## What this is

A targeted review of the NES implementation against the load-bearing decisions already in place ([NES clock topology](nes-clock-topology.md), [CPU bus interface](cpu-bus-interface.md), [Within-family layering](within-family-layering.md), [Runtime internal shape](runtime-internal-shape.md), [Save state format](save-state-format.md)). The NES is the third engineering-bar system and is materially further along than the C64 in coverage breadth — 14 mappers including MMC3/MMC5, nestest 100%, Super Mario Bros. boots and renders, 627/629 local ROMs run for 300 frames — but the same five-seam pattern applies. The catalogue *cannot* catch by construction: silently-locked-in wrong PPU pixel-pipeline timing, lost host input, volatile state that doesn't survive snapshot restore across 14 stateful mappers, oracle integrity, and standing boot-invariant assertions.

The spine stays. The seams need work.

This document mirrors [`spectrum-architecture-review.md`](spectrum-architecture-review.md), [`amiga-architecture-review.md`](amiga-architecture-review.md), and [`c64-architecture-review.md`](c64-architecture-review.md). The structure is deliberately the same so the four reviews can be read side by side. The seam findings are grounded against the canonical NES test-ROM corpus (Kevin Horton's `nestest`, Shay Green's `blargg_*` series, Tepples' `mmc5test` and `oam_stress`, NESdev wiki cycle tables) and the open NES implementations (Mesen2, FCEUX, Nintendulator) that already serve as cross-validation references in this project.

## What we are *not* changing

These decisions are load-bearing, validated by:

- 6502 100% Tom Harte (2 × 2.47M instruction vectors)
- nestest 100% golden-log match (8,991 / 8,991 instructions)
- 14 mappers passing in-codebase tests
- Super Mario Bros. rendering end-to-end with audio
- 627/629 local-archive ROMs running 300 frames clean
- Snapshot round-trip working for every chip + concrete mapper state

Nothing in this review revisits these:

- **Master oscillator drives the loop** ([nes-clock-topology.md](nes-clock-topology.md)). PPU ticks every dot; CPU ticks every 3rd dot (NTSC) / every 3.2nd average (PAL). The crystal is the only time anchor. The previous (pre-rewrite) CPU-driven loop could not pass blargg's vbl_nmi / sprite_hit / odd-frame-skip; this loop can in principle.
- **Pin-level CPU bus interface, no Bus trait.** `mos-6502` (configured as 2A03 via `new_2a03()`, BCD disabled) exposes `addr`, `data`, `data_in`, `rw`, `sync`, `irq`, `nmi` as fields. The machine layer reads pins between ticks and dispatches reads/writes through the address-space decoder (RAM mirroring, PPU registers at `$2000-$3FFF` with $8 mirroring, APU+IO at `$4000-$4017`, mapper PRG at `$4020-$FFFF`).
- **Mapper as `Box<dyn Mapper>` trait object.** One concrete struct per mapper revision. The trait exposes `cpu_read` / `cpu_write` / `ppu_read` / `ppu_write` / `irq_pending` plus mapper-specific A12-rise notification (needed by MMC3/MMC5 IRQ counters). Mapper state is serde-skipped at the chip layer and reconstructed by re-parsing the iNES header on restore.
- **One bus operation per CPU tick.** The 6502 advances exactly one M-cycle per CPU clock. Multi-cycle instructions take the right number of CPU ticks because each one consumes one bus operation. OAMDMA at `$4014` stalls the CPU for 513 / 514 cycles via the machine layer's stall counter (not via injecting fake CPU cycles).
- **DMC DMA cycle stealing in the machine layer.** When the APU's DMC unit requests a sample fetch, the machine steals exactly one CPU cycle (or 2-4 depending on alignment). The stall doesn't reach into the CPU's internal state; it's a one-cycle pause at the bus level.
- **NMI / IRQ as public pin fields.** The PPU's `nmi` flag is set at scanline 241 / dot 1 (with a 2-dot pipeline delay to model the `$2002` PPUSTATUS clear-on-read race the blargg vbl_nmi tests exercise). The machine layer reads `ppu.nmi` and `mapper.irq_pending() || apu.irq_pending()` at the CPU's clock edge and wires them to `cpu.nmi` / `cpu.irq`.
- **iNES parsing in `format-nintendo-nes-ines`.** The 14 supported mappers all live as siblings under `src/mappers/`. The crate is the source-of-truth for "how to construct the right mapper from a ROM"; the machine layer only sees `Box<dyn Mapper>`.

If the review below appears to require revisiting any of these, the review is wrong and the decision wins.

## The five seams

### Seam 1 — PPU dot-level rendering pipeline

**Current state.** `crates/ricoh-ppu-2c02/src/lib.rs` is 2773 lines implementing the per-dot PPU state machine: background tile fetches (NT byte, AT byte, low pattern byte, high pattern byte across 8 dots per tile), sprite evaluation (secondary OAM build during dots 1-64, sprite fetches during 257-320), pixel mux with priority and sprite-0 detection, scroll register evolution, the `t/v` loopy register dance on `$2005` / `$2006` writes, and the NMI / VBL flag state machine. `nestest` passes (CPU only — doesn't exercise PPU rendering); local-archive ROMs survive 300 frames; Super Mario Bros. renders. None of those touch the dot-level edge cases.

**Friction observed.** Three categories of latent timing bugs not caught by anything currently in CI:

1. **NMI delivery edge cases** — the canonical blargg `vbl_nmi_timing` suite has 8 sub-tests for the exact dot-cycle interaction between PPUSTATUS clear-on-read (`$2002` bit 7) and the NMI line. A one-dot-early NMI breaks "polled VBL" games (early-era titles); a one-dot-late NMI breaks NMI-driven sample players. Our 2-dot NMI pipeline delay in `nes-clock-topology.md` was the right structural fix; whether the *exact* dot of assertion matches blargg's expectations is unverified.

2. **Sprite-0 hit and overflow** — sprite-0 hit at the precise dot where a non-transparent background pixel overlaps a non-transparent sprite-0 pixel. Games (notably *Battletoads*) use sprite-0 hit to time mid-frame raster effects to single-scanline precision. The blargg `sprite_hit_*` tests cover ~12 edge cases (clipping, palette-0 transparency, off-screen X, scrolling). Our PPU may or may not match any given case.

3. **Odd-frame dot skip** — on odd frames with rendering enabled, the PPU skips one idle dot at `(0, 0)`. Our `nes-clock-topology.md` flags this; whether it's actually implemented and tested under the blargg `oddframe` ROM is unverified.

The catalogue (Super Mario Bros., Contra, Zelda 2, Tetris, TMNT 2) hashes title-screen frames at a known frame count. Any of these latent bugs could silently lock into the catalogue hash without manifesting on the title screen.

**Diagnosis.** Same shape as the Spectrum's Seam 1: we have a complex pixel pipeline, integration tests don't exercise the dot-level edges, and the canonical reference test suite (blargg) is not in CI. The Spectrum used the Float48K probe; the NES has roughly 200 blargg test ROMs covering exactly this surface — and they're free.

**Proposed change.**

1. Pull the canonical blargg PPU test ROMs into `test-data/blargg-ppu/` (small, ~50 KB total, available from blargg's public site or the nesdev test-roms repository). License is permissive — same shape as the ZEX corpus.
2. Add `crates/machine-nintendo-nes/tests/blargg_ppu.rs` (or a sibling `runtime` test) that runs each test ROM until completion, reads the result byte from PRG-RAM, and asserts pass. Mark as `#[ignore]`-by-default with a CI job that runs them. Mirror the ZEX CI job shape exactly.
3. The result-byte protocol blargg uses is documented: PRG-RAM `$6000` becomes the status byte once the test runs; a sentinel sequence at `$6001-$6003` signals "test running" vs "test complete". The harness polls these and decodes pass/fail.
4. First-pass coverage: `ppu_vbl_nmi`, `sprite_hit_timing`, `sprite_overflow`, `oam_read`, `oam_stress`, `vbl_nmi_timing`. Each is one ROM ~16 KiB; full suite ~10 ROMs.
5. For any test that fails, the failure is a real PPU timing bug. Fix per-test, lock the green status as a CI gate.

The same pattern extends to APU once Seam 1 is stable.

**Silicon evidence (Reference library + cross-validation):**

- *NESdev wiki* — comprehensive cycle-by-cycle PPU rendering tables. The de-facto specification.
- *blargg PPU test ROMs* — gold-standard regression bench. Used by every accuracy-focused NES emulator.
- *Mesen2 source* — the most accurate open NES emulator. Direct cross-check for any failing blargg test.
- *FCEUX* — older but well-known; second cross-check.

**Scope.** Test-harness work — no PPU code changes until a blargg test fails. ~150 lines of test-harness infrastructure + 10 test ROMs in `test-data/`. Per-test fix scopes vary; experience from other emulator codebases suggests 1-3 small bugs surface, each fixable in <20 lines.

**Status: partial landed 2026-05-21.** Harness in `crates/machine-nintendo-nes/tests/blargg_ppu.rs` polls the `$6000` status byte after the `DE B0 61` signature appears at `$6001-$6003`. 12 ppu_vbl_nmi + oam_read/stress tests wired (the sprite_hit + sprite_overflow corpus uses a different console-based protocol; deferred to a separate harness). Baseline: 7 passing / 12 total. One per-test fix landed: `mos-6502` NMI boundary edge-detect removed (commit `65a1b9d`) — fixes ppu_vbl_nmi/04-nmi_control "Immediate occurrence should be after NEXT instruction". Five tests remain failing (real PPU fidelity bugs in NMI/VBL timing precision, suppression race, NMI off-timing, odd-frame dot-skip BG-enable race, OAM stress) — each needs targeted per-test investigation.

**Why this matters for other systems.** The NES is the densest test-ROM ecosystem of any 8-bit system. Establishing the harness pattern here gives the Game Boy (Mooneye + blargg) and CGB the same shape later.

### Seam 2 — Host input → controller routing

**Current state.** `crates/runtime-nintendo-nes/src/input.rs:15-29` routes `InputEvent::Key` and `InputEvent::Button { port: 1, ... }` through `apply_named_button`, which maps button names to bit positions in the controller-1 shift register. Port-2, mouse, axis events are silently dropped per the comment.

**Friction observed.** Three gaps:

1. **No port-2 controller.** The NES has two physical controller ports (CIA-equivalent: $4016 / $4017). Two-player games (Contra co-op, Battle City, Bubble Bobble) need controller 2 wired through to the second shift register. The runtime currently doesn't route this.
2. **No gamepad event production.** Same as the Spectrum had before commit `c3499b5` — the native binary doesn't poll the host gamepad and emit `InputEvent::Button` events. The runtime accepts them; nothing emits them. `emu198x-nes/src/main.rs` would need the `NativeGamepadInput` wiring the Spectrum just landed.
3. **`InputEvent::Key` quirk.** The current routing maps key names ("a", "b", "start", "up", etc.) to controller-1 buttons. That conflates keyboard-as-controller with literal NES keypad input. The Famicom keyboard add-on is post-October, but the routing layer should leave room — port-2 keypad events would land at a future `InputEvent::Key { port: 2, ... }` extension, not the current port-less variant.

**Diagnosis.** Same pattern as Spectrum Seam 2 and C64 Seam 2 — the runtime input layer was scaffolded for single-controller-1 and needs the per-port surface.

**Proposed change.** Mirror the Spectrum's Seam 2 contract:

1. Recognise `InputEvent::Button { port: 1 | 2, name, pressed }` and route to controller-1 or controller-2 respectively.
2. Add `set_controller2` on the `machine_nintendo_nes::Nes` struct mirroring the existing `set_controller1`; thread the shift register through the existing `$4017` read path.
3. Wire `NativeGamepadInput` in `emu198x-nes/src/main.rs` using the Spectrum's `ButtonInputMap` / `AxisInputMap` shape. Default map: gamepad 0 → controller 1, gamepad 1 → controller 2.
4. Define a stable button-name surface: `a`, `b`, `select`, `start`, `up`, `down`, `left`, `right` (already in place). Add aliases for common gamepad SDK names (`south` / `east` / `button1` / `cross` / `circle`) routing to `b` / `a` so the host-side mapper can be neutral.

The Spectrum's `gamepad_maps_for_machine` pattern (`crates/emu198x-spectrum/src/ui/app.rs`) is the canonical reference. The NES is simpler — no variant dispatch since all variants have the same controller surface.

**Scope.** ~30 lines in `runtime-nintendo-nes/src/input.rs`, ~20 lines in `emu198x-nes/src/main.rs`, ~10 lines in `machine-nintendo-nes/src/lib.rs` for `set_controller2`.

**Status: landed 2026-05-21** (commit `a4802c3`). machine-nintendo-nes: `controller2_state` + `controller2_shift` fields, `set_controller2` API, $4017 read path with the same strobe-latched shift register protocol used by $4016. The $4016 strobe falling edge latches BOTH controllers' state. runtime-nintendo-nes::input: routes `InputEvent::Button { port: 1, … }` → controller 1, `port: 2` → controller 2; other ports dropped silently. Gamepad SDK alias mapping (south → a, east → b, plus button1-4 / cross / circle / square / triangle aliases) so host code can stay neutral. 10 new tests (3 machine-level controller 2 + 7 runtime input). emu198x-nes native binary already polls gamepads correctly for port 1; per-gamepad-ID-to-port routing is a deeper enhancement deferred (shared with Spectrum).

**Why this matters for other systems.** The pattern is now in place on Spectrum, Amiga, Dragon. The C64 (just-drafted review) and NES are the remaining systems that need it. Once landed everywhere, the host-gamepad surface is uniform across the product.

### Seam 3 — Volatile state survival across 14 mappers + APU + PPU

**Current state.** `crates/runtime-nintendo-nes/src/snapshot.rs` is 73 lines — small. Round-trip works ("snapshot round-trips work for every chip + concrete mapper state" per the status doc). The serde-skip surface across the NES stack is larger than any other system in the codebase: every mapper has its own state, plus PPU, plus APU.

**Friction observed.** The snapshot system "works" in the sense that the postcard round-trip is byte-clean. The latent issue is whether the *behavioural* round-trip holds — does a snapshot taken mid-DMC-sample-fetch resume cleanly? Does a snapshot taken mid-MMC3-A12-counter resume with the correct IRQ phase? Does MMC5 ExRAM contents survive? The NES catalogue is too small (5 entries) and the snapshot harness too new (per the doc, "snapshots exist now, but should be treated as version-1 internal snapshots until broader compatibility policy lands") to surface these.

The Spectrum's Seam 3 fix surfaced two real bugs (Z80 walker rehydration, ULA config reattachment) on a stack with one CPU + one ULA + one memory + one tape. The NES stack is roughly 5× denser: 14 mappers × per-mapper state + APU 5-channel state + PPU pixel-pipeline state + OAMDMA state + 2 controller shift registers + DMC DMA state.

**Diagnosis.** Same pattern. Need to extend the `after_restore` discipline the Spectrum landed to every NES chip + every mapper. Each `#[serde(skip)]` field needs either a `Default` that produces correct behaviour or a typed rehydrator.

**Proposed change.**

1. Add `after_restore` to the `Mapper` trait (default no-op; per-mapper impls for MMC3 IRQ counter phase, MMC5 PPU-pattern detector state, VRC2a bus latches).
2. Add `Mos6502::rehydrate_walker_sequence()` mirroring the Z80 version. Identical pattern, identical mechanical fix.
3. Audit APU's `#[serde(skip)]` fields: envelope phase, sweep current period, length counter, linear counter pre-load latch, frame counter mode bit, DMC sample remaining, DMC reader buffer. Lock the inventory.
4. Audit PPU's `#[serde(skip)]` fields: pixel pipeline shift registers, secondary-OAM scan state, sprite-0 in-flight flag, BG fetch latch, palette index pipeline. Lock the inventory.
5. Add `crates/machine-nintendo-nes/src/serde_skip_audit.rs` mirroring the Spectrum's. Lock the inventory; CI gate catches drift.
6. Snapshot envelope version bump (currently v1 per the status doc note) to v2 with the audit + rehydration in place. Mark current v1 snapshots as not-loadable to force the cleanup.

**Scope.** Walker rehydration is mechanical (~30 lines, same as Spectrum). Per-mapper `after_restore` is small per-mapper but adds up — 14 mappers × ~10 lines each = 140 lines. APU + PPU audits ~30 each. Audit lock ~50. Total ~250 lines.

**Why this matters for other systems.** Every system with mid-instruction CPU state + stateful audio + stateful video needs this discipline. NES is the densest case in the product; getting it right here is the load-bearing template.

### Seam 4 — Catalogue oracle integrity

**Current state.** `crates/emu198x-catalogue/manifest/nes.toml` has 5 entries (super-mario-bros, contra, zelda-2, tetris, tmnt-2-arcade). No `audio_routing_version` or `frame_routing_version` declared on `[system]`. Same gap as the C64 manifest had before this review.

**Friction observed.** Same shape as Spectrum and C64 Seam 4. When any of the in-flight NES fidelity work lands (PPU dot-level fix from Seam 1, APU envelope rehydration from Seam 3, mapper-specific A12 timing), every catalogue hash will change. Without routing-version gating, the changes silently land into the manifest and lock in the new behaviour before anyone's eyes have been on it.

**Diagnosis.** Catalogue infrastructure already supports `audio_routing_version` / `frame_routing_version` — it's just opt-in per system.

**Proposed change.**

1. Add `AUDIO_ROUTING_VERSION: u32 = 1` in `ricoh-apu-2a03` (or wherever the audio mixer lives — currently APU output is composed of pulse 1 + pulse 2 + triangle + noise + DMC with a specific mix curve).
2. Add `FRAME_ROUTING_VERSION: u32 = 1` in `ricoh-ppu-2c02` (the framebuffer-emitting layer).
3. Reflect both in `nes.toml`'s `[system]` block.
4. Refresh the NES section of `solid-status.md` (when one exists for NES — currently the NES isn't in SOLID scope) in the same commit.

**Scope.** ~10 lines across two chip crates + the manifest. Identical pattern to the C64 Seam 4 fix.

**Status: landed 2026-05-20.** `AUDIO_ROUTING_VERSION: u32 = 1` added to `ricoh-apu-2a03/src/lib.rs`; `FRAME_ROUTING_VERSION: u32 = 1` added to `ricoh-ppu-2c02/src/lib.rs`; `nes.toml` declares both. `verify_routing_versions` in `emu198x-catalogue/src/lib.rs` extended with a `"nes"` arm. Three new unit tests cover the NES happy path + both mismatch paths (audio + frame). The capture-bypass already covers NES by construction (system-agnostic). Seam 1 (blargg PPU gate) will bump `FRAME_ROUTING_VERSION` when any per-test fix lands; Seam 3 (volatile state survival) likely won't touch the routing version directly since `after_restore` discipline rebuilds non-trivial state without changing the canonical mix or pixel pipeline.

**Why this matters for other systems.** Universal across the product. The Spectrum proved the pattern; C64 + NES are catching up.

### Seam 5 — Per-system boot invariants suite + blargg gate

**Current state.** The NES has `nestest_smoke` (in `machine-nintendo-nes/tests/nestest.rs`) as the only timing waypoint test. Marked `#[ignore]`, runs against the local ROM, asserts CPU PC/A/X/Y/P/SP match the golden log at every instruction fetch.

**Friction observed.** Same as Spectrum / C64 Seam 5 — waypoint-level invariants are not asserted independently of the catalogue's end-state view. A PPU regression that breaks Super Mario Bros.'s status-bar split (sprite-0 hit at wrong scanline) might still produce a "title-screen renders" hash that matches.

The NES has a richer test-ROM ecosystem than any other system in scope (blargg, Tepples, Kevin Horton, Shay Green, NESdev community). Lots of these are unused. Seam 1 above lifts the blargg PPU set; Seam 5 generalises to the full surface.

**Diagnosis.** Same pattern. Need a `tests/boot_invariants.rs` in `runtime-nintendo-nes` plus a CI gate on the blargg test corpus. Different from Spectrum / C64 in that the blargg tests are the canonical waypoints — we don't need to write hermetic ones from scratch.

**Proposed change.** Add `crates/runtime-nintendo-nes/tests/boot_invariants.rs` covering:

- Snapshot envelope version locked at v2 (post-Seam-3).
- Controller 1 + 2 shift-register state survives reset cleanly.
- OAMDMA stalls CPU for exactly 513 cycles on even-cycle-start, 514 on odd.
- DMC DMA steals exactly one cycle per sample fetch (or up to 4 on contended alignment per NESdev wiki Table 4).
- Mapper IRQ counter survives `mapper.tick(...)` calls correctly when the A12 line oscillates (covers MMC3 ambiguity).
- PPU NMI line clears cleanly on reset; doesn't leak across snapshot restore.
- Cartridge bytes survive snapshot round-trip (already proven by the manifest harness but worth locking in a hermetic test).

Plus the blargg CI gate from Seam 1.

**Scope.** ~30 lines per waypoint × 7 waypoints = ~210 lines. Plus the blargg gate (already specified under Seam 1).

**Why this matters for other systems.** The blargg test-ROM pattern is well-developed for the Game Boy (Mooneye, blargg), the Sega Master System / Mega Drive (less so but growing), and even the 6502-using systems we don't yet emulate. Establishing the pattern on NES creates the harness shape; later systems plug in.

## Verified non-issues

Recorded here because the audit examined them and they are not seams. Future sessions should not re-discover.

### Mapper-as-trait + `Box<dyn Mapper>`

The pattern is correct. Every mapper has its own crate-internal module + struct. The runtime polymorphism is light (a function-pointer indirect per bus access) and the value is enormous (each mapper is a closed unit with its own state, its own tests, its own iNES dispatch). Don't fold into a single big-enum dispatcher.

### NMI delivery via 2-dot pipeline

[nes-clock-topology.md](nes-clock-topology.md) documents the 2-dot pipeline delay between the PPU setting `nmi = true` at (241, 1) and the CPU actually being told. This was the load-bearing fix that let nestest pass; it's not under question. Seam 1's blargg coverage will verify the *exact* dot edges, but the structural decision stands.

### NTSC vs PAL clock divider asymmetry

NES NTSC is 1:3 CPU:PPU exactly; NES PAL is 1:3.2 average (CPU runs ~6% slower). The tick loop handles this via the variant's master-clock divider configuration; per-frame totals (262 scanlines NTSC, 312 scanlines PAL) differ as expected. Tested implicitly by Super Mario Bros. running at 60 fps NTSC.

### iNES parsing semantics

`format-nintendo-nes-ines` parses iNES 1.0 and detects iNES 2.0 features. Mapper number → concrete mapper class via the `Mapper::from_ines` dispatcher. Submapper handling for mapper 34 (BxROM vs NINA-001) is heuristic; the status doc flags this as "ambiguity". Real NES 2.0 submapper support would resolve it cleanly. Not a structural seam — a parsing refinement.

## Fidelity findings deferred beyond October

Real silicon-level fidelity gaps that don't affect the catalogue regression bench and don't block October-public. Captured here so they're not re-discovered.

### Open-bus PPU register reads

Reading from $2000 or $2001 (write-only registers) returns the most-recently-written byte on the PPU bus, not zero. Some titles read these and depend on the value. Our PPU likely returns zero. Documented as a gap; not a regression risk for the title corpus.

### Famicom Disk System

Separate hardware add-on (FDS) for Japan-only titles. Not in the engineering-bar scope.

### Bandai LCD Compact Wireless / VRC6 / FME-7 expansion audio

Mappers beyond the 14 supported. Expansion audio (VRC6 sawtooth, MMC5 pulse + PCM already supported, Namco 163, Sunsoft 5B) means the audio mix is non-trivial. Long tail; lifted as needed when a real-game requirement surfaces.

### PAL APU envelope subtleties

PAL frame counter has slightly different timings than NTSC (a 7457 vs 7458 CPU-cycle divider). Likely a one-cycle drift on PAL only. Specific titles may surface this.

### Mapper-specific quirks beyond MMC3/MMC5

MMC3 IRQ A12 edge detection: there are at least three variants (MMC3 rev A vs B vs C) with subtly different counter reload behaviour. Most games work with rev B emulation; we may not need to distinguish. Tracked here so it's not re-discovered.

## Order of work

In order of leverage for unblocking NES progression:

1. **Seam 4 (catalogue oracle integrity)** — must land *first*. Same reason as the Spectrum and C64 reviews — gates any subsequent fix's re-capture wave.
2. **Seam 1 (PPU blargg gate)** — pulls the test corpus, fails loud on anything broken, drives the per-test fixes. The biggest concrete fidelity win.
3. **Seam 3 (volatile state survival)** — formalise `after_restore` across 14 mappers + APU + PPU. Locks the audit. Largest scope but mechanical work.
4. **Seam 2 (host input → controller routing)** — small, user-visible, no dependency on the others. Can land in parallel with Seam 1 or 3.
5. **Seam 5 (boot invariants + blargg CI gate)** — incremental, one waypoint per landing PR. The blargg gate becomes CI-mandatory once Seam 1's harness exists.

## Done criteria

- **Seam 1**: blargg PPU test corpus runs in CI under `#[ignore]`-by-default + `--include-ignored` job. All canonical tests pass: `ppu_vbl_nmi`, `sprite_hit_timing`, `sprite_overflow`, `vbl_nmi_timing`, `oam_read`, `oam_stress`. Any failure is a real fidelity bug, fixed per-test.
- **Seam 2**: gamepad event flips a NES controller state byte; native binary and script runner both wire `NativeGamepadInput`. Per-port disambiguation works (port 1 default, port 2 explicit opt-in via map).
- **Seam 3**: every `#[serde(skip)]` field on the NES stack (CPU, PPU, APU, all 14 mappers) has correct `Default` or typed `after_restore` rehydrator. Audit test asserts the inventory. Snapshot envelope at v2.
- **Seam 4**: `audio_routing_version` and `frame_routing_version` constants in place; NES manifest declares both.
- **Seam 5**: `boot_invariants.rs` with the named waypoints; blargg CI gate hot.
- This document is updated with implementation status and links to commits as each seam lands.

## Non-goals

- Adding new mappers. The 14 supported cover 627/629 local-archive ROMs. New mappers land as new titles require them.
- Famicom Disk System or Famicom-only peripherals.
- Submapper disambiguation beyond mapper 34. Lifted opportunistically.
- PAL boot-validation depth. NTSC is the canonical target.
- VS. UniSystem / arcade-NES variants.
- 4-player adapter, Zapper, Power Pad, Famicom Keyboard. All post-October.

## Related

- [`spectrum-architecture-review.md`](spectrum-architecture-review.md) — the template (now closed-out across 5 seams)
- [`amiga-architecture-review.md`](amiga-architecture-review.md) — sibling review for the third active system
- [`c64-architecture-review.md`](c64-architecture-review.md) — drafted same session as this one; mirrors structure
- [`nes-clock-topology.md`](nes-clock-topology.md) — the spine this review preserves
- [`cpu-bus-interface.md`](cpu-bus-interface.md) — universal pin-level CPU rule
- [`within-family-layering.md`](within-family-layering.md) — chip-per-crate the seam fixes respect
- [`runtime-internal-shape.md`](runtime-internal-shape.md) — runtime layer the seam fixes build on
- [`save-state-format.md`](save-state-format.md) — postcard envelope the snapshot work extends
- [`october-catalogue.md`](october-catalogue.md) — October-public bar (NES is engineering-bar; no October deadline)

## Reference library cross-links

The NES reference surface is unusually rich. The most relevant material:

| Reference | Topic | Relevance |
|---|---|---|
| NESdev wiki (PPU + APU pages) | Cycle-by-cycle reference, de-facto spec | Seams 1, 5 |
| blargg test ROMs (`ppu_vbl_nmi`, `sprite_hit_timing`, `oam_*`) | Gold-standard regression bench | Seams 1, 5 |
| Mesen2 source | Most accurate open NES emulator | Cross-validation for all seams |
| FCEUX source | Second-opinion open emulator | Cross-validation |
| Nintendulator source | Original cycle-accurate reference | Cross-validation, historical context |
| Kevin Horton's `nestest.log` | CPU instruction reference (already in CI) | Seam 1 (extended PPU coverage above this) |
| Tepples' `mmc5test`, `oam_stress` | Mapper-specific + OAM edge cases | Seams 1, 3 |

The Spectrum's silicon-level reference was Smith's *The ZX Spectrum ULA*. The C64's was Marko Mäkelä's VIC-II articles. The NES's equivalent is the NESdev wiki — community-maintained but exceptionally thorough. The blargg test-ROM corpus is the canonical regression bench across the NES emulation community.

### Cross-cutting global KB

- `~/knowledge/retro-peripheral-architecture-is-pin-budget-not-design-choice.md` — the 2A03's RDY pin (DMC DMA), the 2C02's A12 pin (mapper IRQ), and the 2A03's NMI input from the PPU are the entire bus-arbitration surface. Everything else (sprite DMA, OAMDMA stall, mapper banking) is built on those three wires. Pin budget shapes architecture, as it does for every retro system.
