# Probe Source

The probe is a Kickstart-compatible boot block plus a position-independent
68000 payload loaded at `0x00030000`.

The payload:

1. disables DMA and interrupts;
2. disables the switchable LED audio filter through CIA-A port A bit 1;
3. programs a single Paula audio channel;
4. publishes the `PAUD` ready record at `0x0002ff00`;
5. enables master and selected-channel DMA; and
6. increments the ready-record field counter while leaving the audio
   configuration unchanged.

The sample buffer contains 256 repetitions of the signed sample pair
`+127, -127`. The large buffer keeps ordinary capture windows away from DMA
startup while preserving a phase-continuous repeating waveform.

The payload does not install interrupt handlers or depend on an operating
system after the boot-device read completes.

## Related files

- [`probe.S`](probe.S) implements the on-machine behaviour.
- [`bootblock.S`](bootblock.S) loads the payload.
- [`custom-registers.inc`](custom-registers.inc) defines the used hardware
  addresses and ready-record layout.
- [`../cases/cases.json`](../cases/cases.json) supplies per-case constants.
