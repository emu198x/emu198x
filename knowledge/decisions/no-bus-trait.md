# Original Z80 bus-interface decision — ban retired

**Original decision:** April 2026. **Ban retired:** 2026-10-28 by Steve (#2178).
The current requirement is [accurate hardware behaviour](cpu-bus-interface.md),
not a restriction on Rust interface shape.

The original Z80 implementation exposes address, data and control pins and
lets the machine inspect them between ticks. That design makes contention,
held signal levels and chip observation explicit. It remains a valid design.

The former prohibition of Bus traits, callbacks and method-call memory access
is no longer binding. Their presence is not a drift trigger and is not a
reason to rewrite otherwise accurate code. The historical reasoning and old
prohibitions remain in Git history.

Any interface must preserve the machine's signals, clock granularity,
arbitration, access side effects and timing. See
[`cpu-bus-interface.md`](cpu-bus-interface.md) and
[`RULES.md`](../../RULES.md) for the current requirements.
