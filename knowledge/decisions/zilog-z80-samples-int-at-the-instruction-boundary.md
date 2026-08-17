# The Z80 samples `/INT` at the instruction boundary

**Status:** Adopted on the Spectrum's evidence, **pending cross-machine
review**. The change is landed because four independent Spectrum
instruments moved together and none regressed. It is recorded here, in
the CPU's own decision folder rather than only in the Spectrum's, because
`zilog-z80` is shared and the next machine to hit this will not read
`spectrum-contention-vs-floating-bus.md`.

## The decision

`Z80::tick` samples `/NMI` and `/INT` at the **instruction boundary** —
the `T1`↑ that begins the next `M1` — not on the retiring instruction's
last half-cycle. Retirement arms `interrupt_sample_pending`; the check
runs at the top of the next `tick`, and the dispatch below it executes
whichever phase the check leaves.

Deferring the sample costs nothing. An accepted response still begins on
the same tick it began on before, so no instruction and no interrupt
response changes length. `halt_interrupt_oracle`'s
`the_acknowledge_costs_what_fuse_charges` holds the IM 2 acknowledge at
19 T-states across every phase of the `HALT` refetch grid, before and
after.

Hosts must therefore present `/INT` **before** ticking the CPU. The
Spectrum driver's `feed_irq()` now runs ahead of `tick_cpu_and_bus()` for
this reason. A host that feeds the pin afterwards hands the CPU the
previous scheduled edge's value.

## Why — and the tension this does not pretend to resolve

