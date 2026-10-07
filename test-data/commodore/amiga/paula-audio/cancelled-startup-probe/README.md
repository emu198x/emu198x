# DAT delivery after cancelled Paula startup

Both pinned reference implementations agree on all 704 observations in 80
scenarios. The native CPU-DAT control matches; before correction the retained
DMA path disagreed in 544 rows. Its DAT arrival bypassed manual startup and
continued updating the DMA length counter. The board separately delivered a
retained word before Paula saw an already-effective DMACON clear.

`reference.py` reuses the audited `handover-probe` extraction chain. WinUAE
`c32694e338fa5f34977f522eb4898adb069d2e73` and vAmiga
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` transition/register methods execute
unchanged in separate C++ programs. The compressed sources retain upstream
copyrights; these diagnostic programs are not linked into Emu198x. The
adapter records requests but supplies only the explicitly scheduled words.
Host compatibility hacks and PWM are disabled.

The inventory crosses both startup waits, four channels, pending/clear IRQ,
and periods 1/2/8/124/65,536. Clock 0 is the prepared startup wait; clock 1
cancels and selects IRQ state; clock 2 delivers DAT after output processing.
No further IRQ acknowledgement or memory grant occurs. Rows contain:

`channel,period,second_wait,pending_irq,clock,state,irq,sample,dat,length`

The native test executes every row twice: through CPU DAT and through an
already-admitted DMA word. It compares exact state/IRQ/sample/DAT and length
change relative to clock 0. Absolute startup length conventions are outside
this check. A nonempty exact inventory is enforced on both sides.

```sh
python3.13 test-data/commodore/amiga/paula-audio/cancelled-startup-probe/reference.py \
  --winuae ../../emulators/amiga/WinUAE --vamiga ../../emulators/amiga/vAmiga \
  --output /tmp/paula-cancelled-startup
cargo test --release -p emu198x-commodore-paula-8364 --test cancelled_startup
cargo test --release -p machine-commodore-amiga-ocs --test paula_phase2_machine
cargo test --release -p runtime-commodore-amiga --test snapshot_roundtrip
```

The board test finds a real admitted Agnus descriptor in each startup wait,
changes its RAM word and the programmed location, then disables DMA. It
checks delivery from the retained address, the frozen length counter, the
IRQ-dependent playback result and the delayed interrupt. The restore test
repeats all four channels and both IRQ/wait cases on OCS/ECS/AGA at four
whole/half-CCK checkpoints around delivery: 192 snapshots.

Existing version-60 fields preserve the corrected state. The change does not
establish full DMAL timing, every simultaneous-edge priority, a new physical
DMACON write latency, live attachment switching, PWM or analogue response.
Primary observations are in
`reference/by-system/commodore-amiga/2026-paula-cancelled-startup-observations.md`
in the umbrella repository. `validation.json` records the failing baseline,
final checks and file hashes.
