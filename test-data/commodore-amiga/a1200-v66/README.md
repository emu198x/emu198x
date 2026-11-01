# Original A1200 v66 fixtures

Captured before extraction from source `cbcda19a116bdf3a86a8d41c186c046842c342b8`.
The generator is `crates/runtime-commodore-amiga/tests/a1200_board_baseline.rs`.
All firmware and floppy bytes are project-written synthetic input; no Kickstart,
Workbench or third-party software bytes are included. These prove continuation,
not historical-software qualification.

Each `.state` contains the full original production v66 save, cut after 137
additional runtime ticks following three native fields. Keyboard, mouse and
joystick controls are held and the synthetic DF0 image is persisted.
The following `.output` records three native fields: elapsed clocks, SHA-256
of each complete serialized frame packet, every audio packet timestamp/rate/
channel count and f32 sample bits, and SHA-256 of the entire final save.
Controls release in the second following field; guest code reads the change.
A separate parked guest produces black/silent output as a negative control.

Generation uses create-new writes and refuses any source HEAD after the
original base; ordinary tests only read these files. Do not regenerate them
with the extracted engine. Original and restored execution were compared
before capture.

| File | Bytes | SHA-256 |
|---|---:|---|
| commodore-amiga-a1200-aga-ntsc-interlaced.output | 14592 | `f534cb196ab98bb9de13e87d0a863666923ba3a2df00ee163b7e3ff3bb49e8d7` |
| commodore-amiga-a1200-aga-ntsc-interlaced.state | 13618223 | `09bc731c87883f15ee400e4042adbac0a2cc853202ac2a5a6b5fc11ede589c11` |
| commodore-amiga-a1200-aga-ntsc-progressive.output | 14556 | `a310b91bcaf3f6dd4221965697ff86ee4218194d55d9e17a5ae597fc7e0e1dcb` |
| commodore-amiga-a1200-aga-ntsc-progressive.state | 13618225 | `02e27723f9aae70468a1458c11fbafb4f66c815e8ba4bae9805950aaad59713b` |
| commodore-amiga-a1200-aga-pal-interlaced.output | 17454 | `534fa890cc98eb0485cace66ca02a4ab45a51976de8aada4e1d46036013ccd84` |
| commodore-amiga-a1200-aga-pal-interlaced.state | 13618221 | `4f2fcf5a4c49a162b9b1d14dd9d45ea6e8f82fd134331d229a91fab459009bfe` |
| commodore-amiga-a1200-aga-pal-progressive.output | 17418 | `359964d1b6c5c8cd45cda1e94bf4c1d64e89858d22a4b3f9920a3b46c8b6e4c8` |
| commodore-amiga-a1200-aga-pal-progressive.state | 13618226 | `a1cfb0aba2a01295ca8b79f1dc50b8da4c837330569fb2b1f93e8f005272e3aa` |
