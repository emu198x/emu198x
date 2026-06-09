---
title: "plan: Acorn Atom to 100% - software loading, VDG graphics, cassette audio, accuracy debt"
type: plan
date: 2026-06-09
system: docs/systems/acorn/atom.md
basis: code-grounded survey of machine-acorn-atom / runtime-acorn-atom + shared mos-6502 and motorola-vdg-6847 crates, live test runs, 2026-06-09
---

# Acorn Atom - road to 100%

Grounded in code and tests. The Atom has no knowledge/systems doc and no reference/by-system/acorn-atom extract (dir exists but empty); sources of truth are the code, its tests, and the status docs. Where docs drift from code, code wins.

## Executive summary

The Atom is a fourth distinct shape: the CPU core is at the ceiling, the machine boots and types, but it is a closed box - there is no way to get software into it. Unlike the C64 (hard core long pole) or NES (finished core + cheap breadth), the Atom's long pole is breadth from a standing start: no cassette load, no .atm/.tap/.uef format crate, no media slot, no graphics modes, no audio out. A learner can boot to ACORN ATOM > and type a BASIC line - that is the whole envelope.

Done: CPU (6502) at ceiling via shared mos-6502 NMOS core (Tom Harte 2.56 M, Klaus Dormann - shared-chip findings), driven through public pin fields (lib.rs:116-129) - no CPU work. Boot + keyboard: cold-starts (cpu.reset(), lib.rs:78-82), types end-to-end through a correctly-modelled Intel 8255 PPI with synthesised VDG field-sync on port C (lib.rs:145-156); both end-to-end tests prove this but both are #[ignore] pending a real 24 KB ROM (live run: 8 unit tests pass, 2 integration tests ignored).

Not done (the whole road to 100%): no software ingestion (load_media rejects every image, runtime.rs:159-166; media_slots empty, profiles.rs:76; no format-acorn-atom-* crate, confirmed by Glob). No graphics modes (wrapper honours only the A/G bit, vdg.rs:46-52, lib.rs:42-43; modes 1-5 render solid green, outstanding-work.md:670-671). No audio (1-bit speaker unwired - runtime pushes an empty audio buffer every frame, runtime.rs:196-202). Line-accurate (not beam-accurate) VDG plus a $B000 control-register wiring concern (Tier B) - latent but real accuracy debt.

So "100% Atom" is breadth-from-zero + VDG graphics + audio + a small pile of accuracy debt, with no CPU work and no hard core long pole. Front-loadable: the highest-value item (.atm/cassette loading) unlocks the Acornsoft library at once.

Totals (focused work): Tier A "Curriculum 100%" (.atm + cassette loading, graphics modes 1-5, 1-bit speaker audio, un-ignore boot/keyboard tests, missing keys) ~4-6 weeks. Tier B Cycle/output accuracy (beam-accurate VDG sub-line [shared-chip], $B000 wiring fix, field-sync/tone provenance) ~2-3 weeks. Tier C Audio fidelity (cassette tone + speaker mixing, cassette save write path) ~2-3 weeks. Tier D Preservation breadth (.uef chunk completeness, utility-slot $A000 carts, printer, expansion, snapshot fullness) ~3-5 weeks. True 100% ~11-17 weeks, front-loaded. Effort key: S=hours, M=a few days, L=1-2 weeks, XL=multi-week.

## Tier A - Curriculum 100%

- .atm file loading (M). Highest leverage. No format crate; load_media rejects all (runtime.rs:159-166); no media slot (profiles.rs:76). Add a format crate + MediaSlot + RAM-injection load path.
- Cassette load .uef/.tap (L). Feed the 2.4 kHz FSK stream through 8255 port C - PC4 is a free-running placeholder tone (lib.rs:152); replace with a real bitstream decoder driven by a tape image.
- Graphics modes 1-5 (L). Wrapper honours only A/G (vdg.rs:46-52,108-113); modes show green (outstanding-work.md:670). The shared motorola-vdg-6847 crate already implements all 8 modes - machine-wiring, not chip work.
- 1-bit speaker audio (M). Speaker (port C) unwired; runtime emits an empty buffer (runtime.rs:196-202). Tap the port-C bit, make a waveform, fill the audio packet.
- Un-ignore boot + keyboard tests (S-M). Both #[ignore] pending a real 24 KB ROM (rom_boot.rs:22, keyboard_type.rs:53). Add an in-tree ROM strategy.
- Missing keyboard keys (S). Shift, shifted punctuation, editing keys unmapped (input.rs:116-119, doc at input.rs:11-12).

