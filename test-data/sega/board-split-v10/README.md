# Original Sega board version-10 continuations

Captured from the unmodified machine/runtime at `796fde9e0` before the
ownership split, with the committed project-owned synthetic cartridges.
No commercial game or firmware is included. The reproducible recipe is
`runtime-sega-master-system/examples/board_baseline.rs`.

Each existing model has its original complete runtime save, the save after
three actual native boundaries, every emitted frame (dimensions, format,
timestamp and all pixels), and every emitted float audio sample bit.
The state includes active mapper/SRAM/controller/Light Phaser state, the
Japanese board's keyed-on FM channel and the Game Gear's asymmetric stereo,
Start and LCD path. Each audio reference contains a nonzero signal. Prior host
audio is drained before capture. The legacy seven-value execution enum and
nine model IDs are preserved independently. Replay checks original live
construction and restoration against these files.

- `sega-game-gear.after.state`: SHA-256 `a18eb182bd0690d5e9d2491785d913361cda6054b4c77f1debaf0a4e77a0c61d`
- `sega-game-gear.audio`: SHA-256 `9bc462a0f8b1f43cc99e82b3aa5e5ad532b71e2efb0b5d06bcc9715d0a2c0eb0`
- `sega-game-gear.frames`: SHA-256 `9f54843f7c8429f996364aafd451260af1b1e53f721ece7c80058315873e5838`
- `sega-game-gear.state`: SHA-256 `b3a27aab6a85939480ed4c9380aba3573ba42bda02458824a02d2f4c6e7f94ad`
- `sega-mark-iii.after.state`: SHA-256 `f3255aa9dbf46b26dd844b3f4bae034884d80ed295f065a578fd2cb19566872d`
- `sega-mark-iii.audio`: SHA-256 `9bc462a0f8b1f43cc99e82b3aa5e5ad532b71e2efb0b5d06bcc9715d0a2c0eb0`
- `sega-mark-iii.frames`: SHA-256 `63a071bd6e6263d4b8732785dc087a2b9ee091f8aa882cff2c889b036ed4c90d`
- `sega-mark-iii.state`: SHA-256 `a89df908aed53a9fe227d0c4050bbcdf96b029b3d38a6a2d13412525e0e9c574`
- `sega-master-system-1-ntsc.after.state`: SHA-256 `f72591445a5a3dfdd2c6a600c0ef86fa021c121346cf8aafbf9c23a6246c6508`
- `sega-master-system-1-ntsc.audio`: SHA-256 `9bc462a0f8b1f43cc99e82b3aa5e5ad532b71e2efb0b5d06bcc9715d0a2c0eb0`
- `sega-master-system-1-ntsc.frames`: SHA-256 `63a071bd6e6263d4b8732785dc087a2b9ee091f8aa882cff2c889b036ed4c90d`
- `sega-master-system-1-ntsc.state`: SHA-256 `c87a757638b6f2c6d60d4ce43e0dcb9dcebf84a32f0ed683b6291f816bb223d6`
- `sega-master-system-1-pal.after.state`: SHA-256 `7170118986c86042ee9ccd5716edf27a4f9693dc4fc00c64799f0653c6125d65`
- `sega-master-system-1-pal.audio`: SHA-256 `56812e0358ed4b494849883b0f466e1fa6cd74800ac5826ef6565cae9a7b12e5`
- `sega-master-system-1-pal.frames`: SHA-256 `2f72550709c0635f58616ed7ae9f0c1b6a8cf5fabb55fc3a248cf48706ebdd74`
- `sega-master-system-1-pal.state`: SHA-256 `f10ed74b29c9753b0a999a544b26054ec9dbdfb71c31b7b889ac1b03b466eb53`
- `sega-master-system-2-ntsc.after.state`: SHA-256 `1a6f73989ba34b4152a332fc99e6e9942dcfcdb643cbfd1aefd171a9771fabe6`
- `sega-master-system-2-ntsc.audio`: SHA-256 `9bc462a0f8b1f43cc99e82b3aa5e5ad532b71e2efb0b5d06bcc9715d0a2c0eb0`
- `sega-master-system-2-ntsc.frames`: SHA-256 `c8d58a4e7ab2b293ebe563d326a88a2007a6b9321926d9b09804ea7f8ac4e351`
- `sega-master-system-2-ntsc.state`: SHA-256 `4eb7dd79b9799e5e0b9bb21db7105d8cbca7678ace0252e1a49d7453da235857`
- `sega-master-system-2-pal.after.state`: SHA-256 `2feaf9be7c16ce98def9cfc4ec550f0949595478d4493448ce0fc9c37bd8c637`
- `sega-master-system-2-pal.audio`: SHA-256 `56812e0358ed4b494849883b0f466e1fa6cd74800ac5826ef6565cae9a7b12e5`
- `sega-master-system-2-pal.frames`: SHA-256 `54e6e7aefe20a5f344cfec36389139ae98946017b86286dfe8a009c32943a668`
- `sega-master-system-2-pal.state`: SHA-256 `b1ab1cb10ee1da71da5cf28b27e2a25163fe5a8ccaa65e4ad32ba545e7f70703`
- `sega-master-system-japan-ntsc.after.state`: SHA-256 `219beb9d2c03d5a58606752fade8ad6b4f5d00dcfc2e9b8094ae6ad0f83770c2`
- `sega-master-system-japan-ntsc.audio`: SHA-256 `ab0985dcc6461a4fafee93d658299ba876ec7c4c146103e90952fb5705727916`
- `sega-master-system-japan-ntsc.frames`: SHA-256 `c8d58a4e7ab2b293ebe563d326a88a2007a6b9321926d9b09804ea7f8ac4e351`
- `sega-master-system-japan-ntsc.state`: SHA-256 `c8166e68b2cbc9c397414e431626811431ee72cd6d7748538ab900c4871af2c0`
- `sega-master-system-ntsc.after.state`: SHA-256 `832d7d12c3c9309d120d918897a96b3ad48ee15c76818f401246969fdb20ccd0`
- `sega-master-system-ntsc.audio`: SHA-256 `9bc462a0f8b1f43cc99e82b3aa5e5ad532b71e2efb0b5d06bcc9715d0a2c0eb0`
- `sega-master-system-ntsc.frames`: SHA-256 `c8d58a4e7ab2b293ebe563d326a88a2007a6b9321926d9b09804ea7f8ac4e351`
- `sega-master-system-ntsc.state`: SHA-256 `cfad314b164e610d1a80e9ed6937326649b23a511c5f4c459e9757be1737ebee`
- `sega-master-system-pal.after.state`: SHA-256 `d7930be7b6b22beec2fc8ce8c22230720fd614d52cec5186662a7a7dfa5e4f97`
- `sega-master-system-pal.audio`: SHA-256 `56812e0358ed4b494849883b0f466e1fa6cd74800ac5826ef6565cae9a7b12e5`
- `sega-master-system-pal.frames`: SHA-256 `54e6e7aefe20a5f344cfec36389139ae98946017b86286dfe8a009c32943a668`
- `sega-master-system-pal.state`: SHA-256 `77c0cf5f38efa1f8db8f5cf5708fa9f293ecc19916345a995b1d7d754402e399`
