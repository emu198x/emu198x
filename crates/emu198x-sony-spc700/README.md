# emu198x-sony-spc700

The Sony SPC700, the S-SMP at the heart of the Super Nintendo's sound module,
as a cycle-accurate CPU with a pin-level bus.

Each `tick()` is one CPU cycle. The CPU drives `addr`, `data`, `rw`, `idle`
and `sync`; the host performs the access between ticks and puts read data on
`data_in`. There is no bus trait and no callback. The IPL ROM, the timers and
the `$F0`–`$FF` I/O registers belong to the host, not the CPU.

Checked cycle by cycle against [SingleStepTests/spc700](https://github.com/SingleStepTests/spc700).

The instruction bodies follow [ares](https://github.com/ares-emulator/ares)
(ISC licence, © ares team, Near et al), re-expressed in Rust.
