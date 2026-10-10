# Dormann interrupt and Clark decimal validation

The shared NMOS 6502 has two external-programme gates in
[`dormann_programmes.rs`](../crates/emu198x-mos-6502/tests/dormann_programmes.rs).
They exercise the public CPU pins one cycle at a time against RAM. They add no
CPU behaviour, snapshot state or dependencies.

## Reproduce

```sh
python3 scripts/prepare-dormann-tests.py
cargo test --release -p emu198x-mos-6502 --test dormann_programmes -- --include-ignored --nocapture --test-threads=1
```

The staging script downloads fixed revisions and verifies SHA-256 before
writing any files. It refuses to overwrite different existing fixtures. Use
`--output DIR` and `EMU198X_6502_DORMANN_PROGRAMMES_DIR=DIR` for another location;
the default is `~/Projects/198x/assets/test-suites/6502/dormann-programmes`.
An explicitly configured missing directory fails without fallback. Missing or
wrong-sized images fail an invoked test. Run the staging script to verify the
full identities; the Rust loader checks sizes, not cryptographic hashes.

Third-party source, listings and binaries remain external inputs. The staging
script retains sources/listings alongside the binaries.

| Programme | Upstream revision and path | Binary SHA-256 |
| --- | --- | --- |
| Interrupt | [freewilll/apple2-go](https://github.com/freewilll/apple2-go/tree/5899396fb6d578289eb67b7a83333282cade04c3/cpu), `6502_interrupt_test.bin.gz` decompressed | `986cfecf0f36a398235b5e936b4ceabef4eccf3d447d5bae3fc2a08c12b5b666` |
| Decimal | [Gopher2600](https://github.com/JetSetIlly/Gopher2600/tree/823f26b152140feccc3be79d9719140c6797e4db/hardware/cpu/tests/klaus2m5/decimal_mode), `6502_decimal_test.bin` | `03798ab778456cc350044fdbe28b4078278648892712b994cdbdda09018674e7` |

The interrupt source is Klaus Dormann's GPL-3.0-or-later programme. Compared
with [his source](https://github.com/Klaus2m5/6502_65C02_functional_tests/blob/7954e2dbb49c469ea286070bf46cdd71aeb29e4b/6502_interrupt_test.a65),
the chosen build changes only zero-page origin to $0000 and code origin to
$0800, apart from whitespace. Its success self-loop is at $0AF5.

The decimal programme is Bruce Clark's public-domain code, distributed with
Dormann's suite. This build enables A/N/V/Z/C checks for NMOS, including invalid
BCD. It starts at $0200 and stops at DONE $024B with ERROR at $000B. The harness
stops before the supplied $DB sentinel: that is STP on 65C02, not on NMOS.

## What passes and what the interrupt result means

Observed on 2026-10-10:

- Decimal: **53,953,825 cycles**, ERROR=0, **131,072 ADC and 131,072 SBC cases**
  (256 × 256 operands × both carry inputs). Reaching DONE without those counts
  does not pass. A binary-only 2A03 constructor fails with ERROR=1.
- Interrupt with a **five-cycle external feedback delay**: success loop after
  **3,016 cycles**, six IRQ assertions, six NMI assertions, eleven IRQ/BRK vector
  reads and six NMI vector reads. This delay is a controlled test stimulus,
  not a claim about a physical peripheral's propagation time.
- Immediate feedback and delays of **one through four cycles**: the programme
  stops at $0B5C after **2,721 cycles**, rejecting the B bit in an NMI stack
  frame. Dormann explicitly warns at this assertion that concurrent NMOS BRK
  and NMI can produce it. Our gate requires this particular stop, an active
  concurrent-interrupt test, and one NMI vector selected after a status push
  with B set. It does not treat arbitrary traps as success.

The feedback register is $BFFC, with asserted bits 0/1 driving IRQ/NMI. Levels
are routed every CPU bus cycle and NMI edge detection remains in the CPU. A
0–12-cycle experiment found the five-cycle completion window; at six and above
the programme's earlier IRQ timeout already fails. Keeping the immediate case
as a separate gate prevents that chosen completion window from hiding the
collision.

This distinction is necessary: temporarily disabling the core's NMI vector
hijack made the immediate-feedback programme reach its nominal success loop.
The new collision gate then failed, as it should. Restoring the original CPU
source returned all eight harness tests to passing. The existing NES
`cpu_interrupts_v2` suite provides a separate oracle for the hijack path.

Other negative controls disconnect IRQ/NMI feedback, jump directly to either
success address, or exhaust the execution budget. Each is rejected. Neither
this programme nor a delayed-input pass establishes every IRQ/NMI phase or the
interrupt routing of another machine.

Full logs and source identities are retained in the
[implementation record](https://github.com/emu198x/docs/blob/main/plans/2026-10-10-6502-dormann-programmes.md).
