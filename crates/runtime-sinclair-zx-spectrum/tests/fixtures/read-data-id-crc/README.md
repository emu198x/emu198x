# Legacy v6 ID-error controller checkpoints

These eight small Postcard fixtures contain only the programmatic µPD765A controller and a 512-byte sector filled with E5. They contain no CPU, firmware, external media or native machine snapshot. Generated from controller/runtime source at 5cded0a6b646f493d7cc2abc8292eb26c3837f49 before the #1852 fix, using the temporary test fragment retained in docs/plans/2026-10-19-read-data-id-crc/reference/capture_legacy.rs. The capture command and raw output are retained with the study.

Names are opcode-acceptedPayloadBytes-RQMhigh.postcard. Opcodes 46/4C; accepted 0/179; low/high RQM. Matching data mark, ID-only CRC (DE set, DD clear; RDD includes the deleted-mark bit 40), CHRN 0/0/1/2. The new runtime wraps each unchanged controller fixture in a v6 envelope and restores it, checking ID-error result before any host status read; exact Postcard re-encoding first proves the layout is unchanged.

Fresh valid transfers and result prefixes must retain byte-identical native roundtrips. Old faulty active transfers intentionally normalize to an ID-error result; earlier payload consumption cannot be undone. No v6 schema/version changes.

SHA256:

- `46-0-0.postcard`: `b8c56383cd6ce05930cd66bf4d2c46cb8ce5c705160f5b00d89cf7c5932cd7ce`
- `46-0-1.postcard`: `29555a8d051c2b22c9bacb8c69e748debd2db4f8a6c8627438839eb042be4adb`
- `46-179-0.postcard`: `e457a2da7ff19519f877f28eb3441d11c073bca0e80d5c2f5de004ad700d3d30`
- `46-179-1.postcard`: `ecbb2d6d92adde6af9ad59a3819dae47e27b4f4897849ad05c0bc23bfccb5cee`
- `4c-0-0.postcard`: `42dc1721b37d064641870496cae66e815ee91ee79ec1906484cab83206a17e8e`
- `4c-0-1.postcard`: `163a7a96e6594b803c1e6419bd1a5c569eca26a91ba12ca36aea21ab1f9fbf32`
- `4c-179-0.postcard`: `579535e2a3dc9b509d74e97717ba54951a9eb9cb35b98e6213eed7f1f767399c`
- `4c-179-1.postcard`: `fa6929576c3d4c00b553b0c94264e92fa881fca1211cbc3c82d281ec6ae85482`