**Zilog UM0080 says something different.** "The CPU samples the `/INT`
signal with the rising edge of the last clock cycle at the end of any
instruction" — the *start of the final T-state*. That is earlier than the
behaviour this record replaces (the final T-state's falling edge) and
earlier again than the behaviour it adopts (the boundary). Taken
literally, the datasheet supports neither.

**FUSE samples at the boundary**, and FUSE is this project's governing
reference for Spectrum timing
([`fuse-governs-the-contended-window.md`](fuse-governs-the-contended-window.md)).
`spectrum.c:91` wraps the frame and calls `z80_interrupt()` in the same
event handler, and FUSE runs events only between instructions.

The evidence is one-sided on the Spectrum:

| instrument | before | after |
|---|---|---|
| ZXSpectrum4.net timing survey | 8 of 69 failing | **0 of 69** |
| `halt_interrupt_oracle` | 2 of 4 failing | **4 of 4** |
| Float48K floating-bus probe | 14336 | **14337**, its expected value |
| `zilog-z80` unit, ZEXDOC/ZEXALL, FUSE corpus, Tom Harte | green | green |

The survey is the load-bearing one, and its own ratchet comment had
already described the answer without recognising it: seven of the eight
surviving failures disagreed on **`R` alone, each by exactly one**. An
interrupt accepted one instruction late runs one more `M1` fetch, and `R`
counts it. That is a completely independent corroboration — the survey
knows nothing about Float48K, floating buses or contention.

**What would settle the tension properly** is the same treatment on a
second Z80 machine with its own reference emulator, per
[`../../RULES.md`](../../RULES.md) rule 32. The Spectrum's ULA asserts
`/INT` on a raster schedule that this project has pinned three ways; a
machine whose interrupt comes from a different source (an Amstrad gate
array, a CTC, a VDP) exercises the same sampling instant against a
different assertion instant, which is exactly the axis the Spectrum
cannot vary. Until that is done, this record is adopted rather than
settled.

## What was ruled out on the way

- **Reordering `feed_irq()` alone.** Byte-identical output. At the tick
  where the old check ran, the ULA has not raised `/INT` in either order.
- **Moving the check alone.** Also insufficient, for the mirror reason:
  the pin the CPU reads at the new tick is still the previous edge's.

The two were separate half-T-state lags on the same signal, and only
correcting both moves anything. Each looked like a dead end on its own,
which is worth remembering — this is the shape that makes a real defect
read as "not it".

## Consequences for the tests

Six `zilog-z80` unit tests asserted the response phase immediately after
`step_one`, and pulsed `irq`/`nmi` low again at that point. Under the new
sampling instant the CPU is on `M1(T1Rise)` there with the sample armed,
so those tests now hold the pin through one further tick. The behaviour
they exist to check — acknowledge pins, the `R` increment and its bit-7
preservation, wait-state insertion, and half-cycle snapshot round-tripping
— is unchanged and still asserted; only the stimulus moved.

`interrupt_sample_pending` is `#[serde(default)]`, but note that the
snapshot encoding is positional: this adds a field to `Z80`'s wire
format, so snapshots written before it will not decode. That is a
snapshot format change, not a compatibility shim.

## Cross-machine review: what the validation surface actually offers

Surveyed 2026-08-13 and written up in
[`z80-validation-surface.md`](z80-validation-surface.md). The short
version: ZEXDOC/ZEXALL, Tom Harte, the `zilog-z80` unit suite, the
ZXSpectrum4.net timing survey and `halt2int128` are real gates and all
pass. Three things that looked like Z80 validation are not — the FUSE
reference suite was never running (corpus now wired in, and after fixing
a half-cycle sampling error in the harness it **passes at 1351 of
1356** — the engine agreed with FUSE all along), the
Super HALT Invaders Test never leaves its title screen, and `tape_smoke`
silently degrades to a pass when its Spectron oracle is absent.

For the second machine this record asks for, there is no Z80
machine-level suite in `assets/test-suites` and no vendored reference
emulator for the Master System. The cheapest route is SMS or SG-1000,
whose VDP raises `/INT` from a line counter rather than from the beam,
and for which `genesis-plus-gx` is already vendored under
`emulators/multi-system/`.

## Amstrad CPC: two independent references agree, 2026-08-17

The CPC turns out to be better provisioned than the survey above
assumed: `emulators/amstrad-cpc/` vendors **two** reference emulators,
Arnold and Caprice32. Both sample `/INT` at the instruction boundary,
and neither derives that from FUSE or from a Spectrum.

**Caprice32** (`src/z80.cpp`) checks after the instruction returns, in
the main execution loop:

```c
z80_execute_instruction();
z80_wait_states
if (z80.EI_issued) { ... if (z80.int_pending) { z80_int_handler } }
else if (z80.int_pending) { z80_int_handler }
```

**Arnold** (`src/cpc/z80/z80.c`) checks at the end of the opcode
dispatch, after the instruction's `switch` completes:

```c
/* check interrupts? */
if ((R.Flags & (Z80_CHECK_INTERRUPT_FLAG | Z80_EXECUTE_INTERRUPT_HANDLER_FLAG))
    == (Z80_CHECK_INTERRUPT_FLAG | Z80_EXECUTE_INTERRUPT_HANDLER_FLAG))
{
    Cycles += Z80_ExecuteInterrupt();
}
```

This matters because the CPC varies the axis the Spectrum cannot. Its
`/INT` comes from the Gate Array's 6-bit HSync counter, driven by the
CRTC, not from the beam. Our own CPC already honours the host ordering
contract this record imposes — `machine-amstrad-cpc` sets `cpu.irq =
gate_array.interrupt()` immediately before each `cpu.tick()` — and the
Gate Array's counter advances on the CRTC tick, so the assertion instant
is quantised by a different clock entirely.

**This is corroboration, not the settlement the section above asks
for.** Reading two implementations establishes that the boundary sample
is the consensus among CPC references. It does not run anything. The
"same treatment" means a *running* differential on the CPC, and that is
still outstanding.

### What blocks the running instrument

`SHAKER` (Longshot, v2.6) is the CPC's hardware-accuracy suite, covering
the Gate Array and CRTC 6845 across their manufacturing variants. It
ships as an Extended DSK, and the CPC464 modelled here has no FDC —
`machine-amstrad-cpc`'s own I/O map records the expansion/FDC decode as
"absent on a 464". The suite cannot boot as-is.

Two routes out, neither taken yet:

- Model the µPD765 and a 6128-class variant. A real feature, not a
  test-harness change.
- Extract the AMSDOS binaries from the DSK with the existing
  `format-amstrad-dsk` crate and inject them the way
  `machine-sinclair-zx-spectrum-48k`'s `z80test` harness injects its code
  block. Cheaper, but it bypasses AMSDOS, so anything SHAKER expects the
  firmware to have set up must be supplied by hand. If that route is
  taken, note what `z80test` cost us: injecting `PC` mid-instruction let
  the in-flight instruction consume the injected binary's first bytes
  (#943). Enter on an `m1` edge.

SHAKER is also aimed at video circuits; whether it measures interrupt
*acceptance* timing rather than only the counter's downstream effects is
unconfirmed.

| | |
|---|---|
| Upstream | `shaker.logonsystem.eu/Shaker_CSL/shaker26.dsk` |
| Version | 2.6, five modules `SHAKE26A`–`SHAKE26E` |
| Licence | Creative Commons, attribution requested — cite the CRTC Compendium |
| SHA-256 | `f7082f8eab521d632c343a288f54038af6df090c59b372e0d2866269c2cc4d08` |
| Size | 194,816 bytes |

## See also

- [`spectrum-contention-vs-floating-bus.md`](spectrum-contention-vs-floating-bus.md)
  — the campaign this came out of, and the `HALT` convergence loop in
  Float48K that made the defect visible.
- [`fuse-governs-the-contended-window.md`](fuse-governs-the-contended-window.md)
  — the precedent for preferring FUSE to a paper reading.
- [`z80-validation-surface.md`](z80-validation-surface.md) — which Z80
  gates are real, which are silent, and what a cross-machine proof needs.
- [`cpu-bus-interface.md`](cpu-bus-interface.md) — why the pins are public
  and the host drives them, which is what makes the ordering rule above a
  host contract rather than an internal detail.
