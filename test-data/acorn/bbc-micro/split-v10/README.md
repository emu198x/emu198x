# BBC pre-split version-10 state and continuation

Captured from `df13140e7` before moving the original model-based machine.
`runtime-acorn-bbc-micro/examples/board_baseline.rs` retains the recipe.
The MOS program and media are generated project code, containing no Acorn ROM.

The guest loop samples the keyboard and joystick through VIA reads into RAM;
the debugger deliberately returns $FF for I/O reads.

Each profile boots the synthetic CRTC/ULA setup, executes a VDU-range helper
and then loops outside HAZEL. Captures retain a partial field, both video
buffers, PSG/audio state, both VIAs, ADC conversion, keyboard/fire input and
an active motor/deck cassette waveform. The later boards retain mounted,
writable DFS media and in-flight WD1770 seek/latch state. B+ retains its paged
RAM/shadow display; Master retains 65C12 state, ANDY/HAZEL/LYNNE, sideways RAM
and the CMOS clock. The existing focused tests exercise each banking path.

The debugger advances 1,234 CPU cycles before capture without advancing the
runtime's time; three subsequent native-frame timestamps, all pixel/audio
sample bits and the entire continued envelope are frozen separately. Keep
these observations fixed across extraction; never regenerate them to fit
migrated code. Run `cargo test -p runtime-acorn-bbc-micro --test board_split_compatibility`.

- `acorn-bbc-master-128.after.state`: SHA-256 `9d6aeb5005e3552a109d678a00cb4ac5907f1051c2cdf3057bb7ed4600a2596d`
- `acorn-bbc-master-128.audio`: SHA-256 `b9867bf1b2d303249ba388987dcd8da824a48a993f4a9f408525ce31ad27d0f2`
- `acorn-bbc-master-128.pixels`: SHA-256 `040df1f76ba526cedf5e6d8f489e22f215abd4da494b5a4dfbf38619eff34c30`
- `acorn-bbc-master-128.state`: SHA-256 `a349b03053ef612aaf7012d19f1e3d286510eea30bd849d85a88108b9f489aa2`
- `acorn-bbc-master-128.times.json`: SHA-256 `1c2caf48d1f49d8b70aabcb31ccc952fb1a43027798cd98f06bfdc9a7dda69d9`
- `acorn-bbc-micro-b-plus.after.state`: SHA-256 `2f2dfb0ddfc3e4c287a3c51e14126a81f6fbb5a66e4fbcc872230777d45772d8`
- `acorn-bbc-micro-b-plus.audio`: SHA-256 `b9867bf1b2d303249ba388987dcd8da824a48a993f4a9f408525ce31ad27d0f2`
- `acorn-bbc-micro-b-plus.pixels`: SHA-256 `040df1f76ba526cedf5e6d8f489e22f215abd4da494b5a4dfbf38619eff34c30`
- `acorn-bbc-micro-b-plus.state`: SHA-256 `7f7c077ca50b8cfc673e1d6050a6e3a0563f6bd8d14032c9bff38a8c456bb402`
- `acorn-bbc-micro-b-plus.times.json`: SHA-256 `1c2caf48d1f49d8b70aabcb31ccc952fb1a43027798cd98f06bfdc9a7dda69d9`
- `acorn-bbc-micro-b.after.state`: SHA-256 `7c034c1bd1f7b1eb6641f5cc4b5d9b21750e1c6d18b0e6bc3181d56eedd29474`
- `acorn-bbc-micro-b.audio`: SHA-256 `b9867bf1b2d303249ba388987dcd8da824a48a993f4a9f408525ce31ad27d0f2`
- `acorn-bbc-micro-b.pixels`: SHA-256 `2258062d0cda7e2b11809e7dd8061129a8c0de32a4c68ffbb0cdcdbc87cb43bd`
- `acorn-bbc-micro-b.state`: SHA-256 `93b6f476f52183d3e440530f880640eefaae48674ad1d25472be7bd726711fa0`
- `acorn-bbc-micro-b.times.json`: SHA-256 `a949ede0fc0ad4e56a0ba381235ff937fa58710f0d1f54bfa45b20025cfd26cc`
