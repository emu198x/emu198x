# VIC-20 pre-split version-8 compatibility fixtures

Captured with `runtime-commodore-vic-20` at `3f29610e0` before moving the
model-based machine. The retained `examples/board_baseline.rs` is the capture
recipe; never regenerate these fixtures as part of the split.

The generated 6502 program is original project code. These files contain no
manufacturer firmware. Both regions capture an in-flight CPU, VIC picture and
audio, both VIA timers, held keyboard/joystick, a low external user-port RX,
independent 3 KiB and BLK1 RAM, and a BLK5 cartridge. Native continuation uses
three real frame boundaries. Host serial/modem queues are not snapshot state.

The runtime time remains zero during the initial debugger instruction steps;
the machine clock has progressed to at least 12,345 cycles. Frame timestamps,
every pixel and audio sample bit, and the whole continued version-8 envelope
are frozen separately. The envelope retains the original cartridge image for
reset, in addition to its live decoded mapping.

Run `cargo test -p runtime-commodore-vic-20 --test board_split_compatibility`.

- `commodore-vic-20-ntsc.after.state`: SHA-256 `4cbf5e77aa29590627d0861c4e8542cc83ab572abe17cb3808671aecf1f3fbbc`
- `commodore-vic-20-ntsc.audio`: SHA-256 `b1585dabc2fbb04fbfd7c5dcf30f5b26ed69a55badf70ef0f54910bf334a4acb`
- `commodore-vic-20-ntsc.pixels`: SHA-256 `0c7534854edec144d67761d74086a1b007e5ec200ba3f911f00e6cbe9a9b3367`
- `commodore-vic-20-ntsc.state`: SHA-256 `977bf6a6cc5aff6130367214741ca3318e317f0d381f7a2fa8c3f33e60be3c47`
- `commodore-vic-20-ntsc.times.json`: SHA-256 `ac131d4890b9a9cb95beac58ad97dd2621d8cb306df4259bc2a7fa91f9adcf98`
- `commodore-vic-20-pal.after.state`: SHA-256 `8cdc44c4889f4488f1ee26db21d941d5a91d223fe45888a70d00abd5ffc71c5d`
- `commodore-vic-20-pal.audio`: SHA-256 `9f30a5cfdf8c7dbba0e6bc011897f97a2fd4b833dfdbe2117c1458d8dd7f88b4`
- `commodore-vic-20-pal.pixels`: SHA-256 `6bfd0ae6956a7aceb183e5f58cf15870de2e836a292d1fb2e7eac529fc58b36d`
- `commodore-vic-20-pal.state`: SHA-256 `629804a7e5308bf26cde4b1ec73d3345bcf3efd757ef5bd41152a2a10c7b00f0`
- `commodore-vic-20-pal.times.json`: SHA-256 `98e1c0ca727454514a0c3d39c2f3a58ff77d188a8fd3bf7a89ea0a9faae6ac9d`
