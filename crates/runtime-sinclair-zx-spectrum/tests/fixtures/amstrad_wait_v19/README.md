# Original Amstrad v19 rejection specimens

These complete saves were generated before the physical WAIT implementation,
using source f176914a335b45932d936f36f454d681b3520c08 (the task branch adds only
the diagnostic plan). Registry Z80 0.8.0 and 40077 0.2.0, original Cargo.lock.
No proprietary firmware: all-RAM mode 2 maps [4,5,6,3], bank 3 at FF00 contains
NOP / JP FF00, I=0, and each machine runs 101337 master ticks before capture.
The independent original capture helper and logs are retained in private task
evidence. These bytes are immutable and must reject before v20 payload decode.

SHA-256:

- `plus2a.state`: `9731e180ff82b8ab26c49f82085ad29f80b421ff99651d7804a34816b659c28a`
- `plus2b.state`: `0d83f0ccaf6d5eb1d2676174dd81c6c86119f0701582da142e77b0895d89accb`
- `plus3.state`: `47f266a1aeff0a5173161308974f0b182289471b4d97f892b3525afe3042d1af`
