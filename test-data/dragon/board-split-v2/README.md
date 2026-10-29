# Original Dragon and CoCo version-2 continuations

Captured from the original machine/runtime at `796fde9e0`, before extracting
board owners. The recipe lives in `runtime-dragon/src/board_baseline_tests.rs`;
the explicitly ignored capture helper requires a new output directory in
`EMU198X_DRAGON_BOARD_CAPTURE`. Ordinary replay always requires these files.
All firmware, guest code and media are project-owned synthetic inputs.

The four existing profiles and a Dragon 64 native-mode case each include the
whole initial envelope, whole continued envelope, all pixels/dimensions/
timestamps of three emitted native frames, and all float audio sample bits.
The guest configures PIA lines and drives the DAC. Captures include keyboard
and joystick state, SAM/CPU phase after partial stepping, mounted synthetic
CAS/VDK media and Dragon 64 ACIA input. Prior host audio is drained before the
snapshot. Both original live execution and restored execution must match.

- `coco1.after.state`: SHA-256 `f443006aef63325f2be8b73f0eadeeb88c53caced2485baea6ebad5735a3052f`
- `coco1.audio`: SHA-256 `197ea632d0552c7de41e54613f8ec3301cf2c2432bdfdf857fd1475607ba8018`
- `coco1.frames`: SHA-256 `f7874dcfb8ccea07b3a0090ced25a2a950fd64ed98ee51a6082449494f534ed7`
- `coco1.state`: SHA-256 `3c7f64d03d0b79798d059d8d450324c5dffbba28554dd0c721dc163bd5cda26a`
- `coco2.after.state`: SHA-256 `29d73cb96d8bbd710fc9db11da2781d3f2ed1813a3ec794fc16e490af1ad408a`
- `coco2.audio`: SHA-256 `197ea632d0552c7de41e54613f8ec3301cf2c2432bdfdf857fd1475607ba8018`
- `coco2.frames`: SHA-256 `f7874dcfb8ccea07b3a0090ced25a2a950fd64ed98ee51a6082449494f534ed7`
- `coco2.state`: SHA-256 `7668baf44eeabeb32ff3d3cf08f4475363adc107bcd5aa4116805c8e0363b798`
- `dragon32.after.state`: SHA-256 `7616ae3fedf6d257ba188d916a6704b7e6ce33bd44d8795d53e77b8845a7d402`
- `dragon32.audio`: SHA-256 `454598ef03e9ef2cd102ff9003262092e9fd5710e407ce8c2db03f2d0a1e79ba`
- `dragon32.frames`: SHA-256 `75e832f59aaff779c1e46e8ed5c8682538df71fec8a2d293bf07221a9d49d2a9`
- `dragon32.state`: SHA-256 `ee1bb9dbb2e6843758764c9cbac430a21fb116e4e215de66a610948aea25110e`
- `dragon64-native.after.state`: SHA-256 `2b3a6ed58b1f8764e96cffb7cccc2541349529c96de7e3664699af2c1ce2066b`
- `dragon64-native.audio`: SHA-256 `454598ef03e9ef2cd102ff9003262092e9fd5710e407ce8c2db03f2d0a1e79ba`
- `dragon64-native.frames`: SHA-256 `75e832f59aaff779c1e46e8ed5c8682538df71fec8a2d293bf07221a9d49d2a9`
- `dragon64-native.state`: SHA-256 `4c9c009d93e591a4feb0f9de1872bfb080a033177732b05c342152ad46e2eae3`
- `dragon64.after.state`: SHA-256 `736419a9640d2bfac21b19f6ba18f0e3ccf33a46a7213a9894693700abfd37bf`
- `dragon64.audio`: SHA-256 `454598ef03e9ef2cd102ff9003262092e9fd5710e407ce8c2db03f2d0a1e79ba`
- `dragon64.frames`: SHA-256 `75e832f59aaff779c1e46e8ed5c8682538df71fec8a2d293bf07221a9d49d2a9`
- `dragon64.state`: SHA-256 `85d8d6b0204f49699fd96f5c37fa8fc370e337062f84fe1ae9c71be8a4a972fe`

`control.rom` freezes the original synthetic input, including its obsolete
reset vector. The recipe loads its own guest through the existing program
loader before executing, so it does not fetch that vector. Current board
boot tests use the corrected `$8000` vector in the shared synthetic firmware.

- `control.rom`: SHA-256 `6ed1b1d1007c23d4d3d7e45d1669d9f9945c15c024138a8cc0242598342af293`

The legacy captures retain mounted VDK state even in the CoCo envelopes:
the old loader accepted that undeclared slot. This is save-compatibility
coverage, not a claim that a CoCo has DragonDOS hardware. CoCo profiles still
offer only their existing tape/cartridge surfaces; proper CoCo disk-controller
hardware is separate work. No new disk capability is introduced by the split.
