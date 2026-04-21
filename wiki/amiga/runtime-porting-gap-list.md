# Commodore Amiga runtime — port-gap analysis (2026-04-21)

Phase 1–3 gap list for task #179, following the archive-port
methodology. The runtime is a bridge crate rather than a chip, so
the methodology collapses into a single-commit rewrite: the archive
is the **old** pre-restart runtime; the live crate targets the
**new** `machine-commodore-amiga-ocs`.

## What the runtime is

The Amiga runtime bridges `machine-commodore-amiga-ocs` (the
authoritative A500 OCS machine) with the `emu198x-shell`
`MachineCore` trait. It owns:

- The `MachineProfile` metadata (A500 OCS PAL model, Kickstart
  firmware requirement, DF0 media slot, capability declarations).
- The `AmigaOcs` machine instance + stored ROM bytes so `reset()`
  rebuilds cleanly.
- An RGBA frame buffer (ARGB → RGBA byte-reorder of the machine's
  768×576 framebuffer).
- Input routing (keyboard-only for now).
- A small set of query paths surfaced via `SessionQueryProvider`.

## Current-tree coverage

| Area | Current state |
| --- | --- |
| `machine-commodore-amiga-ocs::AmigaOcs` | ✅ live, fully functional |
| `AmigaOcs::tick` — master/4 step | ✅ |
| `AmigaOcs::insert_adf` / `eject_disk` | ✅ (Floppy port, #169) |
| `AmigaOcs::key_event` | ✅ (Keyboard port, #174) |
| `AmigaOcs::denise().framebuffer()` | ✅ 768×576 ARGB |
| `emu198x-shell::MachineCore` contract | ✅ stable |
| Live runtime bridging `MachineCore` ↔ `AmigaOcs` | ❌ absent (archive targeted old machine) |

## Archive coverage (`crates/runtime-commodore-amiga-archive/`)

The archive targeted `machine-commodore-amiga` (pre-restart). Its
shape:

| Area | Archive state |
| --- | --- |
| `Model::A500OcsPal` enum + `profile_for` metadata | ✅ reusable verbatim |
| `AmigaRuntime::new(model, kickstart)` | ✅ bridge pattern works |
| `MachineCore` impl — `profile/time/reset/load_media/run_until` | ✅ pattern works |
| `SessionQueryProvider` dispatching dotted string paths | ✅ pattern works |
| 60+ query paths referencing old-machine fields | ❌ all point at pre-restart Amiga API |
| 40+ `examples/` debugging tools for pre-restart internals | ❌ stale |
| Audio via `take_audio_buffer()` | ❌ new machine doesn't expose this yet |
| Viewport extraction via Denise archive's `extract_viewport` | ❌ unneeded — `AmigaOcs` framebuffer is already 768×576 |

## Known divergences / simplifications

1. **No audio output yet.** The runtime pushes empty `AudioPacket`s.
   Paula has `mix_audio_stereo()` at the chip level; wiring a
   sample-rate-correct resample buffer through `AmigaOcs` and out to
   the runtime is a separate follow-up (not blocking the port).

2. **Trimmed query-path set.** The archive exposed ~60 paths wiring
   deep into old-machine internals (trackdisk debug logs, CIA read
   counters, etc.). The port ships ~20 paths covering the fields
   `AmigaOcs` already exposes cleanly. More can be added lazily when
   verifier UI panels need them.

3. **All `examples/` binaries dropped.** They were one-off
   diagnostics for the pre-restart investigation. Git history
   preserves them; the live crate ships with the lib + a `tests/`
   directory only.

4. **Framebuffer is direct, not viewport-extracted.** The archive
   ran `Denise::extract_viewport(Standard, pal=true, deinterlace=
   true).to_display()` — a scale + crop through the archive's
   superhires raster buffer. `AmigaOcs` already produces a 768×576
   ARGB framebuffer (pixel- + line-doubled for square-pixel 4:3),
   so the runtime just copies ARGB → RGBA.

## Per-phase execution

**Phase 1–3 combined** — the archive's purpose was to bridge the
*old* machine; the live crate bridges the *new* one, so the port is
a wholesale rewrite in a single commit. Sub-steps:

1. Swap `machine-commodore-amiga` dep → `machine-commodore-amiga-ocs`.
2. Drop examples + old query paths + audio buffer wiring.
3. Rewrite `AmigaRuntime` against `AmigaOcs` — keep the model/profile
   metadata (unchanged), rebuild machine state management
   (reset/load_media), stream frames via `denise().framebuffer()`.
4. Keep `SessionQueryProvider` with a tight query set pulled from
   `AmigaOcs`'s existing public accessors.
5. Add tests covering the `MachineCore` contract.
6. Rename archive directory to retire the `-archive` suffix.

## Conclusion

Smallest "port" by effort because the crate is a bridge, not a
chip model: there's no state machine to preserve bit-for-bit, no
HRM spec to match, just an adapter between two APIs. The risk is
confined to the `MachineCore` trait implementation, covered by
trait-contract tests in the live crate.
