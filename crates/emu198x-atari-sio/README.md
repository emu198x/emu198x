# emu198x-atari-sio

The Atari 8-bit SIO bus and the disk drives on it. SIO is two serial lines,
DATA OUT from the computer and DATA IN back, plus a command line the computer
drops to announce a command frame. Every device hears every byte, and the
device ID in the frame picks the one that answers.

The bus is modelled at byte granularity; the bit timing is POKEY's. The machine
hands whole bytes across: `send()` for what the computer put on DATA OUT, and
`poll_response()` for what a device has ready for DATA IN. Disks are numbered
sectors and a write-protect tab, and know nothing of the file they came from.

The command protocol follows the *Altirra Hardware Reference Manual* §9.1.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
