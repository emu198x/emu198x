# Sense Drive Status samples are separate from inserted media

**Status:** approved2026-10-20. Lane: Spectrum launch-hardening.

The generic µPD765A accepts independent asserted logical T0, WP and Fault
samples, alongside existing READY and TS. SDS latches the selected physical
unit's samples and echoes the command's logical unit/head. Reset preserves
externally supplied levels. Consumers without adapters retain explicit legacy
fallbacks. Invalid unit indices are ignored, matching READY and TS.

These are digital status samples. They do not implement RW/SEEK multiplex
sampling delays, fault-reset signalling, a stepping mechanism or drive spin-up.
Seek/Recalibrate retain the existing instantaneous head-position model. Their
mechanical accuracy is not inherited from SDS tests. WP assertion inhibits
mutations, including an active write or format; WP deassertion never overrides
the host's read-only mount policy. Exact asynchronous WP sampling timing is
unqualified.

The +3 adapter supplies T0 from its physical head counter only if a mechanism
exists: internal A always exists; a mounted B image represents an external
mechanism. Empty external B is not yet independently configurable. Fault is
suppressed by the board's FLT/T0 gate. WP/TS retain the current mount-policy
level; an empty physical A's WP level remains unqualified.

Primary evidence: Amstrad *ZX Spectrum +3 Service Manual*, sheets12–14 and19;
NEC March1979 application note, printedp15 figure11. The bounded primary
extraction and current audit are in the umbrella:
[drive signals](https://github.com/stevehill1981/198x/blob/e3611eedf5e1975c98a645d410c74938c5f7e166/reference/by-system/sinclair-zx-spectrum/zx-spectrum-plus3-drive-signals.md),
[input audit](https://github.com/stevehill1981/198x/blob/e3611eedf5e1975c98a645d410c74938c5f7e166/docs/audits/2026-10-20-sds-empty-absent-drives.md).
zxsp `27d2c30d86d3f4877e24dd3dd97d7ebd51bab7c6` independently distinguishes
NoDrive from an empty mechanism; its default is precedent, not a measurement.

Spectrum native snapshots advance v8→v9 with approval, preserving the live
samples and any already-latched SDS result. Older states are rejected before
payload decode without modifying the running machine. CPC does not embed this
controller and needs no bump. Portable imports are separate.

Related: #1876. This does not qualify full SDS or another FDC operation.
