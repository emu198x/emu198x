# Interchange Schemas

This directory defines the machine-readable boundary between the neutral OCS
Copper colour probe and evidence producers.

`suite-v1.schema.json` validates the generated build manifest. It binds the
single unresolved question to the expanded case record, source and toolchain
hashes, payload, and ADF.

`capture-v1.schema.json` validates one producer's observation. It records
exact artifact identities, implementation ancestry, OCS machine and firmware
configuration, raw pixel provenance, beam-to-capture mapping, the bitplane
marker interval, and four ordered colour-transition measurements.

Marker intervals use start-inclusive, stop-exclusive source coordinates. A
colour-transition sample is the first source sample rendered with the new
`COLOR00` word. Derived values are calculated without alignment search:

- each transition sample minus the marker start;
- each later transition sample minus the preceding transition sample.

When `captured_fields` names multiple fields, `source_capture.file_name` must
identify a multi-frame container whose frame order matches that array. For
RGBA8 captures, `decoded_pixel_sha256` covers tightly packed, row-major RGBA
bytes for all frames concatenated in container order. Other formats must state
their exact decoded sequence in `pixel_format`. The producer package retains
those bytes under `decoded_pixel_file_name`.

The schemas preserve observations. They do not decide which producer is
correct and do not turn software consensus into a hardware result.

JSON Schema cannot express every cross-field relationship in this record.
After schema validation, `../tools/validate_capture.py` must validate the
candidate against its generated `suite-v1.json`. This second check verifies
artifact identity, adjacent fields, sample-row joins, ordered colour words,
edge bounds, derived deltas, bound files, and evidence classification.

## Related files

- [`../cases/README.md`](../cases/README.md) defines the source question.
- [`../references/README.md`](../references/README.md) defines evidence
  provenance.
- [`../README.md`](../README.md) defines the capture procedure.
- [`../tools/README.md`](../tools/README.md) defines semantic validation.
