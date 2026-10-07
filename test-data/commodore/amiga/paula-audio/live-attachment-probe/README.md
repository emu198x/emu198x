# Live Paula attachment verification

The sweep finds an output-buffer fault without a timing/control mismatch in
its tested scope. It records 60,928 observations in 5,632 scenarios. Native
buffer and raw sample each disagree with WinUAE in 31,152 rows, including
1,824 sample disagreements while unmuted. The full-board example reproduces
the audible interval across OCS/ECS/AGA and all four channels, including 48
whole/half-CCK restore checkpoints.

These are explicit failing diagnostic examples, not passing accuracy gates.
Both exit 101 after completing their inventories. Production code is unchanged;
the existing 114 component tests pass. Promote the diagnostics to regression
gates with the subsequent correction.

## Reproduction

```sh
python3.13 test-data/commodore/amiga/paula-audio/live-attachment-probe/reference.py \
  --winuae ../../emulators/amiga/WinUAE --vamiga ../../emulators/amiga/vAmiga \
  --output /tmp/paula-live-attachment
cargo run --release -p emu198x-commodore-paula-8364 --example live_attachment_probe \
  -- /tmp/paula-live-attachment/winuae.csv
cargo run --release -p runtime-commodore-amiga --example paula_live_attachment_board
```

The producer reuses the audited handover/manual/interrupt extraction chain.
Pinned WinUAE `c32694e338fa5f34977f522eb4898adb069d2e73` and vAmiga
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` methods execute unchanged in
separate C++ programs. Compressed extracted sources retain their upstream
copyrights and are never linked into Emu198x. Host compatibility hacks and PWM
are disabled. Compressed CSVs contain the exact output; the generator also
writes plain CSVs for the native diagnostic. It enforces a nonempty exact
inventory and input/clock agreement before counting reference differences.

Columns:

`channel,period,dma,initial_mode,final_mode,change_clock,attachment_first,clock,state,irq_edge,buffer,sample,target_period,target_volume,request,held_loop,delivered`

Modes are 0=normal, 1=volume, 16=period and 17=both. The inventory crosses
periods 1/2/8/124, both playback modes, all channels, sixteen attachment
transitions (including unchanged controls), boundary-adjacent change clocks,
and both orderings of attachment write versus DAT arrival. The adapter offers
one additional word from the change clock onward, requiring a request in DMA
mode. It observes IRQ before acknowledging it on every clock. Target playback
engines remain idle. Grant scheduling is controlled, not full Agnus arbitration.

Both references agree in all rows on buffer, state, IRQ, target registers,
request, held loop and delivery. Their sampler output differs in 8,304 rows:
vAmiga suppresses repeated high/low output updates until another DAT arrival.
That sampler is not treated as an independent raw-output oracle. Its retained
buffer agrees with WinUAE throughout.

The native diagnostic compares every row and asserts the full row/scenario
counts. Muted mixer output is also checked. The board example follows the
period-eight volume-to-normal trace: $1122 at attached startup, then $3344 and
unmute at clock nine. References hold output zero until high entry at sixteen;
native output exposes $22. All 48 restore checkpoints replay identical chip
state, IRQs, mixer samples and final snapshot bytes, including that error.

Primary evidence:
`reference/by-system/commodore-amiga/2026-paula-live-attachment-observations.md`
in the umbrella repository. Physical ADKCON latency, complete DMAL timing,
target-channel playback and analogue/PWM response remain outside this probe.
