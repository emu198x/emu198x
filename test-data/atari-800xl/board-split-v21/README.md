# Original XL/XE version-21 state and output

Captured before board extraction from engine commit `d265bd53e`, using the
project-owned GTIA-mode cartridge. Each of the four profiles runs both the
original flat 8 KiB image and a generated XEGS 32 KiB image with a fixed copy
of that program and distinct switchable-bank markers. No manufacturer ROM is
included.

After three native fields, each case mounts a modified writable disk and a
pending XEX, holds controller/keyboard input, enables a POKEY tone, writes
main/extended RAM, selects a cartridge bank when present and advances 37 CPU
instructions into a field. The runtime clock is aligned with the live core.
The fixtures include the entire initial envelope and state after three more
native boundaries, every captured pixel/dimension/timestamp, and complete
host audio packets with exact sample bits. All version-21 envelope fields and
raw hardware serde attributes remain part of the comparison.

The private `runtime-atari-800xl/src/board_baseline_tests.rs` recipe requires
`EMU198X_XL_XE_BOARD_CAPTURE` naming a new directory. The normal replay requires
these files and compares both original execution and restored continuation.
Capture also performs an immediate restore/replay check.

SHA-256 identities:

```text
12cb3bc3e53f2c5c45a57e2eb8cee146556a56c3b386c324cbe50fecb38a4989  test-data/atari/gtia-modes/atari-800xl-gtia-modes.bin
0f3f688ac1059cb3388e36223ce04bc444a7e99e29db79991a7415b9057f511e  atari-130xe-ntsc-xegs.after.state
06ac323ee40f45d6a6986359172589baa0f7d6a64b989dcff7ea814b68d8e48c  atari-130xe-ntsc-xegs.audio
914e1c6a3b05e460828f36827d4cadb18b53a6c882820ba2dc8d33354c4e4d80  atari-130xe-ntsc-xegs.frames
ff52e5c840bb53c9dc48ed221736c2a8f3de2b7b48a750cbe4e71311d89015a3  atari-130xe-ntsc-xegs.state
2086d141166e15fc062bf2c825c0a4daf8d37a9cb99baee6f3c14d75c3c2c76f  atari-130xe-ntsc.after.state
06ac323ee40f45d6a6986359172589baa0f7d6a64b989dcff7ea814b68d8e48c  atari-130xe-ntsc.audio
914e1c6a3b05e460828f36827d4cadb18b53a6c882820ba2dc8d33354c4e4d80  atari-130xe-ntsc.frames
fe8ff8a5b0e17c1d3f3e867467327b0d213e13cfa1574f38574f9da57ee2cb1a  atari-130xe-ntsc.state
34628279f8d9ece00f86a23c81bbe74c47b509f7130db4538db9908d53d0b83b  atari-130xe-pal-xegs.after.state
83ad9867e18b3c862a32963533953a56134b3f1196f4c76c5e7f67ef12e7a8bc  atari-130xe-pal-xegs.audio
f89c126ea18b7837ade3986d6ed1cfbf8c338692cd90858887386e715fa97ee2  atari-130xe-pal-xegs.frames
4f9065e5ca0a476ba6b4be2d8c009903c2f60627da80a25cb8e4fc92dad1482e  atari-130xe-pal-xegs.state
ab20f52bb184ca11aef720d6e98e7d676121bfe6971c4b401faf5a638150ddd9  atari-130xe-pal.after.state
83ad9867e18b3c862a32963533953a56134b3f1196f4c76c5e7f67ef12e7a8bc  atari-130xe-pal.audio
f89c126ea18b7837ade3986d6ed1cfbf8c338692cd90858887386e715fa97ee2  atari-130xe-pal.frames
673b89a75c2e9583430833f10f09ae416fe7fa3d01bc11d89ef7adea4ed38d0d  atari-130xe-pal.state
4ae80e7ab4a4872117e7d095ab88f8b3108f35498d8d05584b20bb64ca6e2d9d  atari-800xl-ntsc-xegs.after.state
06ac323ee40f45d6a6986359172589baa0f7d6a64b989dcff7ea814b68d8e48c  atari-800xl-ntsc-xegs.audio
914e1c6a3b05e460828f36827d4cadb18b53a6c882820ba2dc8d33354c4e4d80  atari-800xl-ntsc-xegs.frames
314400abaa36c3af0b8d7956b0b790cb84ee9561ad9e2a1dbe3fab674db1c193  atari-800xl-ntsc-xegs.state
8eb92f729dd4aecf24f59ed30d0afbad6ad4df18508b07fd4a41d6e8e43d2d13  atari-800xl-ntsc.after.state
06ac323ee40f45d6a6986359172589baa0f7d6a64b989dcff7ea814b68d8e48c  atari-800xl-ntsc.audio
914e1c6a3b05e460828f36827d4cadb18b53a6c882820ba2dc8d33354c4e4d80  atari-800xl-ntsc.frames
5fc37a6ce1930e0a5ad5fc780c19120ce0a5d6765e236db0f41baa8676de3e47  atari-800xl-ntsc.state
a72fa9b2aec4b1b327f343780d217f3163b68f3a4a918dd55ac5cbf562555200  atari-800xl-pal-xegs.after.state
83ad9867e18b3c862a32963533953a56134b3f1196f4c76c5e7f67ef12e7a8bc  atari-800xl-pal-xegs.audio
f89c126ea18b7837ade3986d6ed1cfbf8c338692cd90858887386e715fa97ee2  atari-800xl-pal-xegs.frames
c08d7ae84b93bf30f8fec6730ec8abaf74f8402991d18d1a82071572dc0d103a  atari-800xl-pal-xegs.state
e2224d18bab079620d66d63927450a46e26d92ceb94d32da8d24e25ba1189807  atari-800xl-pal.after.state
83ad9867e18b3c862a32963533953a56134b3f1196f4c76c5e7f67ef12e7a8bc  atari-800xl-pal.audio
f89c126ea18b7837ade3986d6ed1cfbf8c338692cd90858887386e715fa97ee2  atari-800xl-pal.frames
8785bd842a9aff119d34a66bfb4325b606c79c928ac2c90a3d8859998559e38f  atari-800xl-pal.state
```
