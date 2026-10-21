# Probe recovered Arnold all-skipped behaviour

Lane: Spectrum launch-hardening. Owners: Emu #1949, umbrella #231.

1. Read original archive members, prior evidence and binding decisions. Preserve
   unrelated work; do not edit vendored references or the archived source.
2. Compile exact fdc/fdi/fdd source and public headers from the hash-pinned 2017
   archive. Replace only platform includes, time and the disk-image adapter.
   Reuse the prior authored EDSK recipe. Record all substitution boundaries.
3. Drive public FDC registers with bounded emulated/host limits. Repeat both read
   opcodes, SK on/off, matching/mismatching marks, single/two-sector tracks. Add
   skip-then-match/EOT2 and a CRC-metadata contrast. Retain raw observations,
   exact source/recipe/compiler/executable identities, logs and unchanged media.
4. Audit raw reads, commands, input/live artifacts, result framing and recovery.
   Prove the audit rejects fabricated data/status/missing cases. Report failed
   model controls explicitly; no universal hardware inference from a model run.
5. Keep native expectations unchanged unless the required evidence is met.
   Push a reviewable research PR, update issues and clean task workspaces after
   verifying exact remote retention and a working offline evidence export.
