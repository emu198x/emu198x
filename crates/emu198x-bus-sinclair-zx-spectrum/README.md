# emu198x-bus-sinclair-zx-spectrum

The ZX Spectrum family's wiring, with no device in it: the traits and types
its ULAs and edge-connector peripherals are connected through.

- `Ula`: how a machine drives a Spectrum-family ULA, one half-cycle at a
  time. The machine ticks the ULA before the CPU, asks whether the CPU's clock
  runs this edge, and reads the interrupt line and the floating bus.
- `MemoryBus`: how a ULA reads the screen from the machine's memory, and asks
  whether an address is contended.
- `FrameTiming` and the `timing` module: the frame each video configuration
  runs (48K, 128K, +2A, Pentagon, Scorpion), as data.
- `Peripheral` and `MemoryIntercept`: the hooks an edge-connector device
  offers, such as I/O ports, M1 fetches, a clock, and memory paged over the
  machine's own.

ULAs, peripherals and machines each implement or call these traits, so each
can be built and published without the others. The ULAs that implement `Ula`
are `emu198x-ferranti-ula-6c001e`, `emu198x-sinclair-ula-7k010e`,
`emu198x-amstrad-ula-40077`, `emu198x-timex-scld`, `emu198x-pentagon-ula` and
`emu198x-scorpion-ula`, over the shared engine in
`emu198x-sinclair-zx-spectrum-ula`. The Interface 1, Kempston joystick and
mouse, Multiface, Beta Disk and DivMMC crates implement `Peripheral`.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
