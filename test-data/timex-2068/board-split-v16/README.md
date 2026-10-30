# Original Timex 2068 board-split evidence

Captured from source `f4cfecff2f4a43471ed82d542428d8d6469bfff1` before moving the engine, using the committed `timex_board_baseline.rs` recipe. Project-written firmware only; no manufacturer ROMs. Version-16 complete envelopes, mode 0 and mode 6, active AY, DOCK mask, HOME RAM, held A and 137 requested partial ticks. Output contains three complete frame packets (pixels, palette, dimensions, timestamps), every audio sample bit and the final complete save. This proves preservation of the original implementation, not hardware accuracy.

SHA-256:

`a9cb2c26dd1ce9215f9f5164a5c2d5822500ed4ef6bb6ebf92c83e44685d76ab`  `timex-tc2068-mode0.output`
`5e917698bb79a0b44f019bb32a123f8694ccb3072b3bc6369c51118aaff5cc4a`  `timex-tc2068-mode0.state`
`bde8d0b906b64f29f26f70136bd1b791f40af01554b17fa19ca06be5effddaac`  `timex-tc2068-mode6.output`
`9c96f56a2fe881e414501e35e3f70a92da485e8bb60181c575269fcc95475cc3`  `timex-tc2068-mode6.state`
`cf26be9e41eaac985ad85903fbf0f7690f86120affc65d19a1c1b1dd3740aea4`  `timex-ts2068-mode0.output`
`1b62f583e831e92443e9112839e4d3df228186a74da1f819e4b4a7531872e1c2`  `timex-ts2068-mode0.state`
`01c4f939b080c6191745b6041af61060206d0e62b2ab6d9f8524ada41a79bd26`  `timex-ts2068-mode6.output`
`186c846aaba5c2bf5bf9269943f82bc7ac94ba250ed206a4dcbd97371bec68ce`  `timex-ts2068-mode6.state`

The restore-continuation assertion currently fails on the unmodified engine: eight pixels in the first resumed frame differ in each tested board/mode combination. See [#2209](https://github.com/emu198x/emu198x/issues/2209). These captures are pre-split evidence, not a completed split or a passing restore-fidelity claim. The reproduction uses a 16 MiB test-thread stack, matching the large-stack approach already used by Spectrum runtime tests.
