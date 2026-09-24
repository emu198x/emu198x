# z80test fixture identity

The Spectrum exerciser uses Patrik Rak's MIT-licensed
[z80test 1.2a release](https://github.com/raxoft/z80test/releases/tag/v1.2a).
The TAPs remain external fixtures; this directory records their identity.

| Item | Pin |
|---|---|
| Archive | `z80test-1.2a.zip` |
| Archive SHA-256 | `7df0443d703e6b3114ea04b4cdef3e13b91421c62e37185c1036d06864cacbaf` |
| TAP hashes | [SHA256SUMS](SHA256SUMS) |
| MEMPTR TAP SHA-256 | `444582ddfa4d05711b6235e743ddf68295231e97ba92cc838c5d09a106c9a10f` |

Upstream [commit 2d136c1](https://github.com/raxoft/z80test/commit/2d136c197b58d13438fd5bba18117504b399c530)
corrected the MEMPTR CRCs for `INIR->NOP'` and `INDR->NOP'` from `0x0a537b63`
to `0xf3b1be2f`. This correction shipped in 1.2a; the test names and the
on-screen `2012 RAXOFT` banner do not distinguish the versions.

The private corpus's older MEMPTR TAP has SHA-256
`7bbe1bee2bde2c07c5930b91df4bb268d811db3b3fd160999fbb41686e9dbf8d`
and contains the superseded CRCs. On the same emulator and ROM it fails cases
102/103, while the 1.2a TAP passes. These failures must not be used to justify
changing CPU behaviour or expanding an allowlist.

Nightly retains the ROM from the private corpus, downloads the pinned upstream
archive, verifies its checksum, extracts it, and checks all seven TAP hashes
before running the exercisers with strict fixture handling. For local runs,
extract the same archive and set `EMU198X_Z80TEST_DIR` to its `z80test-1.2a/`
directory and `EMU198X_SPECTRUM_48K_ROM` to the local ROM.

An explicit `EMU198X_Z80TEST_DIR` is authoritative: a missing tape is reported
as missing rather than replaced by a default or legacy copy. In strict fixture
mode this fails the test. Default discovery runs only when the variable is unset.

From the extracted directory, verify the tapes with:

```sh
shasum -a 256 -c /path/to/emu198x/test-data/z80test/SHA256SUMS
```
