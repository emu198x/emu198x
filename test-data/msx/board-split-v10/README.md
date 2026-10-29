# Original MSX version-10 profile continuations

Captured from the original machine/runtime at `055162c6c`, before extracting
board owners. All thirteen existing profiles are included; the four generic
MSX1/MSX2 regional configurations are not identified as manufacturer boards.
The recipe is `runtime-msx/examples/board_baseline.rs`. Every ROM, SUB-ROM
and cartridge in these states is project-owned synthetic data.

Each profile has the complete initial and continued runtime state, all pixels,
dimensions, formats and timestamps of three native frames, and every float
audio sample bit. The guest selects the model's real slot/subslot RAM path,
programs AY/PPI controls and MSX2 mapper/RTC ports, and runs the existing
synthetic video program. Cases include both cartridge slots, live SCC sound,
keyboard/joystick input, RAM contents and partial Z80/VDP phase. Prior host
audio is drained before capture. Replay checks both live execution and a
restore against the original whole outputs.

- `microsoft-msx1-ntsc.after.state`: SHA-256 `8037098c2f378a3252929d2c2a060c3152c49cffe3045d6e0d9d2d777bfacd65`
- `microsoft-msx1-ntsc.audio`: SHA-256 `d05a67695197df82c290c20dd9d24b8ef1a983fc22d2f5311b9beb81d11c31fb`
- `microsoft-msx1-ntsc.frames`: SHA-256 `a153e93656cd766e3a079949abf0315453513c4cbffdabd921878f0f7fe47f7c`
- `microsoft-msx1-ntsc.state`: SHA-256 `e8f4cfad2c8ace3ec0138ed210dd1d32cf100683cb77a5053aef5be6212f655f`
- `microsoft-msx1-pal.after.state`: SHA-256 `c2c9fa4f4e5e8500148ff1e628f7ad1abc0e9932d503a4faa89fb8d8e0943fcd`
- `microsoft-msx1-pal.audio`: SHA-256 `79edc736f0bbab76fbf66ca90bcb218d23e2839be536dc7095e797e4eaf2fb29`
- `microsoft-msx1-pal.frames`: SHA-256 `ea936bf91ccd35cd02753f5380fe2b50670d6e6eb1ef49248a94fcadd4091581`
- `microsoft-msx1-pal.state`: SHA-256 `656c360a34a2e43e33c6e3048d6e32962c0d5cd032450ef9d63445afef1daeaf`
- `microsoft-msx2-ntsc.after.state`: SHA-256 `a9eb19b813e1942afe81e209681c764847f8e016ebf7418d8a717957b45bed80`
- `microsoft-msx2-ntsc.audio`: SHA-256 `d05a67695197df82c290c20dd9d24b8ef1a983fc22d2f5311b9beb81d11c31fb`
- `microsoft-msx2-ntsc.frames`: SHA-256 `2f72c57ccb6e4be9a05c8176b7372e1e4b59fcace446ed66495e7d9be6474c6e`
- `microsoft-msx2-ntsc.state`: SHA-256 `9b08b181d099dafbcd2d711d8d7dbfa9f376d782675071097fbf158e20c720f0`
- `microsoft-msx2-pal.after.state`: SHA-256 `5d5a1e28b22fa9cd4102c48f27533b6ff0207b7efe900dcecd1356abea200858`
- `microsoft-msx2-pal.audio`: SHA-256 `79edc736f0bbab76fbf66ca90bcb218d23e2839be536dc7095e797e4eaf2fb29`
- `microsoft-msx2-pal.frames`: SHA-256 `b472509e16e1f4367c5a15d90be81710634d5777517e0f79355a7ac0618dd3a0`
- `microsoft-msx2-pal.state`: SHA-256 `7bc0ffb09ca496c894c34e4cc3e01409e6ecc957b5ebebb6c0472c0b569f22fb`
- `mitsubishi-ml-f80.after.state`: SHA-256 `8145faefcb58624959d3170329bb8d172e7956137076fe035902af2f3b12bf78`
- `mitsubishi-ml-f80.audio`: SHA-256 `79edc736f0bbab76fbf66ca90bcb218d23e2839be536dc7095e797e4eaf2fb29`
- `mitsubishi-ml-f80.frames`: SHA-256 `ea936bf91ccd35cd02753f5380fe2b50670d6e6eb1ef49248a94fcadd4091581`
- `mitsubishi-ml-f80.state`: SHA-256 `7a92e929df92e2dee85580a25c68f7b9f3ac68b74023c7aaa57c80c50dd36bdf`
- `mitsubishi-ml-fx1.after.state`: SHA-256 `b44bdfe7fbc35adab9596e833fc22c9b1c383d367cb8136c948d376d6b10d254`
- `mitsubishi-ml-fx1.audio`: SHA-256 `79edc736f0bbab76fbf66ca90bcb218d23e2839be536dc7095e797e4eaf2fb29`
- `mitsubishi-ml-fx1.frames`: SHA-256 `ea936bf91ccd35cd02753f5380fe2b50670d6e6eb1ef49248a94fcadd4091581`
- `mitsubishi-ml-fx1.state`: SHA-256 `d16c9083ba383520b620aefff89e282a268fe262efb79d14fde45ae0497d8556`
- `panasonic-fs-a1.after.state`: SHA-256 `028cde07446c2bb8a1fc73bf0587f870fa69f701db13e303415bc1771fb00421`
- `panasonic-fs-a1.audio`: SHA-256 `d05a67695197df82c290c20dd9d24b8ef1a983fc22d2f5311b9beb81d11c31fb`
- `panasonic-fs-a1.frames`: SHA-256 `2f72c57ccb6e4be9a05c8176b7372e1e4b59fcace446ed66495e7d9be6474c6e`
- `panasonic-fs-a1.state`: SHA-256 `373d2126647e922c674882729b22b0c43d5d255cdff193d460dfb4c937ddddc8`
- `philips-vg-8235.after.state`: SHA-256 `39816faea9ca305b2bfc8ab066a61db1137dc467529ec2344bab0b68afa79180`
- `philips-vg-8235.audio`: SHA-256 `79edc736f0bbab76fbf66ca90bcb218d23e2839be536dc7095e797e4eaf2fb29`
- `philips-vg-8235.frames`: SHA-256 `b472509e16e1f4367c5a15d90be81710634d5777517e0f79355a7ac0618dd3a0`
- `philips-vg-8235.state`: SHA-256 `c31e3fa9d45078611bbad175b93a50b574908ef88221dbdb6e9a2fe82b832e4a`
- `sony-hb-10.after.state`: SHA-256 `a786db0a274f7be55412a6493ded4bfbb861127bc6f9c2e78a2a07db02cc4d60`
- `sony-hb-10.audio`: SHA-256 `d05a67695197df82c290c20dd9d24b8ef1a983fc22d2f5311b9beb81d11c31fb`
- `sony-hb-10.frames`: SHA-256 `a153e93656cd766e3a079949abf0315453513c4cbffdabd921878f0f7fe47f7c`
- `sony-hb-10.state`: SHA-256 `3655eb45e2f8eb910786847712360dcc6aedaf3c9622598894cf28752c02ac8b`
- `sony-hb-10p.after.state`: SHA-256 `e7e24c137195fb55469279117f00b135d855fdcf68912b1e336b74d6d8b50c28`
- `sony-hb-10p.audio`: SHA-256 `79edc736f0bbab76fbf66ca90bcb218d23e2839be536dc7095e797e4eaf2fb29`
- `sony-hb-10p.frames`: SHA-256 `ea936bf91ccd35cd02753f5380fe2b50670d6e6eb1ef49248a94fcadd4091581`
- `sony-hb-10p.state`: SHA-256 `96aee8c00221b51cebf3f4b97eda5c15ec53788508957f92f8e6e020cd7563a8`
- `sony-hb-f1.after.state`: SHA-256 `05656fc8ad5e11562735a4c526ef843da7653079557666c2ad8f1aa4c32bf6ee`
- `sony-hb-f1.audio`: SHA-256 `d05a67695197df82c290c20dd9d24b8ef1a983fc22d2f5311b9beb81d11c31fb`
- `sony-hb-f1.frames`: SHA-256 `2f72c57ccb6e4be9a05c8176b7372e1e4b59fcace446ed66495e7d9be6474c6e`
- `sony-hb-f1.state`: SHA-256 `7d2e37f7658fb4fb3e0e9c65ec45064e407e15c9cd02927948c302d390be22c3`
- `toshiba-hx-10.after.state`: SHA-256 `c75367eca35955253388032b4eb8083f0af05e0b61b4ac9f01f5a059474ff64a`
- `toshiba-hx-10.audio`: SHA-256 `79edc736f0bbab76fbf66ca90bcb218d23e2839be536dc7095e797e4eaf2fb29`
- `toshiba-hx-10.frames`: SHA-256 `ea936bf91ccd35cd02753f5380fe2b50670d6e6eb1ef49248a94fcadd4091581`
- `toshiba-hx-10.state`: SHA-256 `821d4423615eefac7f2823684c9389be991ea7fe00f4368c5c5128675dfb34ce`
- `yamaha-cx5m.after.state`: SHA-256 `bf24a69fc651e25fdef795da79ac8f84c7cc85e3eaf7fbd342574e421f970e83`
- `yamaha-cx5m.audio`: SHA-256 `79edc736f0bbab76fbf66ca90bcb218d23e2839be536dc7095e797e4eaf2fb29`
- `yamaha-cx5m.frames`: SHA-256 `ea936bf91ccd35cd02753f5380fe2b50670d6e6eb1ef49248a94fcadd4091581`
- `yamaha-cx5m.state`: SHA-256 `38327817555c9cbf0d0b79fcb3a7898ac68b028d2e9f8b89973f7711c0784d26`
