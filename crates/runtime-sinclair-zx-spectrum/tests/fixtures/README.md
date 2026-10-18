# Synthetic released-controller fixture

`upd765-v0330-write.postcard` is a 1,098-byte postcard of only the live
`Upd765a` controller from Emu 0.33.0 commit
`69f80d9393c9847b34d60c4db57c2232d9aa2dee`. It contains no ROM, CPU or RAM
state. All disk bytes are synthetic. It is covered by this repository's
GPL-2.0-or-later licence.

SHA-256: `774e9946c7a2657c1745cabc12a49b1c56e637fdf91beef540f0326e909493e1`.

Generation uses the existing snapshot test helper `writable_sector_runtime(true)`
from that revision, sends Write Data `[45,00,00,00,01,02,01,2a,ff]` (hex), and
transfers the first 180 bytes of a 512-byte payload where byte i is
`(i + 71) mod 256` through `send_fdc_bytes`. Serialize `runtime.machine().fdc`
with `postcard::to_allocvec`. The next byte request is ready.

Until snapshot v5, the regression decoded/re-encoded the original string and
continued its write, detecting positional-layout/enum-tag drift. Spectrum v6
adds per-sector read progress; rejection of older states was explicitly approved
on 2026-10-18. The fixture stays unchanged as historical evidence. Its current
test prefixes these controller-only bytes with old version 5 and checks that the
version guard rejects before interpreting the deliberately incomplete envelope,
without changing the live runtime or source disk. It does not claim a full
released-envelope migration test. Current write/save/restore continuation remains
covered by the active snapshot tests.
