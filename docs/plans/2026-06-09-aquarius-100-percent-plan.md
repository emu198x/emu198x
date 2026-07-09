> Planning document. Do not treat status claims here as current unless they match `../status/current-system-usability.md`, `../status/outstanding-work.md`, and `../../RULES.md`.

---
title: "plan: Mattel Aquarius to 100% — char-ROM firmware, media formats, audio routing, display fidelity"
type: plan
date: 2026-06-09
system: docs/systems/mattel/aquarius.md
basis: code-grounded survey of machine-/runtime-/emu198x-mattel-aquarius crates with live test runs, plus shared-chip findings (Z80 at-ceiling, AY-3-8910 partial), 2026-06-09
---

# Mattel Aquarius — road to 100%

What it would take to bring the Mattel Aquarius to feature- and accuracy-complete,
grounded in a code-level survey of the actual crates and tests. The CPU is shared
and at-ceiling; the only audio chip is the shared AY-3-8910 (a peripheral, and
the Aquarius consumes only its I/O ports today). So this plan is almost entirely
**system shape** — firmware plumbing, media formats, audio routing, and display
fidelity — not core-chip accuracy.

## Executive summary

**The Aquarius is a small, nearly-finished system whose long pole is breadth, not
depth.** It is a Z80-only machine (`crates/machine-mattel-aquarius/src/lib.rs`,
823 lines, v0.2.0) with a memory-mapped 40×24 character display rendered directly
in the machine crate — there is no custom video chip and no custom sound chip on
the base unit. The CPU is the shared `zilog-z80` core, which is **at the accuracy
ceiling** (Tom Harte 1,604,000/1,604,000, FUSE 1351/1356) — **no CPU work** on the
road to 100%. The sole audio device is the *optional* Mini Expander's shared
`gi-ay-3-8910`, and even that is wired only for its I/O ports (controllers): the
tone/noise/envelope generators are constructed but unconsumed
(`lib.rs:218-221, 260`).

What *is* done: BIOS boots live to the Microsoft BASIC magenta/black title
(verified 2026-06-01, `bios_boot.rs`); the memory map, scrambler "software lock"
(`descramble`, `lib.rs:431-437`), cartridge top-of-memory mapping, 8×6 keyboard
matrix with A8-A15 row select, and the Mini-Expander controller decode (MAME-cited
disc/button codes, `lib.rs:89-109`) all work and are tested. **All 12 machine unit
tests and 2 runtime tests pass.** Three integration tests are `#[ignore]`d — all
gated on copyrighted ROMs not in-tree (`bios_boot`, `cart_autostart_probe` ×2),
not on defects.

The long pole is **the separate character-generator ROM is not a first-class
firmware**. The display only renders correctly with the 2 KB char-gen ROM, which
lives on a separate chip — but the machine profile declares only the BIOS as
firmware (`profiles.rs:59-63`), so the generic firmware path (`from_firmware`)
can't supply it; only the `--char` CLI flag and the dedicated `set_char_rom` setter
can. Without it the renderer falls back to the BASIC ROM's upper 2 KB, which is Z80
code, so glyphs render as garbage (`lib.rs:498-506`; `drivability-assessment.md:358`).
And the char ROM is **dropped on snapshot restore** (`snapshot.rs:11-18` omits it),
so a restored session renders garbage even after the ROM was supplied. These are
the headline correctness gaps.

Everything else is breadth: no cassette (`.caq`) format (port `$FC` swallowed,
`lib.rs:474`), the 1-bit speaker is exposed but never resampled to host audio
(runtime emits empty packets, `runtime.rs:286-292`), the AY tone/noise generators
are unconsumed, the display is painted once at end-of-frame (no mid-frame fidelity),
and the TEA1002 palette is plausible-but-uncalibrated.

**There is one shared-chip dependency worth naming:** when the Mini Expander AY is
eventually made audible, it inherits the AY-3-8910's established envelope/noise/shape
timing defects (envelope and noise an octave too fast; alternating and hold envelope
shapes wrong). Those are chip-level and tracked at the chip; the Aquarius work is to
*route* the AY audio, not to fix the chip.

**Totals (focused work):**

| Tier | Scope | Estimate |
|------|-------|----------|
| A — "Curriculum 100%" | char-gen ROM as first-class firmware + snapshot fix; speaker audio routing; doc fixes | **~1 week** |
| B — System fidelity | per-scanline display, TEA1002 palette calibration, complete snapshot (CPU/RAM/AY/keys) | **~1.5–2 weeks** |
| C — Audio completeness | consume the Mini-Expander AY tone/noise audio + mix with the 1-bit speaker (rides the shared AY defect fixes) | **~3–5 days** |
| D — Preservation breadth | cassette `.caq` load (+ save), 16 KB game-cart split coverage, printer, BIOS/cart asset sourcing | **~1–2 weeks** |

