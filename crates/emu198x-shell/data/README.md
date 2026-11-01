# Portable band-limited step kernel

`band-limited-kernel.f64le` contains 257 phase rows of 96 little-endian IEEE
754 binary64 corrections, in phase-major order (197,376 bytes). Its SHA-256 is
`1c5f2d3373b63b6644afe27da4a09a853683a70a7c74506a7bf1d722b0221e67`.

These are the existing Blackman-windowed sinc coefficients from
`audio/band_limited.rs` at source `cbcda19a116bdf3a86a8d41c186c046842c342b8`,
computed with Rust 1.99.1 on macOS ARM on 2026-11-01. Cutoff, support, integration,
normalisation and interpolation are unchanged. The table fixes the original
coefficient bits so Linux and Wasm do not select a slightly different filter
through their host sin/cos implementations. It contains project-generated
numerical data, with no third-party firmware or media.

The original formula is retained in `generate-band-limited-kernel.rs`.
Compile it with the pinned toolchain and run it with a **new** output path;
it refuses to overwrite a file. Reproduction on the original host must match
the SHA-256 above. Other host math libraries can produce different low bits:
that is the diagnosed defect, not a reason to replace this table. Normal
builds decode the committed table and never run the generator. A coefficient
change requires explicit review of save continuation and spectral evidence.

The shell tests check pulse area, DC settling, exact pending-tail restoration,
bounded dense transitions and no dropped pulses. A1200 frozen v66 tests compare
complete save bytes and first resumed video/audio/timing observations without
tolerances, including the final resampler history.
