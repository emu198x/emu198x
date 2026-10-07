# Registered DDF reservation sequencer observations

These fixtures preserve executable software-reference evidence for the next
shared Agnus DMA integration. The live driver does not yet generate automatic
bitplane reservations from these stages. They do not certify the current
Emu198x scheduler or resolve the outstanding relative DMA/busy timing rows.

The generator compiles unchanged functions and cadence/plane tables extracted
from registered FS-UAE revision `f362278ccd4c60991caac3b4d240d4a3f751bea2`.
It requires `custom.cpp` SHA-256
`75bb300ba2798913997f82dde988db465746e9b7f22699bbc275690c8ae227e5`.
`verification.json` records each extracted fragment's hash and the resulting
fixture hashes. Vendored source is not edited.

## Reproduction

Use Python 3.13+ and Clang, with no additional Python dependencies. Generate
into scratch, then compare both CSV files and the verification record against
this directory. Compilation, execution, source identity and positive coverage
checks must succeed before the generator writes any fixture.

```sh
python3 build-reference.py /path/to/registered/custom.cpp /private/tmp/ddf-reference
cmp registered-cases.csv /private/tmp/ddf-reference/registered-cases.csv
cmp registered-events.csv /private/tmp/ddf-reference/registered-events.csv
cmp verification.json /private/tmp/ddf-reference/verification.json
```

The reversed-order negative control must fail with:
`request-before-comparator gate failed: first BPL1 h=64, expected 65`.
A modified source must fail its SHA check.

```sh
python3 build-reference.py /path/to/registered/custom.cpp /private/tmp/ddf-reversed --reverse-order
```

## Scope and external inputs

The extracted functions are `bprun_start`, `bpl_dma_normal_stop`,
`islastbplseq`, `generate_bpl`, `decide_bpl` and `get_cck_clock`.
The harness verifies request-before-comparator ordering from `do_cck` and
`generate_dma_requests`. Display autoscale bookkeeping is discarded, and
scandoubling is disabled. `write_rga` records one reservation without
simulating RGA arbitration, address sampling, memory service or Denise.

Inputs are sampled bitplane DMA enable, vertical-window state, effective
DDF register values, installed chipset and stable fetch cadence. The harness
supplies those inputs before the CCK, so it does not measure DMACON, DIW,
BPLCON0, FMODE or DDF register-write propagation latency. The plane input is
six for non-AGA eight-slot sequences, eight for AGA eight-slot sequences,
and the full legal plane count for the shorter sequences. Enhanced superhires also supplies the horizontal bypass, following the
registered `check_harddis` predicate; other scenarios use fixed hard limits
unless scenario 9 explicitly enables bypass. The cadence tables
are extracted from the source; the harness selects their mode/resolution
index directly. It does not execute `setup_fmodes` or register-limit decoding.

There are 336 cases, each spanning two complete physical lines without
resetting sequencer state at wrap. Both 227-CCK and 228-CCK line lengths are
used. These fixed lengths probe parity and wrap; they do not emulate the
machine's PAL/NTSC or programmable-beam length selection.

`registered-cases.csv` uses these identifiers:

| Field | Values |
|---|---|
| chip | 0 OCS, 1 ECS, 2 AGA |
| mode | FMODE bits 1:0; OCS/ECS use 0, AGA uses 0..3 |
| res | 0 lores, 1 hires, 2 superhires; OCS uses 0..1 |
| line_ccks | 227 or 228 |

All DDF values below are effective masked inputs. Initial DMA and vertical
window are enabled; run/stop/soft state is idle and the initial hard-start
gate is open, matching the registered source's deterministic static state.

| Scenario | Inputs and changes, repeated at each line's stated position |
|---|---|
| 0 | Ordinary DDFSTRT=56, DDFSTOP=208 |
| 1 | Phase-shifted start=60, stop=208 |
| 2 | Equal start=stop=56 |
| 3 | Start=28, stop=232; fixed stop and cross-wrap terminal unit |
| 4 | Early start=16, stop=208; carried hard-start permission |
| 5 | Start=56, stop=208; DMA off at 80, start rewritten to 112 at 88, DMA on at 96 |
| 6 | Start=56, stop=208; DMA off at 214, on at 222, during termination |
| 7 | Start=56, stop=96; at 112 rewrite start=128 and stop=176 |
| 8 | Start=56, stop=208; vertical window off at 80, on at 96 |
| 9 | Start=56, stop=208; enhanced horizontal hard-limit bypass enabled |

Rewrites persist into the following line. The harness does not reinitialize
registers or sequencer state at horizontal wrap. OCS omits scenario 9.

## Recorded events

`registered-events.csv` has 74,577 rows: 71,835 reservation rows and 2,742
additional state-transition rows. A row is emitted whenever a request occurs
or run, stopping, soft-enable, hard-limit or previous-enable state changes.
Idle counter increments without those events are not emitted.

`line` and `h` are the reference harness's physical positions. `clock` is
from unmodified `get_cck_clock`. `plane` is zero-based (0 means BPL1), or -1
for a transition without a reservation. `mod` records the reservation's
terminal-modulo flag; it is not a modulo value or pointer update.
`run_before`, `cycle_before`, `stop_before` describe entry to this CCK;
the corresponding `after` fields describe state after request generation
and comparator evaluation. `soft_after`, `limit_after`, `hwi_after` retain
the enhanced soft gate, hard limit and previous enable edge state.

There are 3,772 terminal-modulo requests. Every case has at least one request;
case IDs are unique and complete. The normal first BPL1 reservation is at
65, and the terminal BPL1 reservation is at 217 with modulo set. All twenty
normal BPL1 request positions match each of the 145 previously captured live
reference lines (2,900 paired requests). That cross-check covers only the
ordinary lores sequence, using the existing trace under
`/private/tmp/emu198x-shared-dma-stages/display-trace/`.

## Limits of the evidence

These are compiled software-reference observations, not a new live-machine
capture or silicon trace. The 336 cases do not establish full-machine
conflict priority, future-cell ownership, register propagation, service width,
pointer retirement, normal Denise data retirement, CPU stalls or pixels.
The older reference's FMODE=2 service-width discrepancy remains excluded
from native service validation as documented in `../display-dma-address/`;
this probe measures reservation cadence and never executes that service.

The current binding decisions retain compressed DDF start-admission and
endpoint models. Cross-wrap rows here expose reference staging that those
models do not represent; they do not independently resolve the WinUAE/vAmiga
bus-position disagreement recorded in those decisions. Future production
integration must preserve the shared slot authority, saved stage replay and
independent Denise counter, and validate both machine traces and raster output.
