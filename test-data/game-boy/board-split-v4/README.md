# Pre-split Game Boy version-4 state and native output

Captured at `82baa05ef` before moving the shared machine. The recipe is
`runtime-nintendo-game-boy/examples/board_baseline.rs`; all cartridge and
firmware bytes are generated project code, with no Nintendo firmware.

Seven existing profiles plus five optional-firmware cases retain the v4
machine/envelope exactly. Normal cases include joypad, timer, OAM DMA and,
on colour boards, banked WRAM/VRAM, palettes, HDMA and a speed switch. Their
CPU executes from HRAM when OAM DMA blocks ROM. Boot cases instead capture
still-mapped firmware and verify its subsequent unmap/handoff; SGB boot-ROM
execution is not implemented and has no fabricated boot case (#326).

Each `.frames` file is the postcard-encoded vector of all three emitted
`CapturedFrame`s, including timestamps, dimensions, format, palette and every
pixel. `.audio` retains every stereo float's little-endian bits; `.after.state`
retains the entire continued state. The debugger initially advances the
machine without advancing runtime time. Already-produced host audio is drained
before capture: the APU intentionally excludes that queue from saves, and the
recipe verifies draining leaves the serialized hardware bytes unchanged. Keep these observations fixed across
the split and retain the independently generated genuine v3 fixtures.

- `agb-boot.after.state`: SHA-256 `1b36523d8861fa709c641d4209849a89f85ac1236f8eecced9ada77cee7152de`
- `agb-boot.audio`: SHA-256 `ea6815aec8f6a937248362d19a380231384a28c33714eba12c2ebf5e88f5687b`
- `agb-boot.frames`: SHA-256 `2d7a83fdd607964889f21d88d5ee233b93a218200ffe8bd8a386fd26ea0773c5`
- `agb-boot.state`: SHA-256 `c9e1c80bf196f653a71bcd6997da246753e30d501293fdc3b7c5fe5f4e6ca966`
- `agb.after.state`: SHA-256 `3ba7bf9290d303924f47951dfea2cc1b833f8df8b8ab515b4b406b43c9f2d1b3`
- `agb.audio`: SHA-256 `dea3b718ff253de76a3eba55101b37af795e00b134c503a7d97b87f932d6b5c3`
- `agb.frames`: SHA-256 `73aa860338dd2218aaace98ed4d92f2371015cdffd61a97d2469fe08bb9a47d6`
- `agb.state`: SHA-256 `98e1d885eae923f8eed79200de0378e2ed9db48cf1cd3d75b25bbc22ddef6dcd`
- `cgb-boot.after.state`: SHA-256 `e22a3917690bcf5cb276819f52873fbc30c2ea14df5fd2ca880eaa4502fe461e`
- `cgb-boot.audio`: SHA-256 `ea6815aec8f6a937248362d19a380231384a28c33714eba12c2ebf5e88f5687b`
- `cgb-boot.frames`: SHA-256 `2d7a83fdd607964889f21d88d5ee233b93a218200ffe8bd8a386fd26ea0773c5`
- `cgb-boot.state`: SHA-256 `c0b3adc75e505e468c3f6aa214b21bac3bc0e865358a42712f6849f5e85da277`
- `cgb.after.state`: SHA-256 `daf0bbfda70ad27578c37e6f3afc52e0a44f4ead5f20734a29012e569a27805b`
- `cgb.audio`: SHA-256 `dea3b718ff253de76a3eba55101b37af795e00b134c503a7d97b87f932d6b5c3`
- `cgb.frames`: SHA-256 `73aa860338dd2218aaace98ed4d92f2371015cdffd61a97d2469fe08bb9a47d6`
- `cgb.state`: SHA-256 `13287041dbd71fd7779d343fed49f7deb19b35531faf86582d77349fd05e20dc`
- `dmg-boot.after.state`: SHA-256 `193b745380cd272903ca43a5def9ae3ec8e1d40f016442982c63b34ef702b28d`
- `dmg-boot.audio`: SHA-256 `ea6815aec8f6a937248362d19a380231384a28c33714eba12c2ebf5e88f5687b`
- `dmg-boot.frames`: SHA-256 `79cb66ef9efadddef4352905dcbc1e274d01be2cbebe4a033d1e4d18a25b9157`
- `dmg-boot.state`: SHA-256 `d8457a06d3054a1a993659111460347e58b3c2fafdfa4e395805389f14ea887b`
- `dmg.after.state`: SHA-256 `d4c9f930f16de5ba37dafecc9be0558b81dab932641512cfdea2ec52625f8cbb`
- `dmg.audio`: SHA-256 `34705e2c7d5b24e5019fc789cf385de71692c06e5d6989c9b8ae9bfa9be26eed`
- `dmg.frames`: SHA-256 `79cb66ef9efadddef4352905dcbc1e274d01be2cbebe4a033d1e4d18a25b9157`
- `dmg.state`: SHA-256 `2dcb08d3a21d040871a3c15a912ee6d275025f0fff61bf460f130d2b0af442a4`
- `dmg0-boot.after.state`: SHA-256 `e12adef398f4c740fd73ada7a7b4c1d7a2abb3f66b86b8ccea3fb9c286e2f350`
- `dmg0-boot.audio`: SHA-256 `ea6815aec8f6a937248362d19a380231384a28c33714eba12c2ebf5e88f5687b`
- `dmg0-boot.frames`: SHA-256 `79cb66ef9efadddef4352905dcbc1e274d01be2cbebe4a033d1e4d18a25b9157`
- `dmg0-boot.state`: SHA-256 `39c5e58611c6f5a0e14d4e3ff38d00280de84d8fbfbbfef10d806d8fa41cde33`
- `dmg0.after.state`: SHA-256 `887683a5062c169339cdf7230eb0ba5df5ca80a39cf0ca49d2f909a9d3538aea`
- `dmg0.audio`: SHA-256 `f3ee00c59806fbb7d3986d5162e87e011a40798c6ddcf3687da7dff956f451a3`
- `dmg0.frames`: SHA-256 `79cb66ef9efadddef4352905dcbc1e274d01be2cbebe4a033d1e4d18a25b9157`
- `dmg0.state`: SHA-256 `b143586fcfbaa83e60fe3879b3ae1b1d7c7bd3d49f4e4ccaacd226d4daf47635`
- `mgb-boot.after.state`: SHA-256 `925b75b20cc7810c270c481477b459ab405dd3ecfc4d981039a53e319d672dbf`
- `mgb-boot.audio`: SHA-256 `ea6815aec8f6a937248362d19a380231384a28c33714eba12c2ebf5e88f5687b`
- `mgb-boot.frames`: SHA-256 `79cb66ef9efadddef4352905dcbc1e274d01be2cbebe4a033d1e4d18a25b9157`
- `mgb-boot.state`: SHA-256 `cb5bb1fd7100f22230dc936e8bf634557f269215d052997155497b3ff191613e`
- `mgb.after.state`: SHA-256 `c3cf7f4eb5351ac8d4ab31fcdbed6db1b53feaf97f958e32dc4eb9bb29051655`
- `mgb.audio`: SHA-256 `34705e2c7d5b24e5019fc789cf385de71692c06e5d6989c9b8ae9bfa9be26eed`
- `mgb.frames`: SHA-256 `79cb66ef9efadddef4352905dcbc1e274d01be2cbebe4a033d1e4d18a25b9157`
- `mgb.state`: SHA-256 `25f78c3de5de35e8c3419c8c8f7adb91b45a0dc50d1861fb112396499174567d`
- `sgb.after.state`: SHA-256 `a7e92263a9d08cc8e8f23ec4a6d289317cc7652e332884e3e3aaf46969c0f701`
- `sgb.audio`: SHA-256 `7cd96b01c414d0fe34a9294b911a21aca363e66c368f382748eac3dd532bd8b2`
- `sgb.frames`: SHA-256 `79cb66ef9efadddef4352905dcbc1e274d01be2cbebe4a033d1e4d18a25b9157`
- `sgb.state`: SHA-256 `9922b6f8cd04ef2bbef8406347a7d7ed1457e5aae589dbb1442490d4486f09f0`
- `sgb2.after.state`: SHA-256 `ea2ff593230d6ad67a6b89b7b38916b146aa8a186b9d10616f27709672ebb718`
- `sgb2.audio`: SHA-256 `7cd96b01c414d0fe34a9294b911a21aca363e66c368f382748eac3dd532bd8b2`
- `sgb2.frames`: SHA-256 `79cb66ef9efadddef4352905dcbc1e274d01be2cbebe4a033d1e4d18a25b9157`
- `sgb2.state`: SHA-256 `77d552ea53424a5b5cfe9ccc224f1bb51cafb7f08292ff7227d6981d77b2d5bf`
