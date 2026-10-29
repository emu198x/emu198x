# Original atari-2600 version-2 continuations

Captured before owner extraction from the original engine/runtime at
`056bf9b7f`. The Atari 2600 profile-clock correction changes only metadata;
these original state and output files also replay after that correction.
The private test recipe is `runtime-atari-2600/src/board_baseline_tests.rs`.
Every cartridge is project-owned synthetic input.

Each case freezes complete initial/continued state, all pixels, dimensions,
formats, signal metadata and timestamps of three actual native frames, and
host audio packet timestamps/rates/channels plus every float sample bit.
Input/control and RAM state is live; instruction stepping leaves a partial
raster/native audio phase. The host timestamp is aligned to the actual core
clock before capture. Original and restored execution must match the same
whole outputs. Captures refuse an existing output directory; normal replay
fails on missing fixtures.

Both regions include flat and F8 cartridges, an active selected bank, RIOT
RAM, keypad/paddle/joystick/fire and console input, and active TIA audio.
Software VSYNC fields are counted without forcing nominal regional lengths.

- `atari-2600-ntsc-f8.after.state`: SHA-256 `22b0e1cc082d335717f026840e106268209bdecb882a1e68b0b97bb0b1a62b62`
- `atari-2600-ntsc-f8.audio`: SHA-256 `5c955e22e841ad1e23e1b18e08c99750b5a02fb2cc5eaef01d39954c6a642a6c`
- `atari-2600-ntsc-f8.frames`: SHA-256 `7725bc0a3bb4efcbd31f61584d75ce8f70bb455e048593426e7dbefdafc66dc9`
- `atari-2600-ntsc-f8.state`: SHA-256 `e8d955c16336bb25a0a090250c8ec63881d883635fbc9d7c4546d0d45436528c`
- `atari-2600-ntsc.after.state`: SHA-256 `675bdd2c6a94a96f5e9ddab275a6dd1b7da4d3974ccaf40d93a795982f15063d`
- `atari-2600-ntsc.audio`: SHA-256 `5c955e22e841ad1e23e1b18e08c99750b5a02fb2cc5eaef01d39954c6a642a6c`
- `atari-2600-ntsc.frames`: SHA-256 `7725bc0a3bb4efcbd31f61584d75ce8f70bb455e048593426e7dbefdafc66dc9`
- `atari-2600-ntsc.state`: SHA-256 `a159374f1c63e4df14581970fb7cb53fed567900e819ac2159f9fcae2793c5a0`
- `atari-2600-pal-f8.after.state`: SHA-256 `503a0f3465b8426b6cfc309c425b7f60d5b555ec08427e15951a809149313012`
- `atari-2600-pal-f8.audio`: SHA-256 `7d48fd6158f000425f1bdef8a2883da37113796d7ff31fd1e474bc2e5b29f7be`
- `atari-2600-pal-f8.frames`: SHA-256 `d202a57240a358feb90b8c75f8b11bd1c3a63754ce27def939e1a9b1d974efeb`
- `atari-2600-pal-f8.state`: SHA-256 `8487fe44fce8fa3bbb1d32a65540c34bf7a0cec871d6621e313ffb98263633ab`
- `atari-2600-pal.after.state`: SHA-256 `584184eae2a30f891f090074006d18bd46c11360efa875a31762c1797e0ecf4a`
- `atari-2600-pal.audio`: SHA-256 `7d48fd6158f000425f1bdef8a2883da37113796d7ff31fd1e474bc2e5b29f7be`
- `atari-2600-pal.frames`: SHA-256 `d202a57240a358feb90b8c75f8b11bd1c3a63754ce27def939e1a9b1d974efeb`
- `atari-2600-pal.state`: SHA-256 `ac701b5e7e90a1d5dd3eb008d08c1a286a59da7c33690faefbbef15904e32ccc`
