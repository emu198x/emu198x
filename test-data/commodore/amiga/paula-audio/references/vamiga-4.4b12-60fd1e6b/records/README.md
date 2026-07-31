# Evidence records

This directory contains one `capture-v1` record per corpus case.

Each record binds the source ADF and payload hashes to the vAmiga revision,
machine and firmware identity, complete configuration hash, source WAVE,
capture fields, analysis procedure, measured frequency, left and right AC RMS,
dominant output, and paired amplitude ratio where applicable.

The records preserve vAmiga's API output order. No left/right remapping was
used during capture or packaging.

## Related files

- [Package overview](../README.md)
- [Capture schema](../../../schema/capture-v1.schema.json)
- [Source captures](../captures/README.md)
- [Producer configurations](../configs/README.md)
