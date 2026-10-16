The first-program source, session, tape and reference runtime come unchanged from
umbrella audit 5d9c49274b456306ff1ecde350b3260ccbbb64fc,
`docs/audits/2026-10-16-spectrum-source-debugging/`. The native journey assembles
that source afresh and runs the retained session against released tools. The
reference tape/runtime are unit-control fixtures, not substituted native results.

The `amiga/` source, runner and verifier are exact retained bytes from umbrella
commit b875be8508eedd1d58e9df950548f45437b7a663,
`docs/audits/2026-10-16-amiga-package-reopen/`. They derive from public sample
revision 51297ac3196a990286b0d21a4dedd4653c0168f0; `source.json` binds inputs
and canonical rebuilt products. The user explicitly approved copying these
seven non-ROM files into Emu on 2026-10-16. Firmware and credentials stay private.

The four JSON targets linked by the original Amiga README are retained directly
from that same public code-samples revision under `amiga/verification/evidence/`.
They keep its source-relative evidence links valid without changing the approved
README or implying that these older observations are the new native results.
