# Source captures

This directory contains one stereo WAVE per corpus case.

Each file contains three adjacent PAL fields at 48 kHz. Samples are
little-endian IEEE-754 binary32 values copied from vAmiga's modelled A500
output without sample conversion. The WAVE `fact` chunk records the stereo
frame count.

File identities and semantic measurements are retained in
[`../records/`](../records/README.md).

## Related files

- [Package overview](../README.md)
- [Capture records](../records/README.md)
- [Producer manifests](../manifests/README.md)
