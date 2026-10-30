# Original Aquarius engine fixtures

Captured from source main `24feaaa44e693902d53d0673e0469b01a9f00852`, before extraction.
The recipe is `common-mattel-aquarius/tests/board_baseline.rs` (originally in
`machine-mattel-aquarius`) and `runtime-mattel-aquarius/tests/board_identity.rs`.
Both use project-written synthetic firmware; no manufacturer ROM is included.

The two core specimens include 16 KiB expansion, an 8 KiB cartridge, held
keyboard/controller input, active speaker toggles and a partial native field.
Each `.output` contains three complete pixel arrays, exact timestamps/frame
counts, every audio sample bit and the final full serialized state. The runtime
fixture is the original NTSC version-4 envelope with BIOS, character ROM,
cartridge, RAM and held input.

Capture with `EMU198X_AQUARIUS_CAPTURE` pointing at a fresh directory. Existing
capture files are never overwritten. A corrupted PAL output byte fails the
comparison; the restored source fixture passes.

## SHA-256

- `ntsc-runtime.state`: `33b77d5076308c748968eef4927b531d68e23acac4258ccdbc2a8e77f8473509`
- `ntsc.output`: `8bdefbb5c475b0f8b292edd45cfe16408825718866abe4ab2f41958d86cbdd09`
- `ntsc.state`: `37ba98e1cb2f6cdf16dd36038dcf016647e8eafaa79d71530845cbfeb7017bc7`
- `pal.output`: `7ac0170913070daa761b5f3c86b342c952ed365b9ace669422ca947e40dec9bc`
- `pal.state`: `131f902c89a80ba9b99d20e8a9835d9cf6b2db426db38ef341cde91bddfe6fce`
