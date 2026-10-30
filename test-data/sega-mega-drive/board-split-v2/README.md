# Original Mega Drive v2 fixtures

Captured before extraction from source main `78d45bb0f0ecb41d8a6b90d9841f237bbeb7d4b2`.
The recipe is `runtime-sega-mega-drive/src/board_baseline_tests.rs`.
All cartridges are project-written; no commercial ROM is included.

Six cases cover Japan, Americas and Europe in native and Power Base Converter
modes. Each full v2 envelope contains held buttons, a six-button pad handshake,
RAM, ROM, VDP, sound and a partial field. Each output contains three full
framebuffers with dimensions and timestamps, every stereo sample bit and the
final complete save. Both direct and restored continuation must match.

The native cartridge paints a red backdrop, plays PSG audio, reads region and
pad lines, and services V-int. The converter cartridge enables Mode 4, plays a
PSG tone, selects mapper bank 3, reads its marker and counts VBlank.

Set `EMU198X_MEGA_DRIVE_CAPTURE` to a fresh directory to capture. Files are
created exclusively. Corrupting a byte in the European native output makes
the comparison fail; restoring the source fixture passes.

## SHA-256

- `genesis-native.output`: `2fe43747572fbc8b5268b1367d0b780dde37d963bbdf4f82017fdb1ba6351ee0`
- `genesis-native.state`: `88797000bb3de02c6bc55bd0683dddc1afda5a8ecbba2ec0a20211e08804da3b`
- `genesis-pbc.output`: `53e0bb7d405f096c41111286df3efda4e523ac56329b470845b2b56093072483`
- `genesis-pbc.state`: `b58f98656262a670aba5356fd31495f80fca7e5978651f02872761eb6f1352d7`
- `mega-drive-europe-native.output`: `fc07c5af99ff6e1ce0798ade69253096d9eab526e9c7fc29b14fa1b9a44a3ff3`
- `mega-drive-europe-native.state`: `d6f74e6d87bc6747db53739907f79e2e2935006acb4b561e96b06f73ab16627c`
- `mega-drive-europe-pbc.output`: `fc03d4ba2c125e54403d7b88aad9985d31bd5260fec1779d71dc4d3974c7fe69`
- `mega-drive-europe-pbc.state`: `e551c40e09cd458f50ed529ac3afaa8bf1ba57a9e43f0def0d28dc2462f0dcd3`
- `mega-drive-japan-native.output`: `8e9388c0b3797704668b21cbf6853a03d0cb14cb47f20343183e576e6d346c18`
- `mega-drive-japan-native.state`: `ccbc6fb17b2a227749b68ef9140a7c4e0d2d06399fd286a1510ba13933376832`
- `mega-drive-japan-pbc.output`: `3a34ad4fd64ff0b51dc9ca13442af3390542b525f5b1e49402175ea67c6c654d`
- `mega-drive-japan-pbc.state`: `0a16ddc19582fb6f0b716d35b9fc659018d374830f96437d87cf23839c52c49a`
- `native.bin`: `cad3be7c729b2c4d875c89d51d04ab33b4821545b720070f2ff9f453c0c2f26b`
- `power-base.sms`: `0724901b738b4419cf3327e6ff1af9eff695a5e4b417ded47760753250932efb`
