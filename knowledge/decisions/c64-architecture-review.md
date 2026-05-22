# Decision: C64 architecture review — tighten the seams, not the spine

**Date:** 2026-05-20
**Status:** Mostly landed — Seam 1 audit only; Seams 2, 3, 4, 5 landed 2026-05-20/21

## What this is

A targeted review of the Commodore 64 implementation against the load-bearing decisions already in place ([CPU bus interface](cpu-bus-interface.md), [Within-family layering](within-family-layering.md), [Runtime internal shape](runtime-internal-shape.md), [Crate naming](crate-naming.md), [Save state format](save-state-format.md)). The C64 is the next engineering-bar system after the Spectrum and has been at the same "boots to READY, runs real titles to partial states" level since the April push that landed the four-chip wiring. This review names the seams that the catalogue *cannot* catch by construction: silently-locked-in wrong VIC-II timing, lost host input, volatile state that doesn't survive snapshot restore, oracle integrity, and standing boot-invariant assertions.

The spine stays. The seams need work.

This document mirrors [`spectrum-architecture-review.md`](spectrum-architecture-review.md) and [`amiga-architecture-review.md`](amiga-architecture-review.md). The structure is deliberately the same so the three reviews can be read side by side. The seam findings are grounded against the Commodore Programmer's Reference Guide, the C64 service manual, and the C64-specific reverse-engineering literature in the Reference library plus the established C64 cycle-exactness body of work (VICE, Hoxs64, Marko Mäkelä's VIC-II articles).

## What we are *not* changing

These decisions are load-bearing, validated by the 6502 100% Tom Harte pass, the KERNAL boot-to-READY proof at frame 108 (~2.16 s emulated, matching real-hardware ~2.5 s), and the seven catalogue entries running through to expected end-states. Nothing in this review revisits them:

- **Master oscillator drives the loop.** `C64::tick` in `crates/machine-commodore-c64/src/machine.rs:369` advances one phi2 cycle: VIC first (asserts BA low for badlines / sprite DMA), CIAs second, IRQ/NMI/RDY wiring third, CPU bus transaction fourth (gated on RDY for reads), SID last. The phi2 counter is the only time anchor.
- **Pin-level CPU bus interface, no Bus trait.** `mos-6502` exposes `addr`, `data`, `data_in`, `rw`, `sync`, `rdy`, `irq`, `nmi` as fields. The machine layer reads pins between ticks and dispatches reads/writes through the `$01` banking decoder.
- **`VicMemory` trait abstraction.** VIC-II reads source memory through `VicMemory::vic_read(addr)`, which the machine implements by reflecting the current CIA2-PA-derived bank. The trait keeps `mos-vic-ii` agnostic of `C64Memory`'s banking layout.
- **RDY gates reads only.** The NMOS 6502 (and 6510) honour RDY by stalling on reads but completing writes — `cpu.rdy = !vic.ba_low || !cpu.rw`. Critical for badline accuracy.
- **One-op-per-tick discipline.** CPU advances one bus operation per phi2 tick when RDY is high. Multi-cycle instructions take the right number of phi2 cycles because each one consumes one bus operation.
- **BA→RDY routing in the machine layer.** Neither the VIC-II nor the CPU mediates this; the machine wires `vic.ba_low` and `cpu.rw` into `cpu.rdy` directly. Same shape as the Spectrum's ULA-drives model, transplanted to the BA-stall pattern.
- **IEC bus as shared mutable state.** When the 1541 is attached, `IecBus` is a separate struct passed by mutable reference to both `C64::tick_with_iec_bus` and the drive's tick path. Real silicon: the bus is wired-AND between drive and host; the struct models that.

If the review below appears to require revisiting any of these, the review is wrong and the decision wins.

## The five seams

### Seam 1 — VIC-II BA/RDY cycle accounting

**Current state.** `crates/machine-commodore-c64/src/machine.rs:371-379` reads `_cpu_stalled` from `vic.tick(...)` and discards the value, then computes `cpu.rdy = !vic.ba_low || !cpu.rw` from the bus-arbitration field. The VIC-II's `tick` implements per-cycle BA pulldowns for badlines (40 character DMA cycles + 3 cycles of pre-badline BA assertion at cycle 11) and sprite DMA (2 cycles per active sprite, pre-allocated regardless of enable).

