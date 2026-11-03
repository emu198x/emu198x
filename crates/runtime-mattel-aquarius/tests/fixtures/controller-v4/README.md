# Original controller-history v4 specimens

Captured before the v5 fix from b52deeb8ea7059d861d788dcb88c20985a507aa2,
using a synthetic 8 KiB ROM whose first two bytes are `18 FE` (JR to itself),
and the runtime default character ROM. Each file follows one native frame with
both named buttons held on the named port. Both NTSC and PAL boards are retained.

These complete original files are rejection controls, not v5 expectations. Their
SHA-256 identities are in `sha256.json`; do not regenerate them with the new writer.
Opposing directions produce identical saved electrical state, demonstrating why
v4 cannot recover logical held inputs exactly. No proprietary firmware is included.
