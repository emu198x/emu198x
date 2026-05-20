# Decision: Spectrum joystick architecture

**Status:** Kempston migration landed 2026-05-07. Sinclair Interface 2 keyboard mapping landed 2026-05-20. Initial 2026-05-06 draft misframed the fix; revision history below.

**Drift trigger:** if you find yourself reaching for `Peripheral` to model the Kempston joystick, or adding `kempston:` fields to a machine where it shouldn't physically attach (+2A / +2B / +3), **stop and re-read this entry first.**

## What "the right architecture" actually is

Three pieces, none of them a peripheral lift:

### 1. Kempston joystick — `Peripheral`, attached only to machines that can host one

**Resolved 2026-05-07.** Kempston is now a `KempstonJoystick` peripheral
(crate `peripheral-kempston-joystick`) with `attached: bool` and `state: u8`
fields, implementing the family's `Peripheral` trait. The original
`Peripheral` trait docstring carved Kempston out as "too simple to abstract" —
that reasoning ignored optionality (a `pub kempston: u8` field is *always
there*, so every machine emulated a permanently-attached interface even
when no real user had one plugged in) and wrong-machine modelling (the
+2A/+2B/+3 carried the field in spite of having a rear-connector pinout
that doesn't physically fit a Kempston). The trait doc has been updated
to remove the carveout.

Current placement, post-migration:

| Variant | Hosts a Kempston? | Default attached state |
|---|---|---|
| 16K | yes | unattached |
| 48K | yes | unattached |
| Spectrum+ | yes | unattached |
| 128K | yes | unattached |
| Grey +2 | yes | unattached |
| Pentagon 128 | yes | unattached |
| Scorpion ZS-256 | yes | unattached |
| Timex TC2048 | yes | unattached |
| Timex TC2068 / TS2068 | yes | unattached |
| +2A | **no field** — Amstrad changed the rear connector pinout in 1987; classic Kempston interfaces don't fit | — |
| +2B | **no field** | — |
| +3 | **no field** | — |

Default is unattached on every variant. Host code (the runtime input
mapping, when it grows joystick handling) flips `attached = true` to
present the interface, then writes button bits into `state`.

### 2. Sinclair Interface 2 — runtime input mapping, not a machine-side concern

**Resolved 2026-05-20.** The grey +2 / +2A / +2B / +3 shipped with built-in Sinclair Interface 2-style joystick ports. Software-wise these aren't a separate I/O port — they're wired to the keyboard matrix, per Grussu (*Spectrumpedia Volume 1* p. 140):

|         | left | right | down | up | fire |
|---------|------|-------|------|----|------|
| Port 1  |  6   |   7   |  8   |  9 |   0  |
| Port 2  |  1   |   2   |  3   |  4 |   5  |

The runtime input layer routes `InputEvent::Button { port: 1 | 2, name }` and `InputEvent::Axis { port: 1 | 2, name, value }` through to the keyboard cache, mapping name → `SpectrumKey` per the table. Routing lives at `crates/runtime-sinclair-zx-spectrum/src/input.rs`'s `if2_button_to_key` / `if2_axis_key_pair` helpers, and is uniform across the family — every Spectrum has a keyboard matrix, so the IF2 routing works on 48K (with a real IF2 cartridge) and the +2/+2A/+2B/+3 (with the built-in side ports) by the same code path. Tested end-to-end by boot-invariant waypoints `if2_port1_fire_closes_keyboard_zero_on_plus3` and `if2_port2_fire_closes_keyboard_five_on_48k`.

The port-numbering choice (port 0 = Kempston; port 1/2 = IF2) was an arbitrary convention chosen for backward compatibility with the earlier Seam 2 Kempston wiring. Grussu's "Port 1 / Port 2" labels correspond directly to the runtime's `port: 1` / `port: 2`.

### 3. µPD765A FDC — leave it where it is

The FDC currently lives in `SpectrumAmstradClassCore` with `enabled = false` for +2A / +2B and `enabled = true` for +3 (gated by `Plus3Marker::HAS_FDC`). The pre-extraction `-plus` crate had the same shape. The Peripheral trait designer's note applies here too: the FDC is inert when disabled (`claims_port` returns false, no port decode happens), and lifting it to a peripheral is real work for no functional benefit. **Defer indefinitely**, until either:

- a second FDC-using variant arrives (e.g. a Beta-disk port plus the Amstrad FDC on a hybrid machine), or
- the catalogue grows a +2A/+2B-only test that proves the unused FDC field is causing a real problem (e.g. snapshot bloat, allocation cost).

Neither is on the horizon.

## Revision history

**2026-05-06 (initial draft, withdrawn):** proposed lifting Kempston to a
peripheral. Withdrawn after rereading the existing `Peripheral` trait
docstring, which carved Kempston out as too simple to abstract.

**2026-05-07 (mid-day, withdrawn):** proposed keeping Kempston as a `u8`
field per the trait's existing carveout, but moving the field placement
to fix the wrong-machine wart.

**2026-05-07 (afternoon, resolved):** reverted to the peripheral
approach. The carveout's "too simple" argument missed two real costs the
field-only approach can't fix without bolt-ons:

1. **Optionality.** A `pub kempston: u8` field is always there. Real
   hardware: most rubber-key 48Ks didn't have a Kempston. Software that
   probes `$1F` for an interface (some loaders do) saw zero instead of
   the floating bus, mis-detecting. A `kempston_attached: bool` flag
   gets us part-way, but it's a bolt-on; the peripheral trait already
   has a clean conceptual answer (`attached: false` = doesn't claim
   port = falls through to floating bus).
2. **Symmetry.** The Sinclair Interface 2 add-on (the 1983 cartridge,
   distinct from the +2's built-in Sinclair-IF2-style ports) has both a
   ROM cartridge slot and joystick ports. Future work models it as a
   `Peripheral`. Treating Kempston as a peripheral keeps the "what
   add-ons can plug into this machine" question answerable through one
   shape.

The `Peripheral` trait docstring was updated to remove the carveout. See
`crates/common-sinclair-zx-spectrum/src/peripheral.rs` and
`crates/peripheral-kempston-joystick/`.

## What still needs doing

**Host code paths producing `InputEvent::Button`.** The runtime accepts
port 0 (Kempston) and port 1 / port 2 (IF2) gamepad events, but no
Spectrum frontend currently produces them. The host paths need wiring
up — mirroring `emu198x-amiga` / `emu198x-dragon` which already do
it. Once those events arrive, the runtime input layer routes them
correctly out of the box (both Kempston and IF2 targets are tested).
