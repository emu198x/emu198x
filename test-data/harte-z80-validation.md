# Tom Harte Z80 validation coverage

Observed on 2026-09-24: **1,604,000 cases executed, 1,602,001 exact matches,
1,999 accepted WZ differences, zero unexpected failures** across 1,604 JSON
files. The full release-mode run completed in 8.74 seconds after compilation.

The private mirror's `v1` archive was verified before extraction:

| Input | SHA-256 |
|---|---|
| `harte-z80.tar.zst` | `8595602edaa4b4082e820be4a435a8217d7f53d438be3462cf75a64295d4d1fa` |
| `ed b3.json` | `a3fb7199e5e4831e18963bea87867facfb1e4929bcc19727a65f9ea2e05fe732` |
| `ed bb.json` | `8dc270488ab19c98681dfdfee9909d396e12609174a1167596f2df808f36afe8` |

These identify the tested bytes; the mirror tag alone does not identify an
upstream source revision. The fixtures remain external to this repository.

## Accepted differences

The [harness](../crates/emu198x-zilog-z80/tests/single_step_tests.rs) permits a
single WZ mismatch for repeating OTIR (`ed b3`) and OTDR (`ed bb`):

- Observed WZ must be BC after decrementing B, plus one for OTIR or minus one
  for OTDR, using 16-bit wrapping arithmetic.
- Reference WZ must be initial PC plus one, also wrapping.
- The fixture must describe a repeating iteration: initial B is not one,
  the budget is 21 T-states, final PC returns to initial PC, B decrements and
  C is unchanged.
- Every other compared register and memory value must match.

The rule preserves the existing output-repeat implementation's BC-derived WZ
and restricts the exception to its documented difference from these vectors.
It does not adjudicate the underlying hardware question. The output-repeat
path is in `execute_outi_outd` in `src/execute.rs`; FUSE 1.7.0 also retains
BC-derived WZ. See [FUSE validation coverage](fuse-z80-validation.md) for the
separate reference disagreements and repeated-input WZ control experiment.

| Opcode file | Exact | Accepted | Unexpected |
|---|---:|---:|---:|
| `ed b3.json` | 1 | 999 | 0 |
| `ed bb.json` | 0 | 1,000 | 0 |

Accepted differences are reported separately from exact matches. A change to
WZ outside the rule fails, even though WZ is the same register that already
had an exception. Exact cases remain exact; shrinking the accepted count is
not itself an error if every checked value matches the reference.

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
2026-09-24: 1,604,000 executed, 1,602,001 exact, 1,999 accepted, zero unexpected.
