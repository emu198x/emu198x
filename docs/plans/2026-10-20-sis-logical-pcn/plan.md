# Keep logical controller PCNs separate from aliased physical drives

Lane: Spectrum launch-hardening. The +3 disconnects external US1, but the FDC
retains four logical PCN registers. Current code masks unit selection before
indexing both physical media and controller state. A Seek through logical unit 2
therefore overwrites unit 0's PCN and completion record.

NEC's A/B datasheet Seek section explicitly identifies four independent PCNs:
https://www.tiffe.de/Robotron/Bausteinuebersicht/FDC/necfdc.htm#Seek .
The Dec1978 Seek description defines stepping by NCN versus PCN, printed p15.
Fuse aliases drive pointers but retains separate `pcn[4]` and `seek[4]` arrays.
MAME retains separate `flopi` PCNs; its selected-drive alias READY polling is not
an independent oracle for this wiring.

1. Add public tests for separate PCNs/completions with mask1, requested unit/head
   echo, readiness failure preservation, recalibrate, and authored physical track
   8 reached by Seek0to5 then Seek2to3. Retain red before production edits.
2. Append saved logical PCNs; keep existing `track` as physical position. Index
   seek countdowns/pending records by unmasked logical unit. Apply signed seek
   delta to masked physical track, bounded to u8, and report logical PCN via SIS.
   Media, read/write sector lookup and ST3 track-zero remain physical.
3. Run all controller tests, Clippy and formatting, retaining exact output.
   Push a patch for parent integration, not a standalone schema-incompatible
   runtime release. Parent owns the approved Spectrum v6→v7 snapshot bump.

No new dependency or public API. The new saved field is part of the approved v7
change, and invalidates positional v6 controller payloads. READY and command
ordering are concurrent parent work and are not changed here.

## Verification

Four of five new tests fail before the fix; the identity-mask control passes.
Afterward all 147 controller tests pass, none ignored, with all-target Clippy
and workspace formatting clean. The physical-read check verifies all128 bytes
on authored track8, followed by outward steps to authored track6. The old test
that identified physical rather than logical busy bits is corrected explicitly.
Initial failures and a test-refinement assertion mistake are retained alongside
the final green output, rather than replacing the audit trail. No runtime
snapshot compatibility claim is made: this patch requires the parent v7 change.
