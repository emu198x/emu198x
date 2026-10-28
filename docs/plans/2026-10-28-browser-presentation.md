# Browser presentation and shared pixel aspect

Lane: engineering-frontier host reliability. Base: 98b5bcabf30e2db87df94362bbacd343f312354e.

Issue #1906 already selects sharp bilinear at the machine's runtime-derived
pixel aspect. Preserve the existing controls, theme, raw frames and captures.
Use the existing 2-D presenter path first, as the issue's approved CSS option;
no new dependencies, GPU backend or snapshot schema is planned. If measured
output cannot meet the requirement through that path, stop for an explicit
backend decision instead of silently adding one.

1. Reproduce fractional pixel-width variation with a browser-backed stripe
   fixture. Compare the existing nearest presentation with integer 2-D
   upscaling followed by smooth aspect-correct CSS presentation.
2. Add one generic runtime-derived pixel_aspect_ratio getter in
   crates/emu198x-web/src/lib.rs and expose it from the Spectrum, Commodore,
   NES and Game Boy bindings. Carry it in web-player/src/worker.js frame
   results; verify against each runtime's existing display contract.
3. Add a private 2-D presentation helper under web-player/src/ and wire
   player.js/player-extras.js/player.css to it. Keep Sharp/Smooth preference
   values. Handle paused resize, device scale and disposal. Give the shared
   Amiga mouse helper the native framebuffer dimensions so backing-store
   upscaling cannot change pointer coordinates. Preserve its existing callers.
4. Test actual rendered stripes and aspect at fractional scales, DPR 1/2,
   downscale, resize and fullscreen geometry. Check input mapping and raw-frame
   invariants, plus the existing inline/worker and Rust host tests. Inspect
   the rendered result, then run the Impeccable mechanical detector once.
5. Push only verified changes, require completed successful CI, verify issue
   closure and clean the task worktree, test browser profiles/server, fixtures
   and build caches.

## Measured direction and approval

The Chrome probe reproduces a one-pixel stroke-width range at 2.98x nearest
scaling. Integer staging plus CSS smoothing reduces the effective-width range
to 0.0667 pixels but does not eliminate it. The user approved a dependency-free
WebGL sharp presenter on 2026-10-28, with the existing 2-D path as compatibility
fallback. Use the existing native shader's sampling maths, not a new filter.

Implement the GPU presenter on an internal canvas and copy its completed
picture to the existing visible 2-D canvas. This preserves DOM/input bindings
and allows a real 2-D fallback when context creation or restoration fails.
Frame data and its capture APIs remain native-sized and unchanged. Verify
actual browser rendering, context-loss/fallback, resize and disposal; no
blanket browser or performance qualification follows from a single capture.

## Verification

- Native generic host: 21 unit tests and 4 parity tests passed. The 11 real-ROM
  tests remain ignored by default; no capture/golden hash changed.
- Chrome WASM parity: all 4 parity tests passed, including runtime display
  metadata agreement and unchanged native RGBA geometry/bytes.
- Strict Clippy passed for all six changed host/binding crates; formatting and
  actionlint passed.
- The real Chrome presenter regression passed at DPR 1 and 2. Fractional
  isolated-stroke effective-width range was zero to floating-point precision;
  readback matched the independent pixel-area oracle within two channel levels.
  Fitted display bounds matched the runtime aspect within one device pixel.
  Sharp/Smooth, downscale, successive-frame upload, native resize, paused resize,
  context loss/restoration, unavailable-WebGL fallback, native mouse coordinates,
  nonzero-offset source views and disposal passed. The deliberate source
  mutation was rejected, proving the guard can fail.
- Full browser distribution built. Native/WASM frame/audio/input parity passed
  for 84 browser variants; 37 explicitly require additional local firmware.
- Existing inline host regression and eight-demo Chrome extras regression passed.
  Actual mounted-player desktop/mobile/fullscreen renders were inspected at
  DPR 2, with paused resizing and no page errors. Impeccable detector returned
  no findings. This is Chrome qualification, not an all-browser performance claim.

The distribution run exposed two pre-existing test drift failures: a rejected
CGB expectation after that model became supported, and native-only families
selected by the browser parity runner. The rejection now uses an unknown model;
parity selects the explicit browser family manifest and rejects a missing family,
missing model or empty fixture set. No browser support surface was expanded.
