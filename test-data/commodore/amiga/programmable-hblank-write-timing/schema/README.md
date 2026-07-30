# Write-Timing Interchange Schemas

This directory defines the machine-readable boundary between the probe corpus
and capture producers.

`suite-v1.schema.json` validates the generated build manifest. It binds case
questions to exact source, toolchain, payload, and ADF hashes.

`capture-v1.schema.json` validates one producer's observation of one case. It
records the reset, mutation, and following output row pairs; black, guard, and
marker intervals; the tested write; and the evidence available for its
position. It also requires enough provenance to distinguish hardware from
software, identify shared implementation ancestry, reproduce the machine
configuration, and audit all pixel transformations.

When `captured_fields` names more than one field, `source_capture.file_name`
must identify a multi-frame container whose frame order matches that array.
For RGBA8 captures, `decoded_pixel_sha256` is the SHA-256 of the tightly
packed, row-major RGBA bytes for every decoded frame concatenated in container
order. A producer using another pixel format must state the exact byte
sequence in `source_capture.pixel_format`.

Schemas describe evidence records. They do not decide which observation is
correct and do not assign expected pixels.

## Related files

- [`../cases/README.md`](../cases/README.md) defines the source questions.
- [`../references/README.md`](../references/README.md) defines the documentary
  evidence boundary.