**Friction observed.** Three of the four "title reaches a stable loader state but doesn't actually start running game code" cases in the catalogue (Ghostbusters loader stall, Thinker/Thomas KERNAL-text-but-not-loaded, Thing on a Spring needing space-press to confirm hand-off) are consistent with subtle BA timing being off. Each title either runs raster effects that depend on exact cycle counts, polls VIC raster compare for timing, or has a loader that uses CIA Timer A in counter mode driven off CIA1 PB6 (CNT1) which is bus-arbitration-sensitive. The catalogue can't catch this because each title's "stable observable KERNAL text state" hashes consistently even when the loader is internally stalled — the hash captures the bezel + bottom-row text, not the live raster.

**Diagnosis.** Three potential issues sit under the same surface and the catalogue can't distinguish them:

1. The discarded `cpu_stalled` return value — the VIC-II's own opinion about whether the CPU should be stalled. We've delegated to `ba_low` instead; the two should agree but the asymmetry is a smell.
2. Sprite DMA cycles 0-2 are always allocated on a per-line basis regardless of `SPRITE_ENABLED`. Our `vic.tick` accounting may or may not match this. Software that watches `$D012` (raster) while spinning a CIA timer will diverge across our model vs real silicon if these counts drift by even one cycle per line.
3. The BA→RDY combination only stalls reads. The exact phi2 of CPU resumption after BA goes high — and the cycle on which the CPU sampling samples the bus first — is the kind of half-cycle subtlety that produces "loader works on real hardware, hangs in emulator" symptoms.

**Proposed change.** Three sub-fixes, all internal to `mos-vic-ii` + the machine layer's tick:

1. Promote `cpu_stalled` from a discarded return value to a load-bearing assertion. Either `cpu_stalled` equals the BA-driven RDY-low condition (in which case the field is redundant and should be removed), or it differs (in which case one of them is wrong and we should align them). Make this a unit test in `machine-commodore-c64`.
2. Audit sprite DMA cycle allocation against Marko Mäkelä's "The MOS 6567/6569 video controller (VIC-II)" reference table — the 8-sprite-and-badline collision schedule is fully documented. Catch any one-cycle drifts with a structural unit test on the `vic.ba_low` raster.
3. Add a "raster compare → IRQ" waypoint that asserts the IRQ fires on the exact phi2 the CPU sees the raster latch reach the compare value (not one phi2 early or late). Catches a cycle-of-one-error in either VIC.tick ordering or IRQ wire latching.

**Silicon evidence (Reference library):**

- *Marko Mäkelä, "The MOS 6567/6569 video controller (VIC-II) and its application in the Commodore 64"* — gold-standard cycle-by-cycle BA/RDY breakdown. Drives most accurate VIC-II ports including VICE.
- *C64 Service Manual* — schematic-level BA pin behaviour and AEC interaction.
- *Computes Mapping the 64 / Mapping the VIC* — register-level reference for raster compare semantics, sprite DMA register effects.

**Cross-validation references.** VICE's `vicii-cycle.c` is the open canonical implementation of the BA / sprite DMA schedule. Hoxs64 is an independent re-implementation that's bit-for-bit Marko-compliant. Either is a per-line reference for our schedule.

**Scope.** ~50 lines in `mos-vic-ii` (cycle accounting), ~20 lines in the machine-layer tick (audit), one structural waypoint test in `runtime-commodore-c64`.

**Status: audit landed 2026-05-21** (commit `a9ab627`). Promoted `cpu_stalled` from a discarded return value to a public field on `Vic`. Audit conclusion: `ba_low` (cycles 12-54 of a badline, 5-cycle window per sprite) and `cpu_stalled` (cycles 15-54 / 2-cycle per sprite) are NOT redundant — they encode the 3-cycle NMOS warm-up between BA assertion and AEC drop. Machine layer correctly drives `cpu.rdy` off `ba_low` (NMOS read-stall semantics); `cpu_stalled` is exposed for future fidelity work modelling writes that race against AEC drop. Six lock-down tests added: badline asymmetry, sprite asymmetry, full 8-sprite DMA cycle table vs Mäkelä §3.8, disabled-sprite cycle release, raster IRQ exact-phi2 assertion, raster IRQ non-spurious-fire. No engine behavior change. Subsequent engine work (e.g. cycle-count corrections from a regression) bumps `FRAME_ROUTING_VERSION` and triggers re-capture.