**True 100% of everything ≈ 4–6 weeks.** It is **front-loaded**: the headline
correctness wins (char ROM as firmware + snapshot fix) are a few days and unblock
the only thing that makes the system *look* right. There is no hard core-accuracy
long pole here — the hard chip (the Z80) is already done, and the only other chip
is a shared, optional peripheral.

Effort key: **S** = hours · **M** = a few days · **L** = 1–2 weeks · **XL** = multi-week.

## Tier A — Curriculum 100% (do first)

| Item | Effort | Notes |
|------|--------|-------|
| **Char-gen ROM as first-class firmware** | **S–M** | The 2 KB character ROM is what makes the screen legible, but the profile firmware vec lists only the BIOS (`profiles.rs:59-63`); `CHAR_FIRMWARE_ID` is defined (`profiles.rs:40`) but never added. Add a `FirmwareRequirement` for it so `from_firmware` and the generic loaders can supply it, mirroring the existing `--char` CLI path (`script.rs:165-178`). Without this the only way to a legible screen is the dedicated setter. |
| **Snapshot preserves the char ROM** | **S** | `AquariusRuntimeSnapshotV1` (`snapshot.rs:11-18`) carries BIOS, cart, expansion, time, model — but **not** `char_rom_bytes`, so a restored session rebuilds with the garbage fallback (`runtime.rs:184-203`). Add the field and restore it. Bug, not breadth: round-trip is silently lossy. |
| **1-bit speaker → host audio** | **M** | The speaker bit is modelled (`lib.rs:476-479, speaker_bit()`), but the runtime emits empty audio packets every frame (`runtime.rs:286-292`). Capture per-sample speaker state during `run_frame` (PWM-style) and resample to the 48 kHz packet. The single audible output of the base machine. |
| **System-doc + status touch-ups** | **S** | Fold the verified state: char ROM unwired-as-firmware, snapshot char-ROM drop, speaker unrouted. The outstanding-work entry (`outstanding-work.md:962-973`) still lists "Mini-Expander AY stub" and "Snapshot deferred"; the AY *I/O ports* are now wired and tested, and a snapshot envelope exists (incomplete) — correct both. |

## Tier B — System fidelity

| Item | Effort | Notes |
|------|--------|-------|
| **Per-scanline display** | **M–L** | `render_display` (`lib.rs:484-519`) paints the whole 320×192 framebuffer once at end-of-frame (`run_frame`, `lib.rs:292`). Mid-frame char/colour-RAM writes only appear next frame. Move to a per-scanline raster so colour-split and mid-frame text effects render the same frame. (Same shape as the C64/NES per-scanline work, far smaller surface.) |
| **TEA1002 palette calibration** | **S** | The 16-colour palette (`lib.rs:174-191`) is hand-transcribed and "plausible but not calibrated against real-hardware photos" (`outstanding-work.md:966-967`). Two entries are already fudged (index 2 "dark blue rendered as black", `lib.rs:176`). Derive from a TEA1002 datasheet / capture and cite it; there is currently no TEA1002 entry in `knowledge/chips/` or the primary `reference/` library (the Aquarius reference holds only magazines), so this needs a primary-source anchor first. |
| **Complete snapshot envelope** | **M** | Beyond the char ROM, the snapshot re-derives the *whole machine* from BIOS/cart/expansion (`snapshot.rs`, `runtime.rs:180-203`) — it captures **no** live CPU registers, char/colour/spare/expansion RAM, key matrix, scrambler latch, speaker bit, or AY state. A mid-session save/restore resets the running program. Serialise the live machine state, not just the construction inputs. |

## Tier C — Audio completeness

| Item | Effort | Notes |
|------|--------|-------|
| **Consume the Mini-Expander AY audio** | **M** | The AY is constructed and ticked only for its I/O ports; the tone/noise/envelope output is discarded (`lib.rs:78-84, 218-221` — "tone generators are unconsumed"). Pull `Ay3_8910` sample output each frame and mix it with the 1-bit speaker into the host audio packet. **Rides the shared AY-3-8910 fixes** — envelope/noise run an octave too fast and the alternating/hold envelope shapes are wrong; those are chip-level and fixed at the chip, but until then Aquarius Mini-Expander music inherits them. Most base-unit software uses only the 1-bit speaker, so this is genuinely the Mini-Expander tail. |

## Tier D — Preservation breadth

