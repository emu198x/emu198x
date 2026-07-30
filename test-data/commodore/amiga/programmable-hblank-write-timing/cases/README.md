# Write-Timing Cases

This directory defines the five mid-line register-write questions in suite
1.0.0.

`midline-hbstrt-past` asks whether moving the start comparator behind the beam
manufactures a start event.

`midline-hbstop-future` asks whether moving the stop comparator ahead after
the original interval ended reasserts blanking without another start event.

`midline-ecsena-enable`, `midline-extblken-enable`, and
`midline-blanken-enable` ask what output becomes visible when one selector is
enabled after `HBSTRT` but before `HBSTOP`.

The case file contains no expected output. Its register words and schedule are
stimuli. Producer observations belong in schema-valid reference records.

## Related files

- [`../README.md`](../README.md) defines corpus scope.
- [`../src/README.md`](../src/README.md) describes the generated probe.
- [`../schema/suite-v1.schema.json`](../schema/suite-v1.schema.json) defines
  the generated manifest.
