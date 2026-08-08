# Contributing

Contributions should preserve this corpus as an implementation-neutral
observation tool.

## Changing the case

The source case asks one horizontal-phase question. Changes to its register
program, marker geometry, ready record, or capture procedure require a suite
version and `source_revision` change. Leave `expected.status` as `unresolved`
and `expected.observations` empty.

Run two builds to check deterministic output:

```sh
python3 tools/build.py
python3 tools/build.py --output /tmp/sprite-horizontal-phase-rebuild
python3 -m unittest tools/test_validate_capture.py
```

Corresponding ADF and payload hashes must match when the toolchain versions
match. Review the generated `suite-v1.json` as part of the change.

## Contributing a capture

Validate each record against
[`schema/capture-v1.schema.json`](schema/capture-v1.schema.json), then run
`tools/validate_capture.py` without `--skip-file-checks`. The schema validates
shape; the semantic validator binds the generated artifact and checks the
relationships between fields, rows, intervals, deltas, files, and evidence
classification. Both checks are required before a capture can be registered.
The producer adapter or procedure must also be rerun to establish that the
retained decoded-pixel file is the stated decoding of the retained raw capture;
the neutral validator can hash both files but cannot infer arbitrary producer
formats.
One record describes one producer, machine profile, and case. OCS, ECS, and
AGA results therefore remain separate records even when one producer captures
all three.

Do not classify software captures as hardware evidence or count related
emulators as independent implementation families. Retain raw blanking and
overscan, disclose every transformation, and do not search for an alignment
that minimises disagreement.

## Licensing

Contributions must be original or available for dedication under CC0 1.0. Do
not copy third-party source, binaries, firmware, or user-interface imagery
into this corpus. Captures may contain only output from the corpus-authored
probe and must be distributable under CC0.

Producer-specific adapters and patches belong with the applicable producer
or capture package, not in the neutral source and build directories.

## Related files

- [`README.md`](README.md) defines the corpus scope and capture contract.
- [`references/README.md`](references/README.md) explains evidence provenance.
- [`schema/README.md`](schema/README.md) explains the interchange records.