| Item | Effort | Notes |
|------|--------|-------|
| **Cassette `.caq` load** | **M** | Port `$FC` reads/writes are swallowed (`io_read` returns `0xFF` default; `io_write` `lib.rs:474` discards). Add a `.caq` tape format crate (the Aquarius cassette container) + a load path through `$FC`, so BASIC `CLOAD` works. The base machine's primary software-distribution medium. **No cassette format crate exists** (`format-*aquarius*` is empty). |
| **Cassette save** | **S–M** | The write path (motor-paced pulse emission + `.caq` writer), riding whatever the family's tape-save decision is. |
| **16 KB game-cart coverage** | **S** | The cart maps top-of-memory by size (8 KB at `$E000`, 16 KB at `$C000`, `lib.rs:391-402`), and the scrambler lock is modelled (`descramble`). Exercise a real 16 KB cart under the `cart_autostart_probe` harness (currently `#[ignore]`, ROM-gated) to confirm the `$C000` mapping + scramble carries a banked title, not just the 8 KB case. |
| **Printer (port `$FE`)** | **S** | Status read returns not-busy; data writes are swallowed (`io_read:454`, `io_write:475`). Wire a host-side text sink if a curriculum unit needs `LPRINT`. Niche. |
| **BIOS + char-ROM asset sourcing** | **S** | The 2 KB char-gen ROM is "not present in `~/.emu198x/roms/` or TOSEC" (`drivability-assessment.md:361-365`); the cart-autostart tests stay `#[ignore]` until BIOS + char + a cart are sourced. A sourcing/cataloguing chore (Cat198x), not code — but it gates un-ignoring three tests. |

## Done as part of this plan (free, ~half a day)

Status-doc reconciliation. The outstanding-work entry (`outstanding-work.md:962-973`)
predates the 2026-06-05 controller wiring and still calls the Mini-Expander AY a
"stub" — its **I/O ports are now wired and tested**
(`controllers_read_through_the_expander_ay_ports`, `lib.rs:760-784`); only the
*audio* is unconsumed. The same entry says "Snapshot deferred", but a (metadata-only,
incomplete) snapshot envelope now exists (`snapshot.rs`). The per-frame NMI noted in
the old "VBlank drives Z80 NMI (50 Hz PAL pulse)" line (`outstanding-work.md:946`) was
**removed as fictitious** — the base Aquarius wires no periodic interrupt, and a stray
NMI corrupted the BIOS cart-detect loop (`lib.rs:281-287`, `cart_autostart_probe.rs:7-12`);
the doc should reflect the corrected model.

## Recommended sequence (highest leverage first)

1. **Char-gen ROM as first-class firmware** (S–M) + **snapshot char-ROM fix** (S) —
   the two correctness gaps that make the screen legible and keep it legible across
   save/restore. Highest leverage; a few days.
2. **1-bit speaker → host audio** (M) — the only audible output of the base machine;
   currently silent.
3. **Per-scanline display** (M–L) — mid-frame fidelity for colour-split text effects.
4. **Complete snapshot envelope** (M) — make save/restore actually preserve a running
   session, not just the construction inputs.
5. **TEA1002 palette calibration** (S) — needs a primary-source anchor first (no
   TEA1002 reference entry exists yet).
6. **Cassette `.caq` load** (M) then **save** (S–M) — the base machine's main software
   medium.
7. **Consume the Mini-Expander AY audio** (M) — the Mini-Expander tail; rides the
   shared AY defect fixes.
8. **16 KB cart coverage, printer, asset sourcing** (S each) — completionist tail.

## Key files

- CPU (shared, at ceiling — no work): `crates/zilog-z80/` (Tom Harte + FUSE green).
- Machine: `crates/machine-mattel-aquarius/src/lib.rs` — memory map (`mem_read`/`mem_write` `:379-421`), scrambler (`descramble` `:431-437`), cart mapping (`:391-402`), keyboard (`io_read` `:439-466`), render (`render_display` `:484-519`), speaker (`io_write` `:476-479`), AY-port wiring (`:439-466`, controllers `:558-591`), no-NMI rationale (`run_frame` `:279-295`).
- Machine tests: `crates/machine-mattel-aquarius/tests/{bios_boot.rs, cart_autostart_probe.rs}` (both `#[ignore]`, ROM-gated); unit tests in `src/lib.rs:633-792` (12 pass).
- Runtime: `crates/runtime-mattel-aquarius/src/{runtime.rs,profiles.rs,snapshot.rs,input.rs}` — firmware gap (`profiles.rs:38-76`), char-ROM rebuild (`runtime.rs:184-203`), empty-audio (`runtime.rs:286-292`), snapshot drop (`snapshot.rs:11-18`).
- Binary/harness: `crates/emu198x-mattel-aquarius/src/script.rs` (`--bios/--char/--cart/--expansion-kb`, char-ROM load `:165-178`).
- AY peripheral (shared): `crates/gi-ay-3-8910/src/lib.rs` (I/O ports wired; envelope/noise/shape defects tracked at the chip).
- Reference: `reference/by-system/mattel-aquarius/` (magazines only — no TEA1002 / hardware datasheet entry); MAME `aquarius.cpp` + `bus/aquarius/mini.cpp` (cited throughout the machine crate).
- Status: `docs/status/outstanding-work.md:939-973`, `docs/status/drivability-assessment.md:358-365`, `docs/status/current-system-usability.md:68`.

