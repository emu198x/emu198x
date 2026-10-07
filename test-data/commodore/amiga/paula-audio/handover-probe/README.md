# Paula DMA/manual handover probe

The native research executable reproduces 2,552 sample/IRQ mismatches in 6,304
observations. It intentionally exits nonzero. Production code is unchanged;
all 112 existing component tests remain enabled and pass. Held loop conditions
differ in 236 rows; playback state differs in 3,420 rows. These counts overlap.

`reference.py` regenerates the previously audited manual adapter, then reuses
its unmodified WinUAE/vAmiga methods with explicit DMA mode edges. Pin and
source checks remain enforced. It adds vAmiga's enable/disable transitions.
WinUAE's external request callback records request lines; compatibility logging
is inert and its host low-period performance workaround is disabled. Host
hacks and PWM remain disabled. DMA DAT preparation mirrors AUDxDAT's holding
write and sampled DMA flag, then calls the original register-event handler.

Four channels, periods 1/2/8/124, eight schedules, unique edge clocks
`{1,max(1,p-1),p,p+1,2p-1,2p,2p+1}` and before/after-output ordering produce
1,408 scenarios and 12,608 rows per reference. No autonomous memory grants
occur after the initial words. The native probe compares the 704 pre-output
scenarios (6,304 rows); its public CCK call cannot apply a post-output DMA
edge without clocking again. It does not fake a second tick to fill that gap.

| Schedule | Initial playback and action |
| --- | --- |
| 0 | Manual; enable DMA at the edge |
| 1 | DMA; disable at the edge, retain IRQ |
| 2 | DMA; disable, clear IRQ before each output clock |
| 3 | DMA; disable, re-enable one CCK later |
| 4 | DMA with another holding word; disable and clear IRQ each clock |
| 5 | DMA; disable, clear IRQ at clock 2p |
| 6 | Manual; enable, disable one CCK later, clear IRQ at clock 2p |
| 7 | DMA with held loop condition; disable/re-enable, clear startup IRQ at 1 |

The initial output word is `0x1122`. DMA startup is primed by the ordinary
idle/start/dummy/real transitions at the time origin; startup IRQ is due at 1.
Schedules 4/7 preload `0x3344`. Length is 64 except schedule 7, which uses 1
and thus sets the reference loop condition through a real DAT arrival.
Observation clocks are the unique set
`{0,1,e-1,e,e+1,e+2,p,2p-1,2p,2p+1,3p,3p+1}` within the run duration.

CSV: channel, period, schedule, edge clock, post-output flag, clock, state,
visible IRQ, signed DAC sample, held loop condition. Inventory checks verify
every input/clock key and reject empty or duplicated output. WinUAE/vAmiga
state/IRQ disagree in 6,280 rows; vAmiga's repeated-edge DAC suppression is
excluded from that cross-reference count. Native samples compare with WinUAE.

The period-8, edge-9 pair distinguishes low-byte histories: schedule 5
continues at 16 after an IRQ clear; schedule 6 stops despite the same clear.
The first low-byte period began with DMA on (no early sampling stage), the
second with DMA off (early sampling still scheduled through the mode pulse).
A present-day DMA flag alone cannot describe the pending transition.

The original HRM third edition p.166, figure 5-8, preserves states 010/011
through DMA changes; only startup states have unconditional DMA-off exits.
The early-sample detail follows the separately documented WinUAE test correction.
This is component evidence, not a physical capture or a complete Agnus bus
probe. Live DMAL transfers, pending-fetch retirement, attachment switching,
PWM and analogue behaviour are not established by this corpus.

```sh
python3.13 test-data/commodore/amiga/paula-audio/handover-probe/reference.py \
  --winuae ../../emulators/amiga/WinUAE --vamiga ../../emulators/amiga/vAmiga \
  --output /tmp/paula-handover-reference
cargo run --locked --release -p emu198x-commodore-paula-8364 \
  --example dma_handover_probe
```

The last command currently fails with
`[240,264,268,228,408,408,328,408]` sample/IRQ differences by schedule.
Extracted methods retain their upstream copyright and licensing. They are
separate diagnostic executables, never linked into Emu198x. Original adapter
code, schedules and observation data follow the corpus CC0 dedication.