**Why this matters for other systems.** Every system with cycle-stealing video DMA (BBC Micro, Atari 800/XL, Apple II HBL, Amiga blitter) shares the same seam shape. Get the C64's right; the pattern transfers.

### Seam 2 — Host input → joystick routing

**Current state.** `crates/runtime-commodore-c64/src/input.rs` handles `InputEvent::Key` and routes character-based key names into the keyboard matrix. There's a partial `InputEvent::Button` path for joystick-style controls, but the routing surface isn't documented and `kempston`-equivalent state lives ad-hoc.

**Friction observed.** The C64 has two physical joystick ports — port 1 on CIA1 port B (shared with the keyboard) and port 2 on CIA1 port A (also shared, but with the keyboard matrix scanning the other direction). A user plugging a gamepad in expects something to happen. The Spectrum's Seam 2 closed-out this exact problem (commit `3087016` for the runtime layer, `6b19411` for the catalogue verification, `eff0528` for IF2, `c3499b5` for the host-side gamepad wiring). The C64 has the same problem unsolved.

Plus the C64 has paddle / mouse 1351 / light pen inputs that all share the same physical ports through CIA1 — those are explicitly post-October work, but the routing layer's shape needs to leave room for them.

**Diagnosis.** The runtime input layer treats input as keyboard-only with an ad-hoc joystick patch on top. Joystick (and future: paddle, mouse) routing is unimplemented per the design pattern the Spectrum just landed.

**Proposed change.** Mirror the Spectrum's Seam 2 approach exactly:

