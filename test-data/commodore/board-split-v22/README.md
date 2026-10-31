# Original C64/C128 version-22 continuations

Project-written synthetic KERNAL/BIOS, BASIC and character ROMs run on the
pre-extraction engine. The guest initializes active VIC video and SID audio;
keyboard contacts and C64 joystick fire remain held. Each case saves at a
137-cycle partial field after three completed fields. Complete `.state` files
and `.output` files retain the next three native frames, sample bits, time deltas
and final runtime state. The test also compares direct/restored reset states.

The six files pairs cover breadbin and C64C PAL/NTSC and C128 PAL/NTSC. They
contain no third-party firmware. `EMU198X_COMMODORE_CAPTURE=1` creates missing
fixtures only and refuses to replace differing existing ones. Normal runs always
compare against the committed original bytes. Deliberately changing an output
byte fails the direct-continuation comparison.
