# Blanking residuals closed

The selector's extra half-CCK delay is removed without changing snapshot 54.
Both portable consumers now sample the guest label after its VERTB
acknowledgement, while preserving the previously completed image and checking
actual video-field progression. The static consumer separates comparator
agreement from counter-qualified UAE phase.

`validation.json` binds the source, native executable and validation logs.
`timed/` retains all ten successful case reports. `static-references.json`
contains fourteen observations derived solely from the two unchanged
reference packages. The static log records all fourteen native observations
and the hashes of their three identical fields. Each static and timed case
requires adjacent guest labels and exactly one video-field advance.

The baseline trace records guest publication on either side of h=22, proving
why the old sampling could report 9 then 11 for adjacent images. The selector
unit test fails before correction; the reference-admission test rejects a
changed capture, and the measurement test detects a shifted edge.

All ten timed cases and fourteen static cases pass. All 205 affected chip
unit tests, 55 existing snapshot tests plus the new selector replay test,
44 query tests, five measurement tests and two reference-admission tests pass.
Clippy is clean and both six-case Test Kit lanes remain exact. The native
emulator is rebuilt. No reference pixel, package or snapshot schema changed.

The five reference-family disagreements remain explicit classifications.
The earlier palette-XOR, top-of-field and OCS far-edge investigations are
outside these corrected blanking failures.