1. Define a stable `port: 0 | 1` convention in the `runtime-commodore-c64` input layer:
   - `port: 0` → CIA1 PA joystick (port 2 on the C64, the "main" gameport — most games use this since the keyboard scan goes the other way and doesn't conflict).
   - `port: 1` → CIA1 PB joystick (port 1 on the C64, shares with keyboard rows 0-7 bits 0-4).
2. Map host axes / buttons to the per-port `joystick_state: u8` byte (5 bits: up/down/left/right/fire; bit 0 = up, active-low).
3. Make `port 1` decline silently when the user hasn't acknowledged the keyboard-conflict (similar to how IF2 events would have keyboard side-effects on the Spectrum — except here the conflict is far worse because most games scan the keyboard while polling joystick).
4. Wire `emu198x-c64` (the native binary) and `emu198x-script-c64` to emit `InputEvent::Button { port: 0 | 1, name, pressed }` from the host gamepad layer. Use the existing `NativeGamepadInput` from `emu198x-shell` (shared with Spectrum / Dragon / Amiga).

The Spectrum's gamepad wiring code (`crates/emu198x-spectrum/src/ui/app.rs:gamepad_maps_for_machine`) is the canonical reference. The C64 equivalent dispatches one map for "PAL with port-2 default", possibly a second for "user requested port 1".

**Scope.** ~30 lines in `runtime-commodore-c64/src/input.rs`, ~10 lines in `emu198x-c64`, the equivalent static `BUTTON_MAP` / `AXIS_MAP` const definitions. No public API churn.

**Status: landed 2026-05-21** (commit `a4802c3`). runtime-commodore-c64::input defines stable input port convention: input port 0 → C64 gameport 2 (CIA1 PA, main `$DC00`) / input port 1 → C64 gameport 1 (CIA1 PB, keyboard-shared `$DC01`) / ports ≥ 2 dropped silently. Gamepad SDK alias mapping (south/east/west/north/cross/circle/square/triangle/button1-4 all → FIRE — the C64 stick is single-fire). emu198x-c64 native binary's `C64_JOYSTICK_MAP` updated from port=2 to port=0 to match the Seam-2 convention. 9 new runtime tests covering port mapping + alias mapping + observable CIA1 PA/PB effect via the actual machine.

**Why this matters for other systems.** Every cassette/cartridge-era system in the roadmap has 1-2 joystick ports that are subtly entangled with keyboard or peripheral state. NES (controllers shared with port latches), Game Boy (no joystick but a single d-pad), Amiga (port 1 mouse, port 2 joystick — already done). The C64 sits between Spectrum (Kempston peripheral) and NES (latched serial input); getting the pattern right keeps it transferable.

### Seam 3 — Volatile state survival across snapshot restore

**Current state.** `crates/runtime-commodore-c64/src/snapshot.rs` is 89 lines — small. The C64 runtime postcards a single struct holding the machine. Sub-chips use `#[serde(skip)]` for non-trivial state (similar pattern to the Spectrum pre-Seam-3).

**Friction observed.** Several `#[serde(skip)]` fields across the C64 stack:

- `mos-sid-6581/src/filter.rs` — the state-variable filter has internal taps that drift on restore.
- `mos-sid-6581/src/envelope.rs` — envelope phase counter is non-trivial.
- `mos-sid-6581/src/voice.rs` — oscillator phase, ring-mod / sync state.
- `mos-vic-ii/src/lib.rs` — sprite DMA pointers, bad-line latch, raster compare in-progress state.
- `mos-cia-6526/src/lib.rs` — timer internal current values, TOD latch state, alarm comparisons.
- `mos-6502/src/walker.rs` (or equivalent) — same mid-instruction-state issue as the Z80 (instruction sequence pointer, partial decode state).
- Datasette pulse-stream position.
- IEC bus line state (when 1541 is attached).
- 1541 6502 walker, VIA timer state.

The Spectrum's Seam 3 fix surfaced two real bugs (Z80 walker rehydration, ULA config reattachment) that the catalogue couldn't catch even with snapshot fidelity checks because the waypoints landed at moments when the latent bugs happened not to fire. The C64 catalogue is smaller (7 entries) and SOLID criterion 8 (Save state) is not yet covered by a `run_c64_entry_with_snapshot_check` harness equivalent.

**Diagnosis.** Same pattern as Spectrum Seam 3. The `#[serde(skip)]` annotations are correct as-is — those fields are large or not-reconstructible from disk — but every one needs a typed `after_restore` rehydrator that produces correct downstream behaviour.

**Proposed change.**

1. Add an `after_restore` trait hook to the C64 machine (mirroring `SpectrumMachine::after_restore`). Default no-op; per-chip implementations rehydrate non-trivial state from preserved invariants.
2. Implement `Mos6502::rehydrate_walker_sequence()` analogous to the Z80's — rebuild the `&'static [MStep]` sequence from preserved `(opcode, step_idx)` after deserialisation. This is mechanical: the same pattern the Spectrum stack uses.
3. For SID filter / envelope / voice, audit which fields are `#[serde(skip)]` and add a `reset_filter_taps()` / `reset_envelope_phase()` / etc. set of helpers that produce *audibly correct continuation* after restore. The bar is: a snapshot taken mid-Ghostbusters-title-music must play correctly when restored — currently the filter taps would be wrong and the music would briefly distort.
4. Audit VIC-II skip fields against Marko Mäkelä's bad-line / sprite DMA tables — anything that affects whether BA goes low on the next phi2 must be preserved or rehydratable.
5. Add a "serde_skip_audit" lock for the C64 stack, mirroring `crates/common-sinclair-zx-spectrum/src/serde_skip_audit.rs`. Locks the inventory; CI gate catches drift.
6. Snapshot envelope version bump to `2` to capture the 1541-attached state and disk image cache, mirroring the Spectrum's Seam 3 disk preservation. Today restoring a C64 snapshot taken with a mounted D64 silently drops the disk.

**Scope.** ~80 lines across the chip crates (rehydrators), ~30 lines in `runtime-commodore-c64/src/snapshot.rs` (envelope v2 with disk cache), one audit file. The most expensive piece is the SID — that needs care because audio glitches are subjectively very noticeable.

**Status: audit lock landed 2026-05-21** (commit `ba3648c`). C64 stack turned out to be already clean — zero `#[serde(skip)]` in mos-vic-ii, mos-cia-6526, mos-6502, machine-commodore-c64, machine-commodore-1541, common-commodore-c64. The only `#[serde(skip)]` annotations are two on `mos-sid-6581` (the transient output audio buffers `buffer` and `channel_buffers`) — and `Default::default()` (empty Vec) is the correct behaviour because the host drains samples per frame, so no rehydration is needed. Audit lock landed at `crates/machine-commodore-c64/src/serde_skip_audit.rs` mirroring the Spectrum and NES patterns. The full per-chip walker / envelope / filter-tap rehydration the original Seam 3 plan envisioned turns out to be unnecessary work because the C64 chip crates never used `&'static` references for state — everything is plain serializable data. The `Mos6502` walker is stored as serializable `OpcodeInfo` enums (not `&'static [MStep]` like the Z80), and SID voice / envelope / filter state is all owned `u8`/`i32`/etc. The Spectrum's Seam 3 fixed a real walker rehydration bug because the Z80 *did* use `&'static [MStep]`; the C64 has no such bug to fix. Future work that adds `&'static` state or external resources (e.g. 1541 disk image cache surviving restore) would land here, but no such surface exists today.

**Why this matters for other systems.** Every system with stateful audio (Amiga Paula, NES APU, Game Boy APU) and stateful video DMA has the same surface. Get the C64 right; the rehydration pattern transfers.

### Seam 4 — Catalogue oracle integrity

**Current state.** `crates/emu198x-catalogue/manifest/c64.toml` has 7 entries (5 disk-loaded titles + Thinker/Thomas tape proofs + Ghostbusters tape). No `audio_routing_version` or `frame_routing_version` declared on `[system]`. The Spectrum manifest carries both since the Seam 4 work; the C64 doesn't.

**Friction observed.** When the Spectrum's Seam 1 landed (the ULA shifter pipeline fix), every catalogue entry's frame hash changed. The routing-version oracle made this fail loud — every entry produced "frame routing version 1 manifest, engine produces version 3" with a re-capture instruction. Without the oracle the change would have silently passed (because re-captures happen ad-hoc anyway) or silently broken downstream tools depending on stable hashes.

The C64 has multiple in-flight fidelity bugs (Ghostbusters loader stall, Thinker/Thomas not-fully-loaded, sprite-DMA-clock skew if Seam 1 lands). Any of these landing will rewrite every catalogue entry's hash. Without routing-version gating, the changes will land silently into the captured manifest and lock in the new behaviour before anyone's eyes have been on it.

**Diagnosis.** The catalogue infrastructure already supports `audio_routing_version` and `frame_routing_version` — `verify_routing_versions` in `crates/emu198x-catalogue/src/lib.rs:366`. The C64 manifest just hasn't opted in.

**Proposed change.**

1. Add `AUDIO_ROUTING_VERSION: u32 = 1` to `common-commodore-c64::audio` (or wherever audio mixing lives — currently the SID's `take_audio_buffer`).
2. Add `FRAME_ROUTING_VERSION: u32 = 1` to `mos-vic-ii` (the framebuffer-emitting layer).
3. Reflect both in `c64.toml`'s `[system]` block.
4. Refresh the C64 section of `solid-status.md` (or wherever the C64's per-criterion status lives) in the same commit.

The capture-bypass mechanism (`run_entry_for_capture`) is already system-agnostic — works for any system once the constants exist.

**Scope.** ~10 lines in `mos-vic-ii` + `mos-sid-6581` + the manifest + the C64 status doc. The routing-version constants get bumped together with the engine-side fix in any future seam that changes frame or audio output.

**Status: landed 2026-05-20.** `AUDIO_ROUTING_VERSION: u32 = 1` added to `mos-sid-6581/src/lib.rs`; `FRAME_ROUTING_VERSION: u32 = 1` added to `mos-vic-ii/src/lib.rs`; `c64.toml` declares both. `verify_routing_versions` in `emu198x-catalogue/src/lib.rs` extended with a `"c64"` arm. Three new unit tests cover the C64 happy path + both mismatch paths (audio + frame). The capture-bypass already covers C64 by construction (system-agnostic). Future fidelity work (Seam 1 BA/RDY accounting, SID filter rehydration in Seam 3, etc.) will bump the appropriate constant and trigger a forced re-capture of the C64 catalogue.

**Why this matters for other systems.** Every system the catalogue tracks needs this oracle. The Spectrum's Seam 4 work is system-agnostic infrastructure; the C64 is the first beneficiary of the pattern beyond the system that originated it.

### Seam 5 — Per-variant boot invariants suite

**Current state.** The C64 has `tests::boots_kernal_to_ready_prompt` (in `machine-commodore-c64`) — an `#[ignore]`d integration test that runs the boot path against real ROMs and finds `READY.` in screen RAM. Beyond that, the only timing-level invariants enforced in CI are the per-chip unit tests (6502 Tom Harte, VIC-II unit tests, SID register file, CIA timer reset semantics).

**Friction observed.** Same as Amiga Seam 5 and Spectrum Seam 5: waypoint-level invariants are not asserted independently of the catalogue's end-state view. When a timing regression slips through that the catalogue happens not to surface (because the loader's hash is computed on a stable text frame, not a live raster), there's no second line of defence. PAL/NTSC variant timing, IRQ rate, raster compare position — none have boot-invariant locks today.

**Diagnosis.** Same pattern as the other reviews. Diagnostic examples and ROM-backed boot tests are append-only; promotion to "CI-mandatory, hermetic where possible" doesn't happen automatically.

**Proposed change.** Add `tests/boot_invariants.rs` to `runtime-commodore-c64`. Mirrors the Spectrum suite's shape. Initial set (each per-variant where the variant differs):

- `kernal_irq_asserts_at_canonical_raster_line` — per variant (PAL: raster 0, NTSC: raster 0 — both off CIA1 Timer A, not raster IRQ, but the test sets a known IRQ source and asserts the phi2 of assertion).
- `vic_ba_low_pattern_matches_canonical_for_badline` — assert BA goes low at cycle 11 of a badline (per Marko Mäkelä's table), stays low for ~43 cycles, returns high. Pure structural — no machine stepping required.
- `sprite_dma_cycles_allocated_per_active_sprite` — assert sprites 0-2 always allocate their 2 cycles per line, independent of enable, and 3-7 allocate only when enabled.
- `cia1_timer_a_irq_at_60hz_target_rate` — locks the IRQ rate the KERNAL configures during boot.
- `snapshot_envelope_version_is_v2` — once Seam 3 envelope-v2 lands, the version is pinned. Catches silent envelope drift.
- `cia2_pa_drives_vic_bank_select` — assert CIA2 PA0/PA1 changes propagate to the VIC bank decoder.
- `paging_via_01_port_changes_active_rom` — the 6510 I/O port LORAM/HIRAM/CHAREN bits select the right ROM overlay. Catches a regression that affects every `LOAD` + `RUN`.

**Scope.** ~30 lines per waypoint, ~250 lines total for the suite. Promoted from existing diagnostic examples and the boot-to-READY test.

**Status: landed 2026-05-21** (commit `4bbd608`). Three new boot invariants on top of the existing 3: `snapshot_envelope_version_is_locked_at_v1`, `six510_io_port_banking_changes_active_rom` (walks `$01` between `$37` ROMs-visible and `$30` all-RAM, confirms `$A000` switches from BASIC ROM 0x42 to RAM 0x00 — catches a regression that would break every LOAD + RUN), `cia2_pa_drives_vic_bank_select` (walks CIA2 PA through all four bank selections and confirms `vic.bank()` tracks). Runtime now runs 6 hermetic + 1 ignored ROM-backed invariant on every `cargo test`. The remaining waypoints from the original plan (raster IRQ timing, sprite DMA cycle table, badline BA pattern) are already locked at the chip-crate level by the Seam 1 audit (`mos-vic-ii` unit tests landed in commit `a9ab627`).

**Why this matters for other systems.** Every per-system review (Spectrum 12 waypoints, Amiga ~8 planned) lands the same shape: a `boot_invariants.rs` that locks the canonical timing facts. The C64's becomes the third example in the family.

## Verified non-issues

Recorded here because the audit examined them and they are not seams. Future sessions should not re-discover.

### 6510 vs 6502 baseline divergence

The `mos-6502` crate is the 6502 substrate. The C64 wraps it with the `$00`/`$01` I/O port for memory banking (LORAM/HIRAM/CHAREN at `$01` bits 0-2, plus tape sense at bit 4 and tape write at bit 3). The wrapping lives in `C64Memory` (`crates/machine-commodore-c64/src/memory.rs`) and is correct — Ghostbusters specifically depended on the bit-mapping fix that landed in April per the C64 status doc. Not a seam.

### VIC-II bank selection from CIA2 PA

The `refresh_vic_bank` helper in `C64::tick` reads `cia2.pa` and writes the inverted low two bits into `vic.set_bank()` on every tick. The path looks expensive but the writes are cheap (single field assignment) and miss-detection would surface as obviously-wrong VIC RAM content. Not a seam.

### IEC bus shared-mutable-state model

`tick_with_iec_bus` takes `&mut IecBus` and passes it through. This is the right shape for a wired-AND bus shared between two computers. Both the C64's CIA2 PA and the 1541's VIA1 read/write the same bus state struct. The seam-class concern would be "the bus state isn't serialised correctly across snapshot restore" — that's in Seam 3, not a separate concern here.

### `mos-6502` Tom Harte single-step coverage

The 6502 is exercised at 100% by Tom Harte single-step vectors (10000 tests per opcode × 256 opcodes). The Spectrum side's confidence in `zilog-z80` comes from the same source, which is why the Spectrum review didn't propose a CPU-level seam. The C64's CPU is on the same footing. Not a seam.

## Fidelity findings deferred beyond October

Real silicon-level fidelity gaps that don't affect the catalogue regression bench and don't block October-public. Captured here so they're not re-discovered.

### SID 6581 vs 8580 filter cutoff curves

Different filter cap voltages between the two SID revisions produce audibly different filter behaviour. Several famous titles (notably most Rob Hubbard work) were composed against the 6581 and sound thin on 8580. Our current `mos-sid-6581` only models the 6581 family. Adding the 8580 as a separate revision is post-October work.

### REU (RAM Expansion Unit)

Cartridge-class DMA expansion. Several demoscene titles require REU. Deferred until cartridge support lands.

### Cartridge support (.CRT + EXROM/GAME lines)

The memory decoder's PLA variants are wired in the archive via `cart.exrom` / `cart.game`; our current implementation is the EXROM=1, GAME=1 case (no cartridge attached) only. Several big titles ship as cartridge images. Post-October.

### Datasette pulse-level read accuracy

Real TAP datasette media work for Thinker / Thomas / Ghostbusters / Thing on a Spring at the "reach a stable observable state" bar but the latter three are not yet proven to fully complete loading. Several known datasette loaders (Novaload, Freeload, Turbo Tape) have specific pulse-timing tolerances we may not match perfectly. Tightening this is engineering-bar work, not October-public.

## Order of work

In order of leverage for unblocking the C64's progression from "reaches stable text states" to "actually runs games end-to-end":

1. **Seam 4 (catalogue oracle integrity)** — must land *first*. Adds the routing-version constants so any subsequent fix's re-capture wave fails loud. Low scope, high leverage.
2. **Seam 1 (VIC-II BA/RDY cycle accounting)** — closes the open loader-stall thread on Ghostbusters / Thinker / Thomas. Re-capture triggered by the Seam 4 version bump.
3. **Seam 3 (volatile state survival)** — formalise the `after_restore` contract, fix the chip-specific `#[serde(skip)]` rehydration, add the audit lock. Same pattern as Spectrum.
4. **Seam 2 (host input → joystick routing)** — small, user-visible, no dependency on the others. Can land in parallel with Seam 1 or 3.
5. **Seam 5 (boot invariants suite)** — incremental, one waypoint per landing PR. Begin once Seams 1 and 3 stabilise.

## Done criteria

- **Seam 1**: `cpu_stalled` and BA-derived RDY agree (audit landed); sprite DMA cycle allocation matches Marko Mäkelä's table within 0 cycles per line; raster-compare IRQ waypoint passes. At least one previously-stuck loader title (Ghostbusters most likely) advances past its current stall to a measurably later state.
- **Seam 2**: gamepad event flips a C64 joystick state byte; native binary and script runner both wire `NativeGamepadInput` through. Per-port disambiguation works (port 0 default, port 1 explicit opt-in).
- **Seam 3**: every `#[serde(skip)]` field on the C64 stack has a `Default` that produces correct behaviour or is rehydrated by a typed `after_restore`. Audit test asserts the inventory. Snapshot envelope at v2 includes the 1541 disk image when mounted.
- **Seam 4**: `audio_routing_version` and `frame_routing_version` constants in place; C64 manifest declares both; catalogue mismatch fails loud with a re-capture instruction.
- **Seam 5**: `boot_invariants.rs` carries at least five per-variant waypoint assertions (PAL initially, NTSC added once the NTSC profile has equivalent fidelity).
- This document is updated with implementation status and links to commits as each seam lands.

## Non-goals

- Adding new chip crates. The C64 stack is complete at the chip level (6502, VIC-II, SID, CIA, plus IEC bus and 1541 substrate).
- Refactoring the chip-per-crate boundary. The within-family layering decision held for the C64 and isn't revisited here.
- Cartridge support, REU, or .CRT format. Post-October per the [product roadmap](product-roadmap.md).
- 8580 SID variant. Post-October.
- NTSC variant boot-validation depth. The PAL profile is the canonical one; NTSC remains research-grade per the C64 status doc.
- The C64-mini / C64-DTV / C128 variants. Not in the engineering-bar scope.

## Related

- [`spectrum-architecture-review.md`](spectrum-architecture-review.md) — the template this review mirrors
- [`amiga-architecture-review.md`](amiga-architecture-review.md) — the sibling review for the third active system
- [`cpu-bus-interface.md`](cpu-bus-interface.md) — the spine this review preserves
- [`within-family-layering.md`](within-family-layering.md) — the chip-per-crate structure the seam fixes respect
- [`runtime-internal-shape.md`](runtime-internal-shape.md) — the runtime shape per-system reviews build on
- [`save-state-format.md`](save-state-format.md) — the postcard envelope the C64's snapshot work extends
- [`october-catalogue.md`](october-catalogue.md) — the October-public bar (C64 is engineering-bar, no October deadline)

## Reference library cross-links

The C64 reference library at `../reference/by-system/commodore-c64/` holds 300+ files. Most relevant to the seams below:

| Reference | Topic | Relevance |
|---|---|---|
| Marko Mäkelä, "The MOS 6567/6569 video controller (VIC-II)" | Cycle-by-cycle BA/RDY/sprite DMA table | Seam 1 |
| C64 Service Manual (1992-03) | Schematic-level pin behaviour | Seam 1 |
| Commodore Programmer's Reference Guide (1983) | Verbatim register semantics for VIC-II / SID / CIA / 6510 I/O port | Seams 1, 3 |
| Computes Mapping the C64 / Mapping the VIC | Memory map and register effects reference | Seams 1, 3 |
| C64 demoscene techniques notes | Where the cycle-exact requirements get exercised | Seam 1 |
| C64 SID engine reverse-engineering notes | Music-driver expectations; informs SID Seam 3 fidelity bar | Seam 3 |

The Spectrum's silicon-level reference was Smith's *The ZX Spectrum ULA* — a single book covering 24 chapters of the chip. The C64 doesn't have one equivalent reference; the canonical cycle-exactness body is distributed across Marko Mäkelä's articles, the original Commodore documentation, and the open-source VICE codebase. The reference library has the documentation side; VICE is the implementation cross-check.

### Cross-cutting global KB

- `~/knowledge/retro-peripheral-architecture-is-pin-budget-not-design-choice.md` — applies to the C64 as much as the Spectrum. The VIC-II's BA pin and the CPU's RDY pin are the entire bus-arbitration vocabulary; everything else (sprite DMA, badlines, AEC) is built on those two wires. Pin budget shapes architecture.
