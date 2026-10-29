# ColecoVision NTSC

This private crate statically binds `common-coleco-colecovision` to the
NTSC board. `ColecoVision::new(bios, cartridge)` has no region argument.
The family binary/runtime and existing profile `coleco-colecovision-ntsc`
remain the entry points. Tests here qualify this board; shared wiring and
runtime tests remain shared evidence. Opposite-board states are rejected.
