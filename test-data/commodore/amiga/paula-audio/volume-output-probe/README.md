# Paula volume output investigation

Paula's documented volume control gates samples over 64 clocks. This corpus
distinguishes that waveform from scalar gain and preserves the unresolved
reference disagreement about pulse phase. It is research evidence, not a
new golden timing oracle or a completed production fix.

`reference.py` checks pinned revisions and source bytes, compiles unchanged
Minimig RTL with installed Icarus Verilog, and extracts WinUAE's unchanged
PWM stepping prefix. The extracted C++ retains its upstream copyright;
WinUAE's GPL licence applies to the excerpt. It is not linked into Emu198x.
No dependency has been added to the application or workspace.

The fixture starts manual playback through register pins, holds both sample
bytes at 0x40 and the period at 1000, and measures 64 CCKs. All 128 volume
encodings are covered, followed by changes from volume 32 to seven selected
values at each counter phase: 576 scenarios, 36,864 rows per reference.
There are no DMA, attachment, byte-boundary or physical bus-latency claims.

Columns (headerless compressed CSV):

`dynamic_write,value,write_phase,clock,counter,volume,raw_sample,gated_sample`

WinUAE starts each isolated kernel scenario with Minimig's observed counter
value. This synthetic alignment isolates stepping direction, not startup
timing. The harness excludes WinUAE's scheduler and FIR. Both paths satisfy
all steady-duty invariants, but gating differs in 19,530 rows. The 448 dynamic
windows differ from scalar integrated output in 375 (Minimig) and 378
(WinUAE) cases. The scalar calculation in the Python report is algebraic;
the separate Rust diagnostic executes the actual native mixer.

The native diagnostic currently exits 1 after observing all 36,864 rows and
reporting 26,464 instantaneous mismatches. Its expected error is
`native scalar output differs from PWM reference`. This is a known-gap
diagnostic, not a normal passing regression. It must not be used to select
Minimig's pulse phase as hardware truth. Production and snapshot version 60
remain unchanged. The existing steady-gain test's incorrect description of
PWM as an approximation has been corrected.

Run from the emulator repository:

```sh
python3.13 test-data/commodore/amiga/paula-audio/volume-output-probe/reference.py \
  --winuae ../../emulators/amiga/WinUAE \
  --minimig ../../emulators/amiga/Minimig-AGA_MiSTer \
  --output /tmp/paula-volume-output
gzip -dc /tmp/paula-volume-output/minimig.csv.gz > /tmp/paula-volume-output/minimig.csv
cargo run --locked --release -p emu198x-commodore-paula-8364 \
  --example volume_output_probe -- /tmp/paula-volume-output/minimig.csv
```

The Python checker rejects empty output and a single corrupted gate, and
checks exact scenario/clock inventories. `comparison.json` retains source
and artifact hashes. A second regeneration must match the artifacts exactly.
Primary evidence and the remaining phase/resampling questions are recorded
in `reference/by-system/commodore-amiga/2026-paula-volume-output-observations.md`
in the umbrella repository.
