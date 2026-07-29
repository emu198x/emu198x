# Contributing

Contributions should preserve the corpus as an implementation-neutral
observation tool.

## Adding a case

Add one entry to [`cases/cases.json`](cases/cases.json). A case must ask one
question, change only the registers needed to ask it, retain the PAL baseline
bits in `BEAMCON0`, define both identities, and leave `expected.status` as
`unresolved`.

Run:

```sh
python3 tools/build.py
python3 tools/build.py --output /tmp/programmable-hblank-rebuild
```

The SHA-256 values for corresponding ADF and payload files must match across
both builds when the toolchain versions match. Review the generated
`suite-v1.json` as part of the change.

## Contributing a capture

Validate a capture record against
[`schema/capture-v1.schema.json`](schema/capture-v1.schema.json). A capture must
identify the exact ADF and payload hashes, the producer's implementation family
and revision, the machine and firmware, and every transformation between the
producer's raw output and the recorded pixels.

Do not classify software captures as hardware evidence. Do not combine related
emulators into independent implementation families. If the producer crops
blanking or applies scaling, retain the capture for diagnostics but classify it
as unsuitable for blank-edge evidence.

## Licensing

Contributions must be original or otherwise available for dedication under
CC0 1.0. Do not copy source, generated code, binaries, screenshots, or test
vectors from another emulator. Documentary references may identify third-party
works without redistributing them.

## Related files

- [`README.md`](README.md) defines the corpus scope.
- [`references/README.md`](references/README.md) explains the evidence boundary.
- [`schema/README.md`](schema/README.md) explains the interchange schemas.
