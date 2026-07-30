# Contributing Write-Timing Cases

This document answers how the programmable-HBLANK write-timing corpus may be
extended without changing the meaning of existing evidence.

Add one case only when it asks one new timing question. Prefer a new case to
adding conditional behaviour to an existing case.

Each case must:

- restore its baseline before the mutation line in every field;
- use a Copper schedule rather than CPU instruction timing;
- place a distinct, non-black `COLOR00` marker immediately before the tested
  write;
- publish an exact ready record and printable case identity;
- retain `expected.status` as `unresolved` until evidence is classified;
- include a following-line control that proves the write took effect.

Changes to a case's schedule, register words, identity, or output geometry
require a suite version change. Existing reference records must remain bound
to their original manifest and artifact hashes.

A new capture package must identify its implementation family. Forks or
frontends sharing the same chipset core are one family, not independent
votes. Unsupported implementations are recorded as unsupported rather than
as behavioural observations.

Before committing a source change, build twice into clean directories and
compare every generated ADF, payload, and suite manifest.

## Related files

- [`README.md`](README.md) defines corpus scope.
- [`cases/README.md`](cases/README.md) defines current questions.
- [`schema/README.md`](schema/README.md) defines evidence records.
- [`references/README.md`](references/README.md) defines evidence status.
