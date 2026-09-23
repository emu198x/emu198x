# NES curriculum browser trial

Canvas-free NTSC binding around `WebMachine<NesRuntime>`. The existing runtime
owns CPU/PPU/APU execution and cartridge validation; the shared browser host owns
pacing, input delivery and RGBA conversion. The page runs this binding in a worker.
No emulator-core behaviour has changed.

Scope: Code198x Meet the Machine 09 and 10 and Dash unit 17, loading their own iNES cartridges,
controller-one buttons, frame stepping and wall-clock advancement. Audio delivery is
explicitly disabled for this trial (Dash is intentionally muted). This is not an npm release or a
claim that arbitrary cartridges, PAL, sound or mobile browsers are validated.
Reset means disposing of the old machine and loading the cartridge into a fresh
one; this also clears input and pacing state.

Build with Rust's `wasm32-unknown-unknown` target, wasm-pack and wasm-bindgen CLI
matching Cargo.lock (currently 0.2.127):

```sh
wasm-pack build crates/emu198x-nes-web --target web --release --out-dir pkg --no-opt
cargo clippy -p emu198x-nes-web --all-targets --no-deps -- -D warnings
cargo run --release -p emu198x-nes-web --example parity -- HEARTBEAT.nes READ-PAD.nes
node crates/emu198x-nes-web/scripts/check-parity.mjs HEARTBEAT.nes READ-PAD.nes
```

Use Code198x's current unit-09/heartbeat.nes and unit-10/read-pad.nes. The Node check
runs the actual generated WASM, compares native framebuffer hashes after 10, 20
and 30 frames with A released/held/released, repeats from a fresh machine and
checks malformed-cartridge rejection. Hashes pin these curriculum checkpoints;
intentional sample changes require rerunning the native example and review.

Code198x website `scripts/build-nes-trial.mjs` builds and copies the generated
JS/WASM into its ignored `public/wasm/nes-trial/` directory. No firmware or
third-party game is bundled. `--no-opt` skips the optional wasm-opt pass; Rust's
release optimisation remains enabled.

Dash comparison, using the current unit-17 ROM:

```sh
cargo build --release -p emu198x-nes-web --example dash_parity
node crates/emu198x-nes-web/scripts/check-dash.mjs DASH.nes target/release/examples/dash_parity
```

This exercises title/start and moving/jumping scenes at 11 checkpoints, repeats
from a fresh WASM machine, and rejects unknown button names. The existing two-ROM
comparison remains a separate regression check.
