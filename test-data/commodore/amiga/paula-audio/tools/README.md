# Build Tools

`build.py` validates the case source, assembles one 68000 payload per case,
packs each payload into a deterministic 880 KiB ADF, and writes a manifest
containing all source and artifact hashes.

The supported invocation is:

```sh
python3 tools/build.py
```

Requirements:

- Python 3.10 or later;
- `m68k-elf-as`; and
- `m68k-elf-ld`.

The build sets a fixed locale and source date. Running it twice from unchanged
sources must produce byte-identical ADFs, payloads, and manifest content.

`make_adf.py` may also pack one already-assembled boot block and payload. It
validates the DOS signature, boot checksum, payload placement, sector padding,
and final image size.

## Related files

- [`../src/README.md`](../src/README.md) describes the assembled program.
- [`../dist/README.md`](../dist/README.md) describes generated output.
- [`../schema/suite-v1.schema.json`](../schema/suite-v1.schema.json) defines
  the generated manifest.
