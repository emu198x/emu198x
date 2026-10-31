# Original Atari ST v1 states

Captured from `c6cb0da7173db46d6ba3557ae38b8b82e3543f42` before extraction,
using the project-written synthetic TOS in
`crates/runtime-atari-st/tests/common/mod.rs`. No commercial firmware is
included. TOS stays external to the existing v1 save format.

Each of four model/clock configurations has colour and monochrome captures.
States include a partial native field, active raster and YM2149 sound, held
keyboard/mouse/joystick input, and two mounted discs with different write
protection. Each `.output` encodes three records of elapsed CPU clocks, full
`CapturedFrame` and audio sample bits, followed by the final complete runtime
snapshot. The runtime's board baseline tests compare direct execution,
re-encoding and the first restored fields against these original bytes.

Capture mode refuses to overwrite differing data. A deliberately corrupted
continuation must fail the same comparator. Monitor replacement and reset
also check disc contents, write protection and the supplied TOS.
