# Original ECS v66 continuations

Captured from untouched production source `85f99c20e45814daba85197bfc0ab662248b811b`
before the onboard RTC correction or board extraction. Four existing profiles,
progressive/interlaced video and boundary/debugger half-CCK cuts give 16 cases.
Each `.state` and `.final-state` is a complete original v66 envelope. Each
`.output` retains three complete frame-packet hashes, every audio sample bit,
packet metadata, elapsed clocks and the complete original final-save hash.
Live and restored original continuations agree exactly for every case.

The ROM and ADF bytes are project-written. The CPU program removes overlay,
writes picture/audio registers, reads held/released controls and exercises
Gayle registers where fitted. Its stack fits within stock 1 MiB RAM. Inactive
firmware is the black/silent control. No third-party firmware/media is included.
The create-new-only generator is guarded by original HEAD and clean tracked
production. Never regenerate fixtures using the corrected/extracted engine.

The original A500+ has a proven RTC-decode bug: stock slow_kb=0 leaves its
onboard RTC inaccessible. Full original final envelopes allow the approved
v67 migration to be checked without throwing away preserved clock history.
No tolerance or blanket hash re-recording is permitted for that correction.

| File | Bytes | SHA-256 |
|---|---:|---|
| active-interlaced.rom | 524288 | `d83f832c29e8595123e9dab10bcdd6f12d61666004cdee7656e43eda5fe83810` |
| active-progressive.rom | 524288 | `57752f796814a4cb5a2e71de7281a55ae81ad110087ea962b851ec5a8f6a5ce9` |
| commodore-amiga-a500-plus-ecs-ntsc-interlaced-debug-cut.final-state | 10356911 | `7afab527b7f1d9c41728ffa90ca491dd0d65d325a1ea21fe1a9556e8fa4d5a21` |
| commodore-amiga-a500-plus-ecs-ntsc-interlaced-debug-cut.output | 14592 | `2d981c501b68498cac20a112b1fd1a4ddc50fc86429d0ab6d4327124f32b0849` |
| commodore-amiga-a500-plus-ecs-ntsc-interlaced-debug-cut.state | 10356910 | `1ed794d75352a83417b8b6d80122ca058ce0604f86a9080bbf5766449ac03c60` |
| commodore-amiga-a500-plus-ecs-ntsc-interlaced.final-state | 10356911 | `7afab527b7f1d9c41728ffa90ca491dd0d65d325a1ea21fe1a9556e8fa4d5a21` |
| commodore-amiga-a500-plus-ecs-ntsc-interlaced.output | 14592 | `4b2be7665c498ea04971dfbcc079f98fc96f05f13c86071cb34ba80cb57ce3f9` |
| commodore-amiga-a500-plus-ecs-ntsc-interlaced.state | 10356913 | `7134d7350ead7c264d3504e70a269eaf1ce596dd4cdc36688addaa720e9a4dd6` |
| commodore-amiga-a500-plus-ecs-ntsc-progressive-debug-cut.final-state | 10356922 | `4ba300a4e47071bfd0b8a88669ce710a6b6baf14cab390da745c9c4630f13342` |
| commodore-amiga-a500-plus-ecs-ntsc-progressive-debug-cut.output | 14556 | `8d3e93248072e2b463a38549e5e390add4758519e7fa48f6010c7bace9d57155` |
| commodore-amiga-a500-plus-ecs-ntsc-progressive-debug-cut.state | 10356919 | `a2bc405987d205fed8c4b0faf2e0df0ca7be79e00c48cb4161c61caaf4c9e457` |
| commodore-amiga-a500-plus-ecs-ntsc-progressive.final-state | 10356922 | `4ba300a4e47071bfd0b8a88669ce710a6b6baf14cab390da745c9c4630f13342` |
| commodore-amiga-a500-plus-ecs-ntsc-progressive.output | 14556 | `d4703d580c0e53249ec95ddc8ee7278d6b0736cfe672a9d306f4f54895e36b05` |
| commodore-amiga-a500-plus-ecs-ntsc-progressive.state | 10356907 | `e4397fb0957dc9263302173fe1d738fb4ead199003487713c7a8060ce217077b` |
| commodore-amiga-a500-plus-ecs-pal-interlaced-debug-cut.final-state | 10356924 | `07253c4a8f44f4199dbbbd779287868a4383f8532b278f9cd11b7627895e9625` |
| commodore-amiga-a500-plus-ecs-pal-interlaced-debug-cut.output | 17454 | `645284b099bdd3bf5459d106c3c99b836d511fbf6d96ea4935fff7c6ab5b980b` |
| commodore-amiga-a500-plus-ecs-pal-interlaced-debug-cut.state | 10356919 | `286ed72254fed0ef116f2f25e1b53c3b7fb80c913eef2d23b940cb2803ba08d7` |
| commodore-amiga-a500-plus-ecs-pal-interlaced.final-state | 10356924 | `07253c4a8f44f4199dbbbd779287868a4383f8532b278f9cd11b7627895e9625` |
| commodore-amiga-a500-plus-ecs-pal-interlaced.output | 17454 | `b2cd628e0960e05af043595ac41e442deb2bd6195a94d1edf66337928ce6ff93` |
| commodore-amiga-a500-plus-ecs-pal-interlaced.state | 10356916 | `43e7a0c1b910396ac21169256042fc03503b56a1f4bf2fa52bc6c57dab3e6618` |
| commodore-amiga-a500-plus-ecs-pal-progressive-debug-cut.final-state | 10356912 | `9cb6c362d796924c141ab39ade0728b4f4619dc7728cda5eece0565b3673e3f4` |
| commodore-amiga-a500-plus-ecs-pal-progressive-debug-cut.output | 17418 | `a0f576762be293075b4fc08db82603c8bd07fd063eeff54b5cd51fc231866edc` |
| commodore-amiga-a500-plus-ecs-pal-progressive-debug-cut.state | 10356916 | `8ec940b8b70fe67b5df76cebfb59b863c2dd3a12b4402a243877d14c9ea526fb` |
| commodore-amiga-a500-plus-ecs-pal-progressive.final-state | 10356912 | `9cb6c362d796924c141ab39ade0728b4f4619dc7728cda5eece0565b3673e3f4` |
| commodore-amiga-a500-plus-ecs-pal-progressive.output | 17418 | `3b3fea7d6021d67c5b9b225d31a8faa492b9d8fa17af5ea3aabe9c6ecb2d3865` |
| commodore-amiga-a500-plus-ecs-pal-progressive.state | 10356916 | `f24c8abebf304e7b3c9d6d125a947ddc0d711b91607dc24e656a20c1e0227661` |
| commodore-amiga-a600-ecs-ntsc-interlaced-debug-cut.final-state | 10356915 | `5127b0cade190798658e902b5886809cbc4a47e1f29c19fa3cafc07fbe5df37d` |
| commodore-amiga-a600-ecs-ntsc-interlaced-debug-cut.output | 14592 | `5b032142ce8dc67bc9fe9c04d0c292f28863d834e1d37c3dfdff6698962ccf4f` |
| commodore-amiga-a600-ecs-ntsc-interlaced-debug-cut.state | 10356914 | `0eb243e14b7afa7611313239c5504c4a61cfb6b3c87cea02db3f0e215cee0653` |
| commodore-amiga-a600-ecs-ntsc-interlaced.final-state | 10356915 | `5127b0cade190798658e902b5886809cbc4a47e1f29c19fa3cafc07fbe5df37d` |
| commodore-amiga-a600-ecs-ntsc-interlaced.output | 14592 | `546fdca02f008c5ef14f373a4937d4a505b107801d30b1d560f9f1ecd1db6fe0` |
| commodore-amiga-a600-ecs-ntsc-interlaced.state | 10356917 | `f3886df3f8f30d85be83dcf92c58bd54786436461883b1f95abe76fb201eff57` |
| commodore-amiga-a600-ecs-ntsc-progressive-debug-cut.final-state | 10356926 | `05e24b821f8875343b754dea916b31118cff5c174609e3c6c63b0657dd10f7dc` |
| commodore-amiga-a600-ecs-ntsc-progressive-debug-cut.output | 14556 | `c3a242bff2e80dc44628703ee1b8c371d1fef36f64e126b0cccb237bf04cf518` |
| commodore-amiga-a600-ecs-ntsc-progressive-debug-cut.state | 10356923 | `a07c5b1d848deb584d37656415deafabb98646920d2754a7ab6ca15a2e1f9dd8` |
| commodore-amiga-a600-ecs-ntsc-progressive.final-state | 10356926 | `05e24b821f8875343b754dea916b31118cff5c174609e3c6c63b0657dd10f7dc` |
| commodore-amiga-a600-ecs-ntsc-progressive.output | 14556 | `2f88433ed64a2814ef28cd7287144c37ff785509a842088018ac08380fc5fb3c` |
| commodore-amiga-a600-ecs-ntsc-progressive.state | 10356911 | `a84ed7f6d3c28fcd80357c4b4d04e57aaccfb9b04be16bc678f4d611222584c7` |
| commodore-amiga-a600-ecs-pal-interlaced-debug-cut.final-state | 10356928 | `464966943b52af0983a2909e83c92a28f5e8ee7b8a0071e7407d9146909ab3b6` |
| commodore-amiga-a600-ecs-pal-interlaced-debug-cut.output | 17454 | `f6399747665a8ca1ad730599c4192045530e746148ec960a05252aa45d3666a8` |
| commodore-amiga-a600-ecs-pal-interlaced-debug-cut.state | 10356923 | `601b6b5d323cf779ea5aae526f60a87eb2d5a290581695ac6c445abe4b3b4038` |
| commodore-amiga-a600-ecs-pal-interlaced.final-state | 10356928 | `464966943b52af0983a2909e83c92a28f5e8ee7b8a0071e7407d9146909ab3b6` |
| commodore-amiga-a600-ecs-pal-interlaced.output | 17454 | `400462c1dbdea124305613be73d741080f46278872b2354e249a79f4de17967f` |
| commodore-amiga-a600-ecs-pal-interlaced.state | 10356920 | `31e2c2e6b159ffc5700777e6b0723286844fb9a7cee217ff3b17d387e65cdf94` |
| commodore-amiga-a600-ecs-pal-progressive-debug-cut.final-state | 10356916 | `f19da9ab86d190ce719cf82a0895d3f8a152f82cd52a3af39d5b5afb8a85b911` |
| commodore-amiga-a600-ecs-pal-progressive-debug-cut.output | 17418 | `c4c05410db5bb87d8af4c0c33f6f747e002ea3525953239d0fbc844ee18b8cae` |
| commodore-amiga-a600-ecs-pal-progressive-debug-cut.state | 10356920 | `3e437f7e4f16ae659205a94d372d2886be3d9ed6e620f8cf85b3801df2853cd9` |
| commodore-amiga-a600-ecs-pal-progressive.final-state | 10356916 | `f19da9ab86d190ce719cf82a0895d3f8a152f82cd52a3af39d5b5afb8a85b911` |
| commodore-amiga-a600-ecs-pal-progressive.output | 17418 | `ef40d7cd79c3f6f6ed7722fb66dfbde62f72e26eed705e3376bbe6e1322ded34` |
| commodore-amiga-a600-ecs-pal-progressive.state | 10356920 | `c4d0f69706b17ab7a5a31d3b9837cf0a4a9baaa5088a5c49a7d3e73cf1b5b5e8` |
| inactive.rom | 524288 | `43442ff508acb00ca6f5834930114b3030751e6066e8072179ce5063807577d8` |
