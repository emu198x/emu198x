# Game Boy browser binding

Canvas-free binding around `WebMachine<GameBoyRuntime>`. The shared browser
player currently exposes DMG skipped boot with user-selected cartridges. The
runtime owns cartridge validation, emulation and input; the host owns pacing,
RGBA conversion and audio buffering. No firmware or cartridge is bundled.

See [the shared player](../../web-player/README.md) for builds and validation.
The native parity example accepts a cartridge path; the shared worker checker
compares its output with the actual generated WASM.
