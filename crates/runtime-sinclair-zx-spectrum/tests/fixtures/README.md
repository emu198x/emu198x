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

The regression must decode and re-encode this original byte string exactly,
then continue the write and read back all 512 independently expected bytes.
It detects positional-layout or enum-tag drift when Format Track is added.
It is not a newly generated round trip through the same implementation.
