# Write-Timing Build Tools

`build.py` validates the source case records, invokes GNU `m68k-elf` Binutils,
builds every payload and boot block, creates each ADF, validates the resulting
disk image, and writes the versioned suite manifest.

`make_adf.py` contains the deterministic ADF packing and Amiga boot-block
checksum implementation. It can also package a boot block and payload from the
command line.

Both tools use only the Python standard library. They do not download tools,
firmware, media, expected captures, or emulator code.

`inspect_fs_uae_capture.py` is a diagnostic reader for registered raw FS-UAE
capture directories. It verifies adjacent-field identity and prints the
guard, marker, and black runs around the mutation output. It is not used to
generate expected output.

## Related files

- [`../src/README.md`](../src/README.md) describes the assembled sources.
- [`../dist/README.md`](../dist/README.md) describes generated output.
