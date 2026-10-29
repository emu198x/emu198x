# Original ZX80 version-4 continuation

Captured from the unsplit engine at `30ac372cb`, with project-owned synthetic
firmware `test-data/synthetic-firmware/sinclair-zx80.rom`.
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
0b6fa762cc7a217dd9e495c900b90a6c1ee333ef1b5dba160b42584c69e70113  sinclair-zx80-16k.after.state
4494d7d7f1d4250ea592c771c5f9f17d4b3fac20c8700f39a5078e7fece0f5be  sinclair-zx80-16k.audio
1d6364935865f56957e9366303b8328aa3886cd5d903fbb3fc9be0e64b410940  sinclair-zx80-16k.frames
7ad5cbd47413bc4e4903408d6ccbf00985d0b0f50fb939c9dd19810b75f5634e  sinclair-zx80-16k.state
99ce1da202d2c0b1ce9028c4e6dc2a7616fada3f84ace3984320c488b5d045fb  sinclair-zx80-usa.after.state
49efd12e13d6ade8dea092ae404a96f6f5f5fa2c8c10536b1f0638c4c242d3a4  sinclair-zx80-usa.audio
eb4e7920c8971f64a8b020dff6dcdc3256c296cffea23251ceb9ba6668d67217  sinclair-zx80-usa.frames
387a53447a2e9015c5e910c08083dcc926949e329b1db34e7e3aa60cf4c320a5  sinclair-zx80-usa.state
a1f3333b168243b3891f9093e740dd4f2e1b4c857a98db295262155d1317dc29  sinclair-zx80.after.state
7b0da014c9d3734f34eaabf8b851cf2ccf1f7e8879b8e7094990b3f1c5bdc0ff  sinclair-zx80.audio
fa8c3c9b371473de93db1b8a0cdfe7729f005f13e0d4481ae3ccf584f9a73eac  sinclair-zx80.frames
a7795b2022abe98025f25a963a626de4a335d462f410e5f0de7254c7555b9622  sinclair-zx80.state
```
