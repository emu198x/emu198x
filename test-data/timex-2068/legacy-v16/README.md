# Legacy Timex v16 rejection specimens

Original complete saves captured at `f4cfecff2f4a43471ed82d542428d8d6469bfff1` with project-written firmware before the ULA state fix. No manufacturer ROMs. Both boards, modes 0 and 6, held A, live AY, DOCK selection and HOME RAM. These omit causal video latches; v17 must reject them before decoding. The capture recipe and original outputs remain on `refactor/timex-2068-board-owners` at `2f682e7b7dfe6d1b85d4979efc7a58c25888b5da`.

SHA-256:

`5e917698bb79a0b44f019bb32a123f8694ccb3072b3bc6369c51118aaff5cc4a`  `timex-tc2068-mode0.state`
`9c96f56a2fe881e414501e35e3f70a92da485e8bb60181c575269fcc95475cc3`  `timex-tc2068-mode6.state`
`1b62f583e831e92443e9112839e4d3df228186a74da1f819e4b4a7531872e1c2`  `timex-ts2068-mode0.state`
`186c846aaba5c2bf5bf9269943f82bc7ac94ba250ed206a4dcbd97371bec68ce`  `timex-ts2068-mode6.state`
