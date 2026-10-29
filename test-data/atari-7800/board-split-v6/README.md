# Original atari-7800 version-6 continuations

Captured before owner extraction from the original engine/runtime at
`056bf9b7f`. The Atari 2600 profile-clock correction changes only metadata;
these original state and output files also replay after that correction.
The private test recipe is `runtime-atari-7800/src/board_baseline_tests.rs`.
Every cartridge is project-owned synthetic input.

Each case freezes complete initial/continued state, all pixels, dimensions,
formats, signal metadata and timestamps of three actual native frames, and
host audio packet timestamps/rates/channels plus every float sample bit.
Input/control and RAM state is live; instruction stepping leaves a partial
raster/native audio phase. The host timestamp is aligned to the actual core
clock before capture. Original and restored execution must match the same
whole outputs. Captures refuse an existing output directory; normal replay
fails on missing fixtures.

Both regions include active MARIA display-list DMA, controller/console input,
RAM, TIA audio and a header-selected cartridge POKEY at $4000. The audio file
also records the POKEY native sample bits drained after those three fields;
this does not claim the existing runtime mixes POKEY into its host packet.

- `atari-7800-ntsc.after.state`: SHA-256 `8b957971d0a2e6c246521ae1f797354b03d5abf22ccf44add3cd3edb1023a37e`
- `atari-7800-ntsc.audio`: SHA-256 `8ba04d5e912f9d191291512dfe3061f2b61e90a62754fa20eb651e4fd918dd74`
- `atari-7800-ntsc.frames`: SHA-256 `d4d439534d39c142cdf32a5c41ce1a8977b3f8001e3c5b1b6d91a24c29994f63`
- `atari-7800-ntsc.state`: SHA-256 `4d7166acd876a993b9ca623162b9d1026afddc2604feddd58dcfe4427636d6f5`
- `atari-7800-pal.after.state`: SHA-256 `4fd126e453432da37e913a317d83b54028f0942efb8664e61dc7c6e901505002`
- `atari-7800-pal.audio`: SHA-256 `8fa165110cf7e2798741f243d2893c562edd93fd38f044dacc23e873863081d4`
- `atari-7800-pal.frames`: SHA-256 `52c7ac5628b01bfd4c1cbc88137228067580b650ad2294771a7121060b7c5f49`
- `atari-7800-pal.state`: SHA-256 `1fe10687a9526025eab770b5ea5d1208990f969f713dfdd6065aea6e908789a0`
