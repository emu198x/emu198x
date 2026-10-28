# Einstein DSK consolidation (#277)

Lane: engineering-frontier host reliability. Base: 49956980d7841a53420aaa49ff16211161e38edb.

1. Compare the current inline parser's accepted geometry and safety checks with
   published format198x-amstrad-cpc-dsk 0.1.1. Verify the existing malformed-image
   and standard/extended controls before changing the parser.
2. Resolve codec gaps before requesting the dependency: the current shared
   DiskTrack drops physical track-header C/H and default N; validation also
   permits descriptor tables to extend into payload and clamps oversized N.
   Preserve Einstein's existing track placement and zeroed-track behaviour.
   Agree any additive Format198x API work before implementation.
3. After approval, use a controller-neutral validated container representation
   and retain only WD1770 representability checks/flattening locally. Reject
   unsupported status, missing/duplicate IDs and nonuniform geometry explicitly.
   Validate before replacing a mounted disk; source files remain immutable.
4. Run before/after malformed and valid controls, machine/controller tests,
   real-image compatibility probe, strict Clippy and formatting. Push a checked
   PR, require all reported CI complete and successful before rebase merge.
5. Verify issue closure and remote patch retention; remove task worktrees,
   temporary fixtures, build caches and processes.

## Verification

The user approved the additive strict API, publication and dependency. Format198x
#78 and release #79 are merged; 0.1.2 was downloaded from crates.io. The published
container source matches the reviewed source. The old parser API stays unchanged.
Only that registry package changes version in the Emu lockfile.

Affected machine/runtime/legacy format-adapter suites: 46 passed, zero failed,
4 ignored; all-target strict Clippy and formatting pass. Existing malformed-size,
payload and atomic-replacement controls pass. Physical headers reordered relative
to container order, and zeroed TIB compatibility, preserve the prior result.
The new unsupported-status/address-mark test fails on the inline-parser baseline
(5 pass, 1 fail), then passes with the checked adapter. These fields are now
rejected rather than silently discarded in the WD1770 flat sector model.

The existing real-MOS Ctrl-BREAK control passes through the published-codec
adapter: it issues disk commands and reads a complete sector. It is a transfer
control, not proof of a full DOS boot. Local-only fixture identities:

- mos.rom: 8,192 bytes, SHA-256 401d0e0bf6f64ba82e68137525749171fbcf9bd9055a49e5ac9a47941a6a0ae1.
- Chuckie Egg (1984)(Tatung Einsoft).dsk: 215,296 bytes, SHA-256 0f05b1575c4a3459c51fbd6592cf7d4ab978019f10a095421f16d2103617c7f4.

No machine snapshot fields or controller timing changed. Loaded files remain
read-only; the adapter validates fully before replacing a live disk.
