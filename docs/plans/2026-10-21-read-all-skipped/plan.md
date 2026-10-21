# All-skipped Read Data result research

Lane: Spectrum launch-hardening. Reference research only; no production, API, schema or dependency changes.

1. Re-read primary NEC SK/deleted-mark/EOT prose and current reference controller paths without treating prior results as authoritative.
2. Reuse the unmodified Fuse public-register/raw-track host and MAME direct-port research host. Observe exact native-owner tuple: C0/H0/N2, command R1/EOT1, GPL2A/DTLFF, logical A, MFM, non-DMA/noTC; physical IDs1(deleted)/2(normal),512byte fields with ((seed+offsetLo) wrapping)^offsetHi, seeds33/34. R2 lies beyondEOT and must not be hostpayload for this case.
3. Include single-sectorR1 contrast, ReadDeleted+SK with opposite marks, and SK-off readable controls. Repeat each with exactpayload/resultdrain/recovery/source/live-media checks; two emulated seconds per Fuse phase,10 total emulated seconds for MAME,15/45hostseconds.
4. Retain exact reference/tool/firmware/source/media identities and raw bounded observations. If results disagree and primary text does not choose an oracle, retain a distinct owning research issue and required-blocked dependency rather than guess a productionfix or upstreambug.
5. Push readable results plus replay/capture source/auditor in the owning repo; root reviews any merge/release separately. Clean all task-owned artifacts after verifiedretention.
