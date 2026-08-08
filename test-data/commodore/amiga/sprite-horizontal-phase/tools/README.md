# Build Tools

`build.py` validates the unresolved case record, generates its assembly
include, invokes GNU `m68k-elf` Binutils, builds the payload and boot block,
creates the ADF, validates its fixed layout, and writes the versioned suite
manifest.

`make_adf.py` implements deterministic ADF packing and the Amiga boot-block
checksum. It can also package a separately assembled boot block and payload
from the command line.

`validate_capture.py` is the semantic companion to the capture JSON schema.
After schema validation, run it against the generated suite manifest and a
candidate capture:

```sh
python3 tools/validate_capture.py \
  --suite dist/suite-v1.json \
  references/example/capture.json
```

It binds the exact ADF and payload, verifies the named files, requires ordered
adjacent fields, joins sample rows to observations, checks interval bounds and
derived deltas, and prevents producer type from being promoted to a stronger
evidence classification. `--skip-file-checks` is for record-development only;
it is not sufficient when admitting a capture package.

Run the semantic-validator regressions from the corpus root with:

```sh
python3 -m unittest tools/test_validate_capture.py
```

All tools use only the Python standard library. They do not download tools,
firmware, expected captures, emulator binaries, or reference media.

## Related files

- [`../src/README.md`](../src/README.md) describes the assembled program.
- [`../cases/README.md`](../cases/README.md) describes the validated inputs.
- [`../dist/README.md`](../dist/README.md) describes generated output.
