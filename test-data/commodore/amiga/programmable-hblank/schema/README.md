# Interchange Schemas

This directory defines the machine-readable boundary between the probe corpus
and capture producers.

`suite-v1.schema.json` validates the generated build manifest. It binds case
questions to exact source, toolchain, payload, and ADF hashes.

`capture-v1.schema.json` validates one producer's observation of one case. It
requires enough provenance to distinguish hardware from software, identify
shared implementation ancestry, reproduce the machine configuration, and
audit all pixel transformations.

Schemas describe evidence records. They do not decide which observation is
correct and do not assign expected pixels.

## Related files

- [`../cases/README.md`](../cases/README.md) defines the source questions.
- [`../references/README.md`](../references/README.md) defines the documentary
  evidence boundary.
