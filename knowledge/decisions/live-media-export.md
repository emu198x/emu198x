# Live media export uses the shared session boundary

Accepted 2026-10-16. Consumer: +3 disk export/reopen qualification, #45. The
user approved one shared script/MCP command rather than a Spectrum-only verb.

`MachineCore::export_media(slot, format)` returns encoded bytes from the live
medium, without host paths or time advancement. Its default refuses export.
The shared session checks the current profile's `media-export` capability,
requires the destination extension to match the explicit format, and creates
a new file. Existing paths, including symlinks/hard links, are never overwritten.
The UI and shared command use the same new-file writer. A filesystem write/sync
failure is reported; it may leave an incomplete newly created file, never an
overwritten source. No automatic save on eject or shutdown is introduced.

The script and MCP action is:

```json
{"action":"export_media","slot":"disk-a","format":"edsk","path":"new.edsk"}
```

Only the +3 advertises this initially. Other variants still refuse, including
after a live variant switch. Formats and slots are validated by the runtime;
MCP registration follows the declared capability through the existing shared
registrar. Existing system-specific export formats do not gain an untested
capability or change behaviour in this slice.

The +3 borrows its controller-owned image; cached mount bytes are provenance
only. Active Write Data/Format Track refuses export without advancing or
aborting the transfer. Completed/restored images encode via the existing
Format198x 0.1.1 adapter. No controller timing or snapshot fields change.
Sector-model preservation limits remain those of the format crate; this is
not a byte-preserving rewrite of an original container.

Qualification must read the actual exported file and reopen in fresh runtimes
through ROM loading. A source build, parse-only round trip, retained code in
old RAM or snapshot restoration cannot stand in for that gate.
