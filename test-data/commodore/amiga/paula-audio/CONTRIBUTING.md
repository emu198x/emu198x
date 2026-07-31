# Contributing Capture Evidence

Build the corpus from an unchanged source revision, retain the generated
manifest, and capture one case at a time.

Do not edit case expectations to match a producer. Submit the raw recording,
capture record, exact execution configuration, and any analysis tooling or
commands needed to reproduce the observations. Register a new producer
directory when the product revision, implementation family, machine
configuration, or capture domain changes.

New cases must change one controlled variable where practical, ask one
explicit question, publish the same ready-record contract, and remain useful
on both emulators and physical hardware.

Commercial firmware must not be committed.

## Related files

- [`README.md`](README.md) defines corpus scope.
- [`cases/README.md`](cases/README.md) defines case metadata.
- [`references/README.md`](references/README.md) defines admissible evidence.
