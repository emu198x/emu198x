# Classify initially-not-ready Recalibrate as abnormal Seek End

Lane: Spectrum launch-hardening. Parent frozen SIS result/phase closure gate.

Current Recalibrate without media returns D0: IC11, EC, no SE or NR. That
misclassifies failure to start as a READY transition and loses retained busy.
NEC December1978 printedp16 explicitly applies Seek READY-loss behaviour to
Recalibrate; printedp15 defines abnormal IC01 with NR. MAME `upd765.cpp`
`recalibrate_start` sets ST0_NR|ST0_FAIL|ST0_SE =68 for initially-not-ready.
Fuse instead uses its IC11 ready-termination representation; retain disagreement.

1. Public register test for all four logical units: initially-not-ready Recal,
   bounded completion, expected68|unit plus PCN0, retained busy before reading
   ST0, selected bit clear with PCN unread, final idle. Capture red first.
2. Correct only this initial classification D0→68, with source-backed comment.
   Do not change READY event latches, PCN policy, timing or save schema.
3. Run all controller tests, all-target Clippy and workspace formatting; retain
   red/green and exact versions/commands. Push separate parent-integration commit.

## Result

The regression fails specifically at ST0: observed208(D0), expected104(68).
After correction all151 controller tests pass, none ignored; all-target Clippy
and workspace formatting pass. The new test checks allfour logical units, both
SISresultbytes, retainedbusybeforeST0 and clearedbusywhilePCNunread. One private
Recal test pinned the old immediatebusyclear; it now explicitly drains both
results and verifies busyclearafterST0. Its initial failure is retained. No
firmware, referenceprocess or snapshotcompatibility claim is made by this test.
