# Interchange Schemas

This directory defines the machine-readable boundary between the neutral
sprite probe and evidence producers.

`suite-v1.schema.json` validates the generated build manifest. It binds the
single unresolved question to the expanded case record, source and toolchain
hashes, payload, and ADF.

`capture-v1.schema.json` validates one producer's observation of one chipset
profile. It records exact artifact identities, implementation ancestry,
machine and firmware configuration, raw pixel provenance, beam-to-capture
mapping, and per-field edge measurements.

`hblank_stop_sample` is the first non-blank source sample after the leading
hardwired blank interval on the declared sample row. Marker and sprite
intervals use start-inclusive, stop-exclusive coordinates. Derived deltas are
signed source-sample differences calculated without alignment search:

- `sprite_start_minus_hblank_stop_samples`;
- `sprite_start_minus_marker_start_samples`.

When `captured_fields` names multiple fields, `source_capture.file_name` must
identify a multi-frame container whose frame order matches that array. For
RGBA8 captures, `decoded_pixel_sha256` covers tightly packed, row-major RGBA
bytes for all frames concatenated in container order. Other formats must state
the exact decoded byte sequence in `pixel_format`. The producer package must
retain those bytes under `decoded_pixel_file_name`; the semantic validator
rehashes them, while review of the producer adapter establishes the raw-to-
decoded transformation. The capture configuration named by
`execution.configuration_file_name` is retained and hashed by the same gate.

The schemas preserve observations. They do not decide which producer or
chipset profile is correct, and they do not turn software consensus into a
hardware result.

JSON Schema cannot express every cross-field relationship in this record.
After schema validation, `../tools/validate_capture.py` must validate the
candidate against its generated `suite-v1.json`. This second check verifies
artifact identity, adjacent fields, sample-row joins, interval bounds, derived
deltas, bound files, and producer/evidence classification consistency.

## Related files

- [`../cases/README.md`](../cases/README.md) defines the source question.
- [`../references/README.md`](../references/README.md) defines evidence
  provenance.
- [`../README.md`](../README.md) defines the capture procedure.
- [`../tools/README.md`](../tools/README.md) defines semantic validation.
