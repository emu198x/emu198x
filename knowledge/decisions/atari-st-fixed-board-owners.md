# Atari ST bank and clock configurations have fixed owners

**Status:** Adopted under the approved #2144 continuation programme.

The implemented 520ST and 1040ST configurations each have PAL and NTSC machine
owners over `common-atari-st`. The owners fix the model, physical RAM banks
and master oscillator. Monitor choice remains a peripheral, and TOS remains
externally supplied firmware. Existing profile IDs and v1 wire bytes remain
unchanged. Each owner has seven exclusive direct tests; the raw engine and
original regressions are shared evidence.

The four names describe the implemented bank/clock configurations, not every
ST/STF/STFM motherboard. This extraction does not establish every board
revision, modulator, drive, power-supply arrangement or BLiTTER configuration.
The existing reference describes the model and MMU bank distinctions in
`reference/by-system/atari-st/atari-st-reference.md` §§1 and 3 at the umbrella;
its original schematic records remain authority for more specific boards.

The shared runtime uses a closed enum in `machine.rs`, following CPC, NES and
Mega Drive. This narrowly overrides `runtime-internal-shape.md`'s generic
runtime requirement for Atari ST, preserving its existing public session
and runtime type. The other per-concern modules retain their roles.

Fixed owners expose read-only chips, mutable CPU debug access and explicit
register/protocol operations. Their entire engine cannot be replaced. Checked
binding validates model, oscillator selection, exact physical RAM banks,
RAM length, keyboard-controller clock, MFP phase and output geometry before
changing a runtime. The guest can still select 50/60 Hz or high-resolution
video independently of its board's oscillator.

Original complete v1 saves and three native continuations cover all four
configurations on both colour and monochrome monitors. First resumed pixels,
sample bits, clock deltas and final state match. Disc contents and write
protection survive reset and monitor replacement. V1 deliberately omits TOS;
a restore still requires the same firmware supplied by the host, and a blank
runtime still rejects a loaded save. V1 does not encode a firmware digest.
