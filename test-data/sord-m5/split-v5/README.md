# Pre-split Sord M5 v5 compatibility fixtures

Captured from ee937d475890cf1013a16935f5d2502413be54f3 before changing the
machine ownership. All ROM bytes are an original synthetic guest; no commercial
firmware is included. `manifest.json` records complete file sizes and SHA-256.
The capture source is `crates/runtime-sord-m5/examples/board_baseline.rs`.

The guest configures text video, a tone and a running CTC timer, then reads the
keyboard/joystick into RAM. A whole-T-state partial-field state carries live
CPU/video/audio/input progress. Its runtime time remains the old accepted
value after direct machine stepping; this is a compatibility fixture, not a
new recommendation for host clock accounting. Three subsequent native requests
record actual times, final pixels, every audio sample and the complete state.

The compatibility test must pass before and after the split. Keep these captured
expectations fixed; a newly generated output belongs in a separate directory
for comparison. The capture revision is the provenance, not a hardware oracle.
Physical timing/legacy PAL PSG qualification remains tracked in #252.
