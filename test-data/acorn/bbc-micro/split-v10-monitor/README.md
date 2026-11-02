# BBC version 10 states, continued with the monitor model

Captured on 2 November 2026 by
`runtime-acorn-bbc-micro/tests/board_split_compatibility.rs`
(`EMU198X_BBC_CAPTURE=<new directory>`), by restoring the frozen version 10
states in `../split-v10` and running three native frames. These files contain
no Acorn ROM.

A frame now ends where the monitor retraces, at a vertical sync it accepts or
by itself when there is none in range (emu198x/emu198x#2229), rather than when
the CRTC's counter wraps. So the observations frozen beside the states no
longer describe the machine; these replace them, and the states themselves are
unchanged. A version 10 save carries no monitor scan, so its monitor re-locks
at the next vertical sync. `after.state` is the continued state as a version
11 save, which carries the scan. The capture mode refuses to overwrite files.

- `acorn-bbc-master-128.after.state`: SHA-256 `78eb6c78c812cd5d8b388e0158eadc9d0e800ba3c5b983f17a2352b8e6dceff0`
- `acorn-bbc-master-128.audio`: SHA-256 `54adbf10372b459cdd76772c3f4a19cb52a4deb9f487627392a3c36bfb402c0f`
- `acorn-bbc-master-128.pixels`: SHA-256 `040df1f76ba526cedf5e6d8f489e22f215abd4da494b5a4dfbf38619eff34c30`
- `acorn-bbc-master-128.times.json`: SHA-256 `53527b5b8f095cc216c75eb7bf1053224493775f7f3c949ac6784525cfe03fdf`
- `acorn-bbc-micro-b-plus.after.state`: SHA-256 `6a4ba551a990b9990ca1977023c77bc737f3a4718989ea56d555c74d7385cfbf`
- `acorn-bbc-micro-b-plus.audio`: SHA-256 `54adbf10372b459cdd76772c3f4a19cb52a4deb9f487627392a3c36bfb402c0f`
- `acorn-bbc-micro-b-plus.pixels`: SHA-256 `040df1f76ba526cedf5e6d8f489e22f215abd4da494b5a4dfbf38619eff34c30`
- `acorn-bbc-micro-b-plus.times.json`: SHA-256 `53527b5b8f095cc216c75eb7bf1053224493775f7f3c949ac6784525cfe03fdf`
- `acorn-bbc-micro-b.after.state`: SHA-256 `45adf1b3761f9eeade6275998e68a4844a32b9008476f1a5cf33951d4d017ddb`
- `acorn-bbc-micro-b.audio`: SHA-256 `54adbf10372b459cdd76772c3f4a19cb52a4deb9f487627392a3c36bfb402c0f`
- `acorn-bbc-micro-b.pixels`: SHA-256 `2258062d0cda7e2b11809e7dd8061129a8c0de32a4c68ffbb0cdcdbc87cb43bd`
- `acorn-bbc-micro-b.times.json`: SHA-256 `ab71af25c0f73e491c4863987ec2fc22c5bd6b9e1a0523a5e64848a76ee45570`
