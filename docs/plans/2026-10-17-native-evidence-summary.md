# Publish the native installation evidence summary

Lane: Spectrum launch-hardening documentation. No code or runtime behaviour change.

1. Read public run/artifact records from 38963566008, 38964506987 and 38965383166.
2. Write `docs/status/native-installation-evidence.md` with accepted jobs,
   selected releases, product checks, rejected records and explicit exclusions.
   Use public sources only; do not export private umbrella evidence.
3. Run Markdown/link checks and verify every stated run, artifact and version.
4. Commit/push the documentation PR. Connect the summary to the existing public
   overview assessment after its immutable revision is available.
