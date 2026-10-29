# CPU bus interfaces: hardware behaviour determines the contract

**Original decision:** 2026-04-09. **Interface-shape ban retired:** 2026-10-28,
by Steve; recorded in #2178. The accuracy requirement remains binding through
[`RULES.md`](../../RULES.md) rules 5 and 6.

## The decision

Model the machine's hardware signals and timing accurately. Public pin fields,
Bus traits, callbacks and read/write methods are permitted interface shapes.
Choose the shape that represents the hardware and makes its behaviour testable.
A trait's presence is not grounds to reject an implementation or rewrite a CPU.
A pin-based API is not, by itself, evidence that the model is correct.

The original ban tried to protect multi-chip interactions: another chip may
observe an address or control line, hold a CPU clock, own the bus for DMA or
acknowledge an interrupt. Those interactions still have to be represented at
their actual modelled times. The mistake was treating one Rust API shape as
the only way to preserve them.

## What remains required

- Keep the master-clock and evidence-backed CPU granularity requirements.
  Interface freedom does not permit instruction-level timing approximations,
  extra clocks hidden in reads/writes or catch-up work outside the clock model.
- Preserve the address, data and control signals that affect the machine,
  including their driven/held values between observation points.
- Preserve bus ownership, waits, contention, DMA and interrupt ordering. A
  read/write operation must occur at the hardware point it represents.
- Keep live memory mappings and side effects visible to the chips that use
  them. A convenient interface must not substitute stale frame-level copies.
- Verify observable behaviour against hardware evidence and the existing
  regression/reference corpus before changing an interface. Do not infer
  equivalence from matching signatures or final output alone.

These are behavioural requirements, not a requirement for public struct
fields, a particular trait or a particular callback signature. Existing pin
interfaces can remain as they are. Retiring the ban does not request a sweep
of CPU/chip code or authorise unrelated architectural rewrites.

For example, the MC68000 interrupt-acknowledge decision still requires the
accepted level to determine the acknowledgement. Re-sampling changing input
lines would be wrong regardless of whether the accepted level is represented
by fields or an interface operation.

## Drift triggers

Re-consult this record when an interface change would hide a signal another
chip observes, move an access or side effect to another clock, bypass bus
arbitration, lose a held value, or replace live data with a stale copy. Reject
those behavioural regressions, rather than a trait or callback by name.

Do not reintroduce the retired ban in generated repository guidance. Existing
extracted repositories adopt corrected guidance when next touched; #2178 does
not require a repository sweep.

## Related

- [Original Z80 interface decision](no-bus-trait.md) — its ban is retired too.
- [Half-cycle signals](half-cycle-signals.md) — timing and signal observation.
- [Accepted MC68000 interrupt level](motorola-68000-interrupt-acknowledge-level.md).
- [DMA chip memory interface](dma-chip-memory-interface.md) — live memory views.
