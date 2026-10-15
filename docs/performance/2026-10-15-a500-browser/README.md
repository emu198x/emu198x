# A500 browser frame-budget investigation

The retained change removes wide integer division from ordinary CPU clock ticks. It improves this workload but **does not resolve #1775** or qualify a playable browser release. No chip scheduling, processor edges, snapshot fields, dependencies or browser build flags change.

Source baseline: `1573f8bf3c0984d3c73729efe1e4d00edd03a42d` (0.34.0). Host: M2 Max / Mac14,6, 32 GiB, macOS 27.0, Chrome 154, Node 24.9.0. Builds use Rust 1.99.0, the locked dependencies, `cargo build --offline --locked --release --target wasm32-unknown-unknown -p emu198x-commodore-web`, and the existing wasm-bindgen 0.2.127 with `--target web`. The JSON records identify each WASM/input by SHA-256. Browser runs use the unchanged `web-player/src/worker.js`, one step RPC at a time, 48 kHz stereo and complete 768×576 RGBA transfers. Synthetic firmware is the repository fixture `test-data/synthetic-firmware/commodore-amiga-kickstart.rom`; Kickstart 1.3 stays local.

## Correcting the original timing evidence

The original joined audit used a browser automation debugger binding. On this same build/harness, attached-browser synthetic samples took 9.631 / 10.728 / 10.912 seconds per 120 frames. An ordinary tab controlled through native accessibility, without binding a browser debugger to that tab, took 3.063 / 3.011 / 2.871 seconds. Its Kickstart 1.3 control took 3.071 / 3.175 / 3.312 seconds after 300 startup frames.

The large original numbers are instrumentation-affected and must not be quoted as normal-browser throughput. V8 documents [WASM tier-down during debugging](https://v8.dev/docs/wasm-compilation-pipeline#debugging). That explains a plausible mechanism; the controlled ordinary-tab observation establishes the practical measurement difference without inspecting private browser internals. Other sessions were active on this desktop. These are local workload measurements, not isolated minimum-spec or hosted-release qualification.

## Retained improvement

The profile has 7,738 V8 samples (`profile-summary.json`). CPU-domain advancement and Denise work dominate; frame copies and audio draining are small. Software 128-bit division is also sampled. `CpuClock::edges_for_tick` previously delegated every tick, including the stock 1:1 rate, to generic `u128` chunk arithmetic. Reduced integral rates have zero phase and can return their numerator directly. Other single ticks use exact `u64` addition/division, with the unchanged wide path on overflow. Chunk arithmetic and serialized representation stay unchanged.

| Environment/workload | Baseline, three 120-frame step totals | Clock change |
| --- | --- | --- |
| Node, synthetic (step only) | 2.967 / 2.942 / 2.951 s | 2.783 / 2.772 / 2.850 s |
| Ordinary Chrome tab, synthetic (identified worker builds) | 2.848 / 2.985 / 2.909 s | 2.783 / 2.691 / 2.657 s |
| Ordinary Chrome tab, Kickstart 1.3 (identified worker builds) | 3.141 / 3.319 / 3.164 s | 3.038 / 3.096 / 5.723 s |

The browser table uses `browser-identified-comparison.json`: each build has a distinct worker/module URL prefix, and its recorded SHA-256 must match the corresponding Node module identity. The earlier `browser-clock.json` reused module URLs and received HTTP 304 responses; it is exploratory, cache-ambiguous evidence and **must not be used to quantify the change**.

The 120-frame budget is 2.396 seconds. Both browser workloads still exceed it. The synthetic identified-build samples and Node samples show a bounded improvement. The Kickstart samples have substantial variation, including the retained 5.723-second outlier; no precise manufacturer-firmware speedup is established. Node final frame SHA-256 and each 230,030-sample measurement agree before/after; synthetic audio is silent and does not establish audible Paula correctness.

Full LTO, SIMD, forced inlining of the large pixel routine, and an inactive-sprite shortcut gave no useful additional gain in these probes and were discarded. The retained patch is only the clock calculation. A cached wasm-opt 117 experiment was also discarded; its run overlapped compilation and establishes no performance conclusion. No new optimizer dependency or feature requirement was added.

## Verification and reproduction

- 484 tests passed across `common-commodore-amiga`, `runtime-commodore-amiga` and `emu198x-commodore-web` in release mode; 31 explicitly ignored. Strict Clippy for these packages/all targets and formatting pass.
- The single-tick invariant compares emitted edges **and retained state** with generic chunk arithmetic across integral/fractional, stopped and near-`u64::MAX` rates, including overflowing intermediate additions. Existing partial-domain, stepping, restore and hardware/boot regressions also run.
- Existing native parity example plus `parity.mjs` passes at frames 1/100/300/310/360 on two fresh WASM boots, with scheduled keyboard, joystick and mouse input, identical frame/sample counts and audio energy, plus invalid firmware/media/clock controls. `profile.mjs` independently verifies save/restore continuation. Synthetic parity remains a bounded fixture result.

Build/generated module directories are supplied to `profile.mjs` and `parity.mjs`; scripts do not download tools or firmware. Run the native `emu198x-commodore-web` `parity` example with the same synthetic firmware and give its JSON to `parity.mjs`.

For browser reproduction, serve `browser-benchmark.html` on loopback beside `synthetic.rom` and your own local `kick13.rom`. Create separate `before-player/` and `clock-player/` directories, each containing the unchanged `worker.js` and that build's `modules/commodore/{emu198x_commodore_web.js,emu198x_commodore_web_bg.wasm}`. The harness hashes the module and firmware bytes and never reuses the other build's URL. The benchmark posts JSON to `./result`; a local receiver can retain it. Do not attach DevTools or a browser automation debugger to the measured tab. Start it through the ordinary page button. Its times exclude canvas presentation and audible output. No firmware belongs in committed evidence.

Remaining work is a dedicated graphics/CPU-domain profiling campaign with real software and browser presentation/audio checks; #1775 stays open.
