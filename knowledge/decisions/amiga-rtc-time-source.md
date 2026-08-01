# Decision: Amiga RTC time source

**Date:** 2026-08-01
**Status:** BINDING

## The question

What advances the battery-backed Amiga real-time clock during execution,
snapshot restore and deterministic replay?

## Decision

The RTC has two explicit clock modes:

- `emulated` advances from completed Amiga system ticks;
- `host-synchronized` advances from elapsed host wall time.

New machines use `emulated` mode. Construction samples host time once to seed
the initial whole-second value, then host scheduling and emulator throughput
cannot change guest-visible time. Tests and other controlled callers can use a
fixed Unix timestamp instead of sampling the host.

The shared Amiga driver advances the RTC once for every completed system tick.
PAL and NTSC machines supply their own system-tick rate. A stopped or held RTC
retains both its whole-second value and its fractional phase.

`host-synchronized` mode remains available at the RTC component boundary for
hosts that deliberately want wall-clock progression. Selecting that mode is an
external-input decision; it is not inferred from machine hardware
configuration.

## Snapshot and query consequences

The snapshot stores the clock mode, whole-second value, subsecond system-tick
phase and phase rate. Amiga runtime snapshot schema version 32 rejects version
31 because the positional RTC payload changed.

An emulated RTC restores at the exact saved phase. A host-synchronized RTC is
normalized to its visible whole second when captured and receives a fresh host
anchor when restored.

In the default emulated mode, grouped and leaf RTC queries sample stable state.
Repeating a query without advancing the machine returns the same value.

## Boundaries

The default constructor still samples the initial timestamp from the host.
Fresh RTC state is identical only when the caller supplies the same timestamp
or starts from the same snapshot. A future runtime-builder option can expose
that seed without placing an external input in `AmigaConfig`.

UTC-versus-local civil-time policy and the presence of an RTC in each machine
profile are separate questions. This decision does not change either one.

## Validation

Component tests cover exact second and calendar rollover, large batched
advances, PAL/NTSC phase rates, HOLD, STOP, clock-mode transitions and snapshot
normalization. Runtime tests cover query stability, byte-level snapshot fixed
points and exact forward replay.

## Related Documents

- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)
- [Save-state format](save-state-format.md)
- [Runtime internal shape](runtime-internal-shape.md)
