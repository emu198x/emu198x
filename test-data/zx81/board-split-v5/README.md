# Original ZX81 version-5 continuation

Captured from the unsplit engine at `30ac372cb`, with project-owned synthetic
firmware `test-data/synthetic-firmware/sinclair-zx81.rom`.
Every existing profile is included. Each machine boots for 20 legacy frame
operations, holds A, writes RAM, threads a tape and starts recording, then
advances 37 T-states before capture. The saved envelope is exactly the
runtime's original field order, version and shared family ID.

The data records the complete starting envelope, the envelope after three
further frame operations, every ARGB32 framebuffer pixel with dimensions and
clock, and every audio sample bit with its clock. This freezes existing
behaviour, including the engine's existing video safety completion; it does
not certify every completion as a physical VSYNC edge or claim firmware
accuracy for the synthetic image. Runtime replay also checks RGBA conversion
and audio packet metadata.

The private `board_baseline_tests::capture_original_board_states` recipe
requires `EMU198X_ZX_BOARD_CAPTURE` naming a new directory. Normal replay
requires these files and fails if they are absent or altered. No original
manufacturer ROM is included.

```text
f15ff0e5c6e920ed382b05d3ff91a6517818ea6ffe6b48391fc9175e08ca9cb1  sinclair-zx81-16k.after.state
390865036b5dee476804a05cd1a898ec1e3702e21b1a2bc7008f6074a7c37733  sinclair-zx81-16k.audio
c8f5d1ceb249bad4dfd75c7a44b9582f516ce1f940c04d69c009c04e2a253100  sinclair-zx81-16k.frames
dd8330eff38ceefed522e4e81e06c51ce7fdd1222b6be34c71e2b47eac6279f3  sinclair-zx81-16k.state
9e7b41554bd2a481ef7df8a43fafde8f8bcec89ad613254e065a7d8a0ffdfc51  sinclair-zx81.after.state
c57a5c778f774666266dc5b808613ab5d95578e2429dc643656ecf5ffc6970e8  sinclair-zx81.audio
4033fceff5bfe52b258ade96f0776a4896cacde71a5d08bcb9b215cccc9b1c38  sinclair-zx81.frames
7143f766c06f2869f9f8492c183ea112be6449466ca0e48dc7229941032db6ba  sinclair-zx81.state
9ddff95efb17824981cf3055eb0a981c3e33548937814e2337b1d12be3d084fc  timex-ts1000.after.state
f379c57e0495e1e59afd950853b0da7538778514c52a4b90e0ee1e1efa53d4f0  timex-ts1000.audio
87027b0be7d987db20d0281d932bd6e1ebe3ec8dd1f8332b305e745fac831e43  timex-ts1000.frames
d725d1c287b0de54d898509bd70c1aa13d13432f4c96a591383771ff2ca24f78  timex-ts1000.state
```