## Tier B - Cycle / output accuracy

- $B000 control-register wiring (S-M). Likely defect. $B000 write latches the whole port-A byte into vdg.control (lib.rs:180-185); render_frame reads css = control & 0x08 (PA3), int_ext = control & 0x10 (PA4) (vdg.rs:108-113). But PA0-3 is the keyboard column index (lib.rs:111-114,182-183) - css reads a keyboard bit. Latent in text mode; corrupts colour-set once graphics land. Verify PA-to-6847 mapping. NEEDS VERIFICATION.
- Beam-accurate VDG sub-line (L). Shared-chip item: line/byte-accurate renderer, mode-control sampled per frame/line; mid-scanline changes not reproduced. Latent for text-mode Atom; chip-level.
- Field-sync/tone provenance (S). Field-sync (master_clock % 20_000 < 1_000) and 2.4 kHz tone (% 416 < 208) are uncited magic numbers (lib.rs:151-153). Replace with datasheet-derived values.

## Tier C - Audio fidelity

- Cassette tone + speaker mixing (M). Mix speaker with cassette tone, pace off the master clock not the % 416 placeholder (lib.rs:152).
- Cassette SAVE write path (M). No writable mount or writer. Add motor-paced pulse emission + writer + writable mount/flush, mirroring the C64 datasette-save decision.

## Tier D - Preservation breadth

- .uef chunk completeness (M). Full chunk coverage beyond a basic load.
- Utility-slot $A000 carts (M). $A000-$AFFF is unmapped backing ROM (lib.rs:141-144; MAME assembly leaves it empty, outstanding-work.md:667). Add a pluggable slot.
- Printer (S). Unwired (outstanding-work.md:672). Niche.
- Expansion (AtomMMC, Econet, colour board) (M-L). Deep tail.
- Snapshot fullness (S-M). Snapshot serialises only {version,time,model_id,bios_bytes} (snapshot.rs:10-16) and rebuilds a fresh machine on restore (runtime.rs:114-116) - RAM/VRAM/CPU/PPI/keyboard all lost. A no-op save-state.

## Done as part of this plan (free, ~half a day)

- "PIA 6520" is wrong - it is an Intel 8255 PPI. outstanding-work.md:653 and the lib.rs file docstring (line 1) say "6520 PIA", but the code uses intel_8255::Ppi8255 (lib.rs:53,66,89). The rows at usability:82 and outstanding-work.md:1267 already say INS8255 correctly; fix the two stale "6520" mentions.
- Boot/keyboard tests are #[ignore], not in CI. Status prose (usability:82) holds only with a local ROM; note the gating.
- No knowledge/systems/acorn-atom.md and reference/by-system/acorn-atom/ is empty - worth authoring.

## Recommended sequence (highest leverage first)

1. .atm loading (M). 2. Graphics modes 1-5 (L). 3. 1-bit speaker audio (M). 4. Un-ignore boot/keyboard tests + missing keys (S-M + S). 5. $B000 wiring fix (S-M) - before/with graphics. 6. Cassette load .uef/.tap (L). 7. Field-sync/tone provenance + beam-accurate VDG (S + L). 8. Cassette SAVE, UEF completeness, utility-slot, snapshot, printer, expansion - the long tail.

## Key files

- CPU (at ceiling): shared crates/mos-6502/src/{lib,cycle,tick}.rs; driven at crates/machine-acorn-atom/src/lib.rs:116-129.
- Machine: crates/machine-acorn-atom/src/lib.rs (memory map :131-189, 8255 + field-sync/tone :145-156, $B000 latch :180-185, cpu.reset() :78-82).
- VDG wrapper: crates/machine-acorn-atom/src/vdg.rs (:46-52, :108-113); shared chip crates/motorola-vdg-6847/src/lib.rs.
- Keyboard/input: crates/machine-acorn-atom/src/{keyboard.rs,input.rs}; runtime crates/runtime-acorn-atom/src/input.rs.
- Runtime: crates/runtime-acorn-atom/src/runtime.rs (:159-166, :196-202); profiles.rs:76; snapshot.rs:10-16.
- Tests: crates/machine-acorn-atom/tests/{rom_boot.rs,keyboard_type.rs} (both #[ignore]); 8 unit tests pass.
- Status: docs/status/outstanding-work.md:636-674, current-system-usability.md:82, drivability-assessment.md:145.
- Reference: reference/by-topic/cpu-6809/motorola-mc6847-...-1984.docling/ (VDG datasheet); no Atom-specific extract exists.
