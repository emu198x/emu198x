# Family-selected browser runtime

A browser host over the existing `FamilyRuntime` and `WebMachine` contracts.
Each Cargo feature links one runtime family; the public player builds each with
`--no-default-features --features <family>` and loads only the chosen artifact.
The small default feature is Game Boy; native catalogue/probe tools enable
`all-families`. No firmware is embedded.

`Fleet` accepts named firmware images before `boot()`, then exposes frame/audio
stepping, named keys/controllers, media slots and tape transport. The runtime
validates firmware and media. Browser-specific exceptions remain explicit:
BBC sideways BASIC installation, C64 PRG import, and Spectrum snapshot parsing.

`fleet-catalogue` emits runtime-owned models, firmware and slots for the shared
player builder. `fleet-probe` reads a private fixture manifest and produces
native frame/audio checkpoints; `web-player/check-fleet.mjs` compares the same
inputs through the shipped WASM worker. Neither tool copies fixture contents
or local paths into the public distribution.

Because this crate links every family, it also hosts the fleet capability
conformance test (`tests/capability_conformance.rs`). The test builds each
variant from synthetic firmware and fails when a declared capability
disagrees with what the runtime does. A dev-dependency on the crate itself
turns on `all-families` for tests, so a plain `cargo test --workspace` runs
the test over the whole fleet.

See [the shared player](../../web-player/README.md) for builds, validation,
site integration and model limits.
