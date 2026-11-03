# Pre-split native-output baseline

`pre_split_77440f443.json` records SHA-256 fingerprints captured from commit
`77440f4435ffd4700696a0e56f2050d71827a39b`, before the physical-owner migration.
All 192 original profile/mode/phase cases have three independent fingerprints:
the initial save, two complete native outputs, and the final save.

The output encoding is postcard over the ordered pair of tuples:
`(timestamp: u64, width: u32, height: u32, pixels: Vec<u8>,
palette: Option<Vec<u32>>, audio_sample_bits: Vec<u32>)`.
Audio samples retain every `f32::to_bits()` value; no perceptual threshold or
picture-only comparison hides changes.

The fixture-free test boots synthetic `DI; JP` firmware, installs patterned
display RAM, drives the video/audio ports, and checks eight partial-frame phases
for every original profile. Timex profiles also cover modes 0, 1, 2 and 6.
Both uninterrupted execution and restored execution must match from the first
resumed frame, and end with identical complete saves.

These captures preserve existing behaviour. They do not establish the physical
PCB behind an old generic profile, qualify analog timing, verify original ROMs,
or replace the separate Spectrum catalogue qualification. When a board identity
changes, retain these fingerprints and make its input mapping and save-version
expectations explicit. Do not refresh this file to make a regression pass.

The original 577 capture files and their individual fingerprints are retained in
the task's verified private recovery archive. They are synthetic inputs/outputs;
no manufacturer firmware is embedded in this manifest.

The fixed-owner gate routes these originals to 352 comparisons across 296 unique
current inputs. Old generic 16K/48K/Plus cases qualify the seven later 6C owners
with matching RAM. Old Issue 1/2 cases cover both RAM populations of those boards.
The other ten profiles keep their original routes and raw save fingerprints.

For the new physical owners, save comparison removes only the new independent
board tag and normalizes the approved envelope version/profile ID back to the
original. Every remaining machine byte is compared unchanged. The new 5C/16 KiB
profiles have no original matching RAM-population state, so their old state hash
is not asserted; native output and exact current save/resume are still checked.
The separate CPU sweeps and original-ROM BASIC checks cover fitted RAM directly.

Five complete original v17 specimens are retained here with their original
fingerprints for atomic rejection tests. They contain synthetic firmware only.
