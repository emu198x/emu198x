# NES regional boards have fixed owners

**Status:** Adopted under the approved #2144 regional split programme.

`machine-nintendo-nes-ntsc` owns the implemented NTSC board and `machine-nintendo-nes-pal` owns the implemented PAL board. Both bind `common-nintendo-nes::NesCore` to an immutable region and machine identity. Each has exclusive direct tests. The raw multi-region engine and original shared regression tests belong to `common-nintendo-nes`, never to another machine crate.

The family runtime keeps its existing public model catalogue, profiles, aliases, media paths and runner. Its `machine.rs` holds a closed enum of the two concrete owners, following the existing Mega Drive pattern. This narrowly overrides `runtime-internal-shape.md`'s generic-runtime requirement for NES: closed board dispatch preserves the existing runtime/session type, while each physical board still owns construction and checked restore. The other per-concern modules remain unchanged in role.

The owners expose the live CPU, cartridge and explicit PPU register/OAM operations needed by existing debug and query paths, but no replaceable whole engine or mutable PPU/APU configuration. Snapshot construction checks the board region, PPU divider/raster, APU region, CPU phase and output geometry before constructing or changing hardware. The v3 wire payload remains the corrected pre-split format in `nes-v3-complete-resume-state.md`.

The two machine IDs are `nintendo-nes-ntsc` and `nintendo-nes-pal`; these also remain the unchanged profile IDs. A blank save has no board payload and retains the currently selected profile. A loaded foreign-board save is rejected atomically. The legacy NTSC profile's FDS compatibility is retained; it does not establish a separate Famicom motherboard owner or independent Famicom hardware qualification.

A restored FDS mapper already contains its BIOS. The runtime re-derives the reset cache from the adapter's CPU-visible `$e000..$ffff` bytes. Otherwise restoring into a blank runtime succeeds but a subsequent reset loses the machine because no host BIOS was supplied. A synthetic BIOS/disk regression reproduces that failure and checks the corrected reset/boot path. This does not add firmware to the save or change its layout.
