# DDF register-write observations

The previous native write path exposed DDFSTRT and DDFSTOP immediately. The
registered reference suppresses the start comparator during a DDFSTRT write,
retains the old stop comparator during a DDFSTOP write, then commits the
queued value after comparison. These fixtures reproduce the resulting DMA
request differences and verify the corrected saved register stage. They are
software-reference evidence, not silicon traces.

`build-reference.py` requires FS-UAE revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2`, `custom.cpp` SHA-256
`75bb300ba2798913997f82dde988db465746e9b7f22699bbc275690c8ae227e5`.
It reuses the adjacent sequencer generator and adds unchanged `DDFSTRT`,
`DDFSTOP`, `push_pipeline` and `empty_pipeline` functions. It verifies the
enclosing RGA service → request generation → comparator → queue retirement
order. The reference source is read-only; generated C++ lives in scratch.

Run with Python 3.13+, Clang and the installed Rust toolchain:

```sh
python3.13 build-reference.py /path/to/registered/custom.cpp /private/tmp/ddf-writes-reference
cmp registered-cases.csv /private/tmp/ddf-writes-reference/registered-cases.csv
cmp registered-events.csv /private/tmp/ddf-writes-reference/registered-events.csv
cmp registered-connected-events.csv /private/tmp/ddf-writes-reference/registered-connected-events.csv
cmp verification.json /private/tmp/ddf-writes-reference/verification.json
python3.13 build-reference.py /path/to/registered/custom.cpp /private/tmp/ddf-writes-negative --immediate
python3.13 run-native.py /private/tmp/ddf-writes-native --probe register
python3.13 run-native.py /private/tmp/ddf-writes-native --probe copper
```

The negative control must exit nonzero with
`immediate-write control failed: 76 cases differ`. Both native probes now pass.
They add no project dependencies or production modifications when run.

## Coverage

The 1,128 distinct case definitions cover OCS, ECS and AGA with 227- and
228-CCK lines; one lores bitplane; masked register values; writes before,
on and after the comparator; and writes across line wrap. The 24 static
controls exercise the same harness with no write. Each case spans two lines
without resetting the sequencer. The reference records 23,478 reservations.

Phase 0 places the external write before request generation/comparison, as
the connected Copper service does. Phase 1 places it after queue retirement;
the write remains pending through the next comparator. This isolates a
CPU-side register delivery boundary, without claiming CPU pin-to-write timing.

Register 0 is DDFSTRT, register 1 DDFSTOP. Ordinary cases begin with start=64,
stop=208 for start writes, or start=56, stop=64 for stop writes. Scenarios
0–4 write 60, 64, 68, 128 or 67 at each h=60..68. Scenario 5 writes zero at
the last physical CCK, from start/stop=0/0 or 216/0. Scenario 6 is the static
control. `registered-events.csv` records zero-based plane, line, horizontal
position and terminal-modulo flag for every reservation, including the absence
of requests through each case's explicit definition.

Three additional checks verify the reference queue's actual repeated-write
behaviour: start/start, start/stop and stop/start before retirement. A second
write flushes the previous shared entry. For two start writes, that flush
occurs after the second write temporarily suppresses the start comparator.
This is reference queue behaviour; the probe does not establish that two such
writes can be delivered within one physical CCK on every CPU model.

## Native reproduction

`probe-native.rs` clocks the real Agnus stages and register handlers. It
supplies vertical/DMA gates and drains descriptors without emulating RAM
service or fixed-channel conflicts. It compares every request and modulo flag
against the registered rows. Before the fix, the same 76 cases differed as in
the reference's immediate-write negative control; all 24 static controls passed.
The corrected stage matches all 1,128 cases and 23,478 reservations.

`probe-copper.rs` runs the ECS machine's shared driver with a complete Copper
list in chip RAM. A stopped CPU avoids unrelated traffic. Fifteen colour
MOVEs precede the tested DDF MOVE. The probe requires that the actual logged
write retire once at h=64, then observes requests and real BPL1PT changes:

| Write at h=64 | Reference requests on this line | Native before the fix |
|---|---|---|
| DDFSTRT 64→64 | None | Begins at 73 |
| DDFSTOP 64→128, start=56 | 65, 73 | Continues through 137 |
| DDFSTOP 64→64, start=56 | 65, 73 | 65, 73 (positive control) |

The stop rewrite causes eight extra memory reads and advances BPL1PT by 16
extra bytes. The control's requests retire into real memory service at h=67
and h=75. This is a connected native reproduction checked against compiled
reference functions; it is not a fresh full FS-UAE guest capture. The production
fix retains effective comparators and one shared pending write in version-51
runtime saves. All three connected cases now match. The ordinary Cargo suites
include both native probes.


`connected.cpp.in` additionally records the nine exact register schedules used
by the connected probe and repaired ECS regressions, including four-plane hires
start rewrites. `registered-connected-events.csv` preserves their unmodified
reference request streams. The generator verifies the missed-start rows,
first hires group, future-stop terminal group and same-cell stop controls.

Runtime tests deliver mature CPU writes through the real motherboard adapter
at both sub-CCK phases, then replay pending and retired states on all three
chipsets. They include line wrap, positive memory-service observations, malformed
candidate rejection without changing the destination, and rejection of version
50. These tests control CPU bus delivery; they do not certify CPU instruction
or pin-to-register latency.
