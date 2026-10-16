# Qualify Intel macOS handoffs and clean installation routes (#167, #168)

The user approved the proposed sequence: execute the existing developer fixtures
on Intel macOS, then audit documented public installation routes on clean hosted
macOS and Windows environments. This is Spectrum launch-hardening validation,
with no emulator-core behaviour, dependencies or qualification architecture change.

1. Reuse `scripts/windows-developer-journey.py` and its exact package, title and
   source-stop invariants. Add strict native Mach-O host validation and an Intel
   macOS pin selecting the same Asm 0.0.67, Build 0.2.12 and Emu 0.35.2 publications.
   Add an opt-in Intel job to `.github/workflows/test-linux-packages.yml`, keeping
   ordinary Linux qualification as default. Verify local negative controls and
   actual native boot/debug/missing-CODE/missing-sidecar outcomes.
2. Inspect current public README/site/release installation routes. Retain exact
   source revisions, instructions, URLs, installer/formula hashes, redirects and
   host environments. Select versions separately for the installer audit: do not
   silently relabel pinned archive observations as current installer behaviour.
3. On disposable hosted Intel/ARM macOS runners, verify no family formula or
   executable is installed, use documented Homebrew commands and retain formula
   trust/prerequisite output, installed PATH/version/bytes and receipt identities.
   Run the same Spectrum behaviour fixture with the installed executables, then
   uninstall and require actual removal. Reuse existing runner Homebrew; record
   that Homebrew bootstrap and consumer desktop interaction remain untested.
4. On disposable hosted Windows, execute each available public PowerShell
   installer, retain its default PATH/profile/receipt behaviour, invoke installed
   tools, run the bounded fixture and exercise documented removal or explicitly
   record its absence. Where no vendor installer exists, record that gap rather
   than substituting ZIP extraction as installer success. Keep any archive-based
   runtime fallback distinct. No claim of Windows client GUI coverage.
5. Preserve actual passes/failures and safe logs. File only reproduced defects
   after deduplication. Product repairs require root-cause evidence and any local
   design agreement; do not broaden the audit into new installer features.
6. Retain final evidence/verifiers/report under `docs/audits/`, compare canonical
   products with #112, commit/push small changes and open reviewable PRs. Close
   bounded issues against their observed acceptance, with unavailable routes and
   remaining owners explicit. Inspect all scratch and verify remote retention
   before removing task worktrees and task-owned processes.

Workspaces: `/private/tmp/198x-native-install-handoffs` (umbrella) and
`/private/tmp/emu198x-macos-installer-handoffs` (Emu). Firmware uses the existing
Emu CI secret, stays outside uploaded evidence and is deleted in always steps.
Reuse existing corpus provisioning if a retained Amiga fixture is needed; never
export corpus credentials or private firmware to another repository.

## Private-fixture boundary

The first native archive job failed its private umbrella checkout before product
execution. Do not widen credentials or export the retained Amiga files without
the pending explicit approval. Keep the public Spectrum and installer checks
independent of that access. The prepared Amiga runner remains recoverable in
remote commit ab149d7cf; its native execution remains outstanding under #167.
The Homebrew Intel Spectrum result separately establishes its declared version
set and fixture outcomes; it does not stand in for the Amiga boot.
