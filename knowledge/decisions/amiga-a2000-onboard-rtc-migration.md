# A2000 onboard clock and v68 migration

Adopted 2026-11-02 under the approved remaining-Amiga split plan. Fix the
observed decoder bug before extracting fixed owners. Commodore's A2000CR
rev-6 schematics, PN 314135-02 (August 1989), PDF page 4 / printed parts page 1,
list the OKI MSM6242B realtime clock at U801 for PCB 312720-05 NTSC and -06 PAL.
Primary library identifier:
`reference/by-system/commodore-amiga/a2000-system-schematics-rev-6-pn-314135-02-1989-08`.
Service update issue 25, item 25/3-2.1 (PDF 43), says rev 6 ships jumpered
for 1 MiB chip RAM. The existing no-slow-RAM preset has that correct layout.

The old construction enabled RTC decode only when slow RAM was fitted,
leaving the stock A2000's existing clock inaccessible. An owned CPU guest
writes seconds=5 but reads 15 in both regions. Both A501 positive controls
and both unexpanded A500 absence controls pass. WinUAE CP_A2000 independently
enables cs_rtc regardless of expansion RAM; it is implementation precedent,
not the primary hardware authority. No clock-chip timing changes are needed.

The approved family writer emits v68. Explicit v66/v67 readers enable only
the legacy A2000 saved RTC-presence flag, retaining the complete saved clock
history and every other field. The existing v66 A500+ migration remains;
malformed v67 A500+ state is not newly grandfathered. V68 cannot remove a
fitted A2000/A500+ clock. Candidate restore/validation precedes live mutation.
Other payloads change only the envelope header, apart from the already-approved
v66 A500+ presence correction. Versions 65 and older remain rejected.

Before correction, 56 original v67 cases were captured from untouched
`c06321677cde6b4aea0211197eb8436ceae0a1a7`. They cover all twelve existing
OCS-shaped profiles, original/8372A silicon, both A1000 boot phases, A501/A530,
progressive/interlaced output and real debugger half-CCK cuts. The archive
retains exact initial/final envelopes and every video/audio/timing observation;
177 decompressed identities are verified. Migration comparisons permit only
the documented header/clock-flag changes, without recording replacement goldens.

A new cold machine seeds its clock once from host time. Independent cold starts
therefore compare guest-native packets rather than assuming the same calendar
input. Restored paths still compare every saved byte apart from the explicit
migration, including the original RTC history and subsecond phase.
