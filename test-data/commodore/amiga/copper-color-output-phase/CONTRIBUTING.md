# Contributing

Contributions should preserve this corpus as an implementation-neutral
observation tool.

## Changing the case

The source case asks one OCS colour-output-phase question. Changes to its
register program, bitplane marker, Copper schedule, ready record, or capture
procedure require a suite version and `source_revision` change. Leave
`expected.status` as `unresolved` and `expected.observations` empty.

Run two builds and the corpus-local tests:

```sh
python3 tools/build.py
python3 tools/build.py --output /tmp/copper-color-output-phase-rebuild
python3 -m unittest discover -s tools -p 'test_*.py'
```

Corresponding ADF and payload hashes must match when toolchain versions match.
Review the generated `suite-v1.json` as part of the change.

## Contributing a capture

Validate each record against `schema/capture-v1.schema.json`, then run
`tools/validate_capture.py` without `--skip-file-checks`. One record describes
one producer, one machine profile, and one case.

Do not classify software captures as hardware evidence or count FS-UAE and
WinUAE as separate implementation families. Retain raw blanking and overscan,
disclose every transformation, and do not shift a capture to minimise its
disagreement with another producer.

## Licensing

Contributions must be original or available for dedication under CC0 1.0. Do
not copy third-party source, binaries, firmware, manuals, or user-interface
imagery into this corpus. Captures may contain only output from the
corpus-authored probe and must be distributable under CC0.

Producer-specific adapters and patches belong with the applicable producer
or capture package, not in the neutral source and build directories.

## Related files

- [`README.md`](README.md) defines the corpus scope and capture contract.
- [`references/README.md`](references/README.md) explains evidence provenance.
- [`schema/README.md`](schema/README.md) explains the interchange records.
