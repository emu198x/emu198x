# Tom Harte Z80 validation coverage

Observed on 2026-09-24: **1,604,000 cases executed, 1,604,000 exact matches,
zero accepted differences, zero unexpected failures** across 1,604 JSON
files. The full release-mode run completed in 7.20 seconds after compilation.

The private mirror's `v1` archive was verified before extraction:

| Input | SHA-256 |
|---|---|
| `harte-z80.tar.zst` | `8595602edaa4b4082e820be4a435a8217d7f53d438be3462cf75a64295d4d1fa` |
| `ed b3.json` | `a3fb7199e5e4831e18963bea87867facfb1e4929bcc19727a65f9ea2e05fe732` |
| `ed bb.json` | `8dc270488ab19c98681dfdfee9909d396e12609174a1167596f2df808f36afe8` |

These identify the tested bytes; the mirror tag alone does not identify an
upstream source revision. The fixtures remain external to this repository.

## Strict state comparison

The [harness](../crates/emu198x-zilog-z80/tests/single_step_tests.rs) requires
all compared registers and memory values to match. It has no accepted WZ
disagreements. Both `ed b3.json` (OTIR) and `ed bb.json` (OTDR) now match all
1,000 vectors exactly. The [repeat-boundary evidence](z80-output-repeat-wz.md)
explains why the core uses instruction-start PC+1 for non-final output
iterations and BC-derived WZ only on termination.

## Scope

This harness checks final CPU state and expected memory values after the
fixture's cycle budget. It uses cycle data to supply reads but does not compare
the bus-event trace. It is not evidence of complete cycle-level agreement.
Nonempty directory/file guards do not establish corpus completeness; the file
and case counts above describe this specific run.

## Cycle input validation

Each case must contain execution cycles. A nonempty case list alone is not
sufficient: an unchanged initial/final state with an empty cycle list would
otherwise pass without ticking the CPU.

Cycle rows deserialize as exactly three values: nullable 16-bit address,
nullable 8-bit data, and a signal string. Out-of-range numbers, malformed
rows and missing/wrong-type signal fields fail parsing. Nullable values remain
valid for idle bus periods. This validates row structure and numeric ranges;
it does not validate signal spelling or compare the emitted bus trace.

The full corpus passed with typed cycle rows and the nonempty-cycle guard on
2026-09-24: 1,604,000 executed, 1,604,000 exact, zero accepted, zero unexpected.

## Explicit single-opcode runs

`run_opcode_00` is ignored by default. When explicitly requested, it requires
its corpus and `00.json`; missing inputs fail with the selected path instead
of reporting success after an early return. With the verified corpus, the
2026-09-24 targeted run executed 1,000 NOP vectors, all exact.
