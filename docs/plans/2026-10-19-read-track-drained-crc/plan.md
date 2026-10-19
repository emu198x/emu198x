# Fully drained Read Track CRC comparison (#1851)

Lane: Spectrum launch-hardening; bounded evidence study approved by the user.
Base: d39df56143246efcef945e52afbc9b78f7b1cfd9. No production behavior,
API, dependency or snapshot change is planned. Existing v0.38.3 ROM qualification
is regression context, not evidence for these controller fault cases.

1. Reuse the event host/raw MFM adapter from the interrupted-ID study, and the
   installed-MAME direct-port Lua probe from the Scan study. Add authored probe
   sources under this folder, never edit vendored reference code. The previous
   probes supply the reusable mechanism; absent historical archive paths do not
   justify a new framework.
2. Observe one sector [1] and two sectors [1,2]/[2,1], clean, ID-only and data-only
   CRC errors in first/last fields. EOT counts fields. Run each case twice on fresh Emu controllers and in
   fresh processes on unchanged Fuse and installed MAME where available.
   Consume all bytes and seven result bytes; verify next-command recovery,
   physical payload order, immutable media and host/emulated bounds.
3. Add an Emu test/probe at crates/nec-upd765a/tests/read_track_drained.rs using
   public registers and Peripheral ticks. Record observations without asserting
   disputed whole-result values from the implementation. Reuse the same matrix
   and independently audited raw-CRC controls for Fuse; DSK metadata for MAME.
4. Retain exact source/executable/adapter/firmware identities, commands,
   environment, raw logs and media in a versioned source-only evidence packet.
   Distinguish inspected MAME source from its unknown-source installed binary,
   controller injection from ROM qualification, and observed agreement from
   silicon truth. No upstream Fuse report.
5. Audit DE/DD, ND/EN, ST0 and CHRN separately; obtain applicable primary or
   physical evidence before altering disputed behavior. If evidence is
   inconclusive, document it and link the smallest follow-up rather than invent
   a fix. Combined ID+data corruption and gaps/flux/index/timing stay unqualified.
6. Add positive/negative auditors that reject missing/duplicate observations,
   zero bytes/assertions, altered media/CRC/marks, false agreement/qualification
   and incomplete result/recovery. Run controller tests, targeted Clippy/fmt,
   auditor tests and whitespace checks. Push a reviewable owning Emu PR and
   merge only after verified checks; keep broader rollout open.
7. Update umbrella progress/visual checklist in its own PR after evidence is
   remote. Retain a standalone verified export, remove task tools/caches/private
   copies and linked worktrees after checking refs and all ignored files.
