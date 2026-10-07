# Paula audio interrupt and manual-playback reproduction

The approved DMA interrupt correction matches all 4,896 executable reference
observations, closing 1,744 mismatches. Startup and loop requests retain a
one-CCK delivery stage; loop conditions wait for the selected byte transition.
Amiga snapshot version 58 preserves both pending stages and rejects version 57.

The separate manual-playback research probe still reports 396 mismatches in
480 observations. It remains runnable and failing; manual playback is outside
this bounded DMA correction. No pre-existing regression was removed or ignored.

## Producer

`reference.py` extracts unmodified vAmiga register, transition, period-event,
AUDxIR and sample-output methods from revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0`. Generated C++ is retained compressed,
with source-file hashes. The scheduler adapter records the actual deadline
requested by AUDxIR and exposes INTREQ at that deadline; it retains the earliest
outstanding request, as vAmiga's `scheduleIrqAbs` does. It is a component adapter,
not a complete Paula interrupt/CPU IPL implementation or a full-machine trace.

The original HRM third edition pages 164–166 corroborate manual startup, pending
interrupt gating, and the held DMA interrupt condition. UAE's `setirq` explicitly
schedules one CCK of delay; its state 2/3 transitions corroborate intreq2 handling.

DMA: all four channels, four attachment modes, six grant opportunities and
51 observation clocks per case. A one-word buffer causes a real wrap on the
following grant. The previous startup IRQ is settled/cleared before the first
real word; only one subsequent word is delivered, isolating the loop interrupt.
The adapter honours the channel's request instead of inventing a DMA request.

Manual: all four channels, period 8, and five scenarios: ordinary startup;
startup with INTREQ already set; early acknowledgement plus a low-phase DAT
write; early acknowledgement plus a high-phase DAT write; and acknowledgement
followed by a new DAT write after playback has stopped. IRQ and DAC sample are
observed through clock 23.

The manual window intentionally stops before the second low-byte output: the
vAmiga sampler's experimental enablePenhi/enablePenlo suppression can hide
later repeated sample edges. It is not an oracle for those edges. A further
source discrepancy exists at acknowledgement exactly on the low-to-high
boundary: UAE samples pending IRQ one clock earlier, whereas vAmiga's event
handler consults it on the transition. This corpus avoids that disputed edge;
it must be measured separately before choosing its behaviour.

```sh
python3.13 test-data/commodore/amiga/paula-audio/interrupt-probe/reference.py \
  --source ../../emulators/amiga/vAmiga --output /tmp/paula-irq-reference
cargo test --locked --release -p emu198x-commodore-paula-8364 \
  --test audio_interrupt_timing -- --nocapture
```

The DMA regression requires all 96 scenarios through its exact row inventory.
The manual research probe checks all 20 remaining scenarios and exits nonzero:

```sh
cargo run --locked --release -p emu198x-commodore-paula-8364 \
  --example manual_interrupt_probe
```

`native-before.log.gz` preserves the original red reproduction of both paths.
The DMA test also checks that disabling DMA drops an unissued loop condition
without cancelling an already-issued IRQ. Component path parity covers 72,000
ticks; live board checks cover all four startup channels. Runtime replay covers
48 pending-stage checkpoints across OCS, ECS and AGA.

Extracted vAmiga code remains GPL-3.0-only, attributed to Dirk W. Hoffmann, and
is compiled only into a separate diagnostic executable. The original adapter
and observation data follow the corpus CC0 dedication.
