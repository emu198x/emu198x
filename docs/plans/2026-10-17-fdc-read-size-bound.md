# Bound guest-supplied Read Data size codes

Lane: Spectrum launch-hardening. Base: `b8f1270ca`.

Read Data and Read Deleted Data shift `128usize` by the raw command N, then
reserve a buffer. An untrusted N=255 can panic before any sector lookup.
Write, Scan and Read Track already reject N>7. The held NEC December1978
sheet does not define all 256 command-N encodings. Fuse clamps N at8 and
MAME clamps >7 to16384 bytes; their disagreement does not establish a hardware
answer. The smallest bounded policy is the existing sector-model N<=7
boundary, with an explicit abnormal ND result above that boundary, preserving
N in the result. This is emulator input rejection, not chip conformance.

1. Reproduce N=255 through public registers under a host deadline, without
   allowing a potentially huge allocation.
2. Reject N>7 before shifting or reserving. Preserve valid N<=7 read behavior.
3. Verify both read commands with every rejected N, current maximum N7,
   truncated/empty/missing media, and command recovery. Run the controller
   suite, fmt and targeted all-target Clippy.
4. File the owning issue and independent PR, retain red/green logs, push and
   clean isolated workspace/cache after verified remote retention.

No dependency, schema, API or timing changes; no source media writes.

## Result

Tracked in https://github.com/emu198x/emu198x/issues/1819.

Both public-register N255 cases failed before the fix with `attempt to shift
left with overflow`. The bounded process exited101, using2,621,440 bytes peak
RSS. Host timeout15s and CPU10s were enforced. macOS refused data/address-space
limits, so no memory-limit claim is made; N255 debug fails before allocation.

After the guard, all104 controller tests pass (55 unit +49 integration),
including five new size-bound groups. The five new groups also pass in the
release profile. All-target controller Clippy, workspace format check and diff
check pass. Adjacent before/after/release/Clippy text files preserve output.

Every N8..255 is rejected for both commands across missing media, empty fields,
truncated fields, full16KiB fields and empty tracks. N7 payload and existing
short/empty-field fill behavior remain unchanged, and a valid read following
rejection succeeds. These tests characterize the existing sector model;
short-field padding and undocumented-N results are not hardware qualification.

No full runtime or ROM qualification is claimed by this isolated bug fix.
