# Export the +3 live disk through shared tools

Lane: Spectrum launch-hardening. The user approved a shared `export_media`
script/MCP command. Only +3 EDSK is advertised initially. No new dependency,
snapshot version, controller timing behaviour or automatic writeback.

1. Add `disk_for_export` to `nec-upd765a`: borrowed live image, explicit absent
   drive/media errors, refuse active Write Data/Format Track. Test no mutation
   and resumed operation. Do not serialize cached mount bytes.
2. In `format-amstrad-dsk`, adapt live controller sectors to the existing neutral
   model and call the published 0.1.1 encoder. Update only that dependency/lock
   entry; preserve order, CHRN/status/lengths. Test changed bytes and limits.
3. Extend `MachineCore` with a default-unsupported byte export method. Delegate
   through Spectrum family/variant layers; only +3 disk-a/edsk succeeds. Add
   `media-export` to the +3 profile only. No snapshot fields change.
4. Promote the UI's new-file export helper to the shell and reuse it. Add a
   shared session method, `ExportMedia` step/observation and capability-routed
   MCP registration. Validate explicit format/extension and create output only
   after successful encoding. Tests cover missing capability/slot/media, active
   writes, existing files/links, invalid destinations and no state advance.
5. Run affected tests, all-target Clippy, formatting and Spectrum build. Keep
   cross-family compile coverage appropriate to the shared additions.
6. In the isolated umbrella worktree, extend the existing ROM-format consumer
   with actual export and new-process reopen. Independently validate EDSK and
   reopen in MAME through keyboard/ROM I/O; no injection or restored state may
   act as loading evidence. Run twice with corruption, wrong-result,
   missing-prerequisite and overwrite/protection controls. Retain truthful
   versioned results, commands, hashes, logs and explicit exclusions.
7. Push/review/rebase merge owning PRs. Ship through the existing release
   workflow if it can be done without taking over another session's release.
   Rerun with released tools before assigning release evidence. Update #45 and
   #67/#84 only to the scope demonstrated. Preserve remote commits/evidence;
   remove owned worktrees/build caches/processes and leave other work intact.
