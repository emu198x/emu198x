# Read Data ignores Specify ND and does not acknowledge its completion IRQ

Current source `ce2d755aff02770c1bc9561f7211b30fadbbd255` has three distinct
signal defects. It does already assert the result-completion IRQ.

The bounded probe ran twice with Rust/Cargo 1.99.0. Each process returned 101,
with 3 passing and 4 failing tests. The second process records executable, probe,
library source and Cargo.lock SHA256 identities. No production source changed.
The ordinary controller suite still passes 177 tests after removing the probe.

## Observations

| Check | Expected from manufacturer text | Current observation |
|---|---|---|
| Specify ND=0 then Read Data | MSR.EXM clear during execution | EXM remains32 |
| ND=0, intervening SDS, then Read Data | Programmed mode remains DMA | EXM still32; no stored mode exists |
| ND=1, first available payload byte | Assert INT; data read acknowledges it | INT staysfalse |
| Read Data reaches result phase | Assert completion INT | Pass: alreadytrue |
| First Read Data result byte read | Acknowledge completion INT | INT remains asserted |
| ND=1 execution-mode status | EXM set | Pass |
| ND=0 execution-byte interrupt | No byte INT | Pass; this is not DMA-transfer qualification |

The assertion path is `read_result` → `setup_result_read`, and the latter
already sets `interrupt=true`. Changing result assertion would fix nothing.
The actual result-read acknowledgement currently handles only `Command::ReadId`.
Specify accepts both parameter bytes, then discards them when the next command
clears `cmd_buf`. Preamble/read/write/Scan/Format status currently sets EXM
unconditionally. Read byte-request ticks set RQM without raising a byte IRQ.

## Primary evidence and independent implementation checks

The existing umbrella corpus holds the actual extracted text and source
provenance; no new hardware fact relies on emulator precedent alone.

- NEC December 1978 data sheet, printed p11 (processor interface) and p16
  (Specify): ND selects non-DMA versus DMA; non-DMA available payload bytes
  raise INT and the data-register read acknowledges it; DMA execution uses
  DRQ/DACK instead of byte IRQ; first result byte resets completion INT.
  PDFSHA256 `e8b07db283aad56358cdace1bf4ac179f961484d10d6c1700b74d9734eea1117`.
  https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf
- NEC A/B data sheet, printed 5-4 (INT),5-11 (EXM), 5-13 (ND), and Specify
  command layout: EXM is set only in non-DMA execution; Specify bit 0 selects
  the transfer mode. PDFSHA256
  `e8a37f1242d4e26b400a524519b0b2efea8a68b83b4770e7a081bb82f6edfebc`.
  https://hxc2001.com/download/datasheet/floppy/thirdparty/FDC/NEC/UPD765_Datasheet_OCRed.pdf
- NEC APC System Reference, printedA3-12 (PDF187), corroborates the per-byte
  read acknowledgement and first-result acknowledgement. PDFSHA256
  `be0438fc83ac10c85acc93658871b7a2d42aa252f28e34fde98fb33b4e34e0cf`.
  https://archive.computerhistory.org/resources/access/text/2021/08/102796836-05-01-acc.pdf
- Vendored MAME `upd765.cpp`: C_SPECIFY stores `spec`; `msr_r` gates EXM on
  `SPEC_ND`; `enable_transfer`/`disable_transfer` manages byte-request IRQ in
  non-DMA mode and DRQ separately; `command_end` asserts result IRQ and
  `fifo_r` acknowledges it on read. It distinguishes output IRQ from internal
  transfer and completion sources.
- Vendored Fuse 1.7.0 `upd_fdc.c`: Specify stores `non_dma`; execution status
  gates EXM on that flag. Its own comment explicitly says only non-DMA mode
  is emulated. Fuse clears a result IRQ at the last result byte, whereas the
  manufacturer processor-interface text and MAME require the first byte.
  That disagreement must not override the primary source.

## Smallest proposed change — approval required before implementation

1. Persist one private serialized boolean, `non_dma_mode`, sampled from
   Specify's final parameter bit 0. The current `cmd_buf` cannot serve as
   retained configuration because another command replaces it. For the ND
   and IRQ scope alone, no extra serialized IRQ field is necessary: existing
   phase, RQM, result position, countdown and `interrupt` already preserve
   request/result boundaries. Byte service must clear its IRQ before allowing
   the final byte's completion path to assert result IRQ in the same call.
2. Gate EXM on that retained mode. Raise byte IRQ at each non-DMA byte request;
   acknowledge it when the payload byte is accepted. Acknowledge the existing
   completion IRQ on first result-byte read. Keep MSR polling non-destructive.
3. Specify is global chip configuration. Shared Read Deleted Data/Read Track,
   Write/Scan/Format engines must not silently keep contradictory status and
   byte-IRQ behavior; identify and test these shared paths within the fix.
   Do not alter pacing, DTL, MT, magnetic fields, firmware or machine wiring.
4. Adding even one boolean changes the positional postcard controller payload.
   Spectrum native snapshots therefore need approved v9→v10, with v9 rejected
   atomically before payload decode, unless a larger explicit migration is
   chosen. CPC embeds no controller payload and needs no bump. Existing legacy
   raw-controller fixtures must remain explicitly identified/rejected, not
   relabelled as newly qualified v10 evidence.

This is enough for the programmed non-DMA digital IRQ contract. It is not a
complete Specify or DMA implementation. The existing controller exposes no
DRQ output, DACK input or TC input; ordinary data-register polling cannot
qualify their handshake. Correct ND=0 status/no-byte-IRQ checks must remain
separate from an actual DMA-transfer claim. Faithful DMA would require an
approved public signal surface and state/pin semantics as additional work.

SRT/HUT/HLT timers and exact head/step mechanics remain separate Specify work.
If implementing those now, retain both Specify bytes (for example a private
`[u8;2]`) instead of the single mode bit and derive the timers from the chip
clock. That is a larger decision; storing bytes does not implement timers.
Power-on/reset ND behavior also needs an explicit evidence-backed policy;
NEC reset text names timer preservation but does not settle ND. Fuse defaults
non-DMA on reset, and MAME preserves its Specify word. Qualification must issue
Specify explicitly rather than claim an unverified power-on mode.

No mode field, API, snapshot version, default or production behavior has been
changed by this investigation.

## Approved implementation scope

On 2026-10-21 the user approved one private ND boolean and Spectrum native
v9→v10 with old-v9 rejection. CPC and the DMA signal API remain unchanged.
Owning defect: https://github.com/emu198x/emu198x/issues/1939.

A second fail-first probe made 18 independent first-request observations across
Read Data/Deleted/Track, Write Data/Deleted, three Scan commands and Format,
with each ND value. All 18 show the same mode/request defects. The eighth test
reports all rows before failing; the complete source has 3 passing controls and
5 intended failures. The first seven-test probe remains available at commit
`b1f230245`. Compressed evidence logs preserve their exact original bytes.

The implementation retains the existing legacy non-DMA fallback for a new or
reset controller. This is an explicit compatibility policy, consistent with
Fuse's reset fallback; hardware power-on/reset ND remains unresolved. Programmed
mode is claimed only after Specify. It is not a timer or DMA-handshake claim.

Shared regression checks consume every payload byte and require per-byte IRQ
acknowledgement, final Read byte/result IRQ ordering, first-result acknowledgement
and immediate-error completion across all nine engines and both programmed
modes. Format's existing model has a full-track post-ID wait; tests bound that
wait at 1,000,000 ticks without changing it or treating it as physical evidence.
Native coverage spans 270 cases: nine engines, two modes and fifteen boundaries.
Active-write host export remains rejected; native state itself preserves the
complete live controller/disk, and continuation checks known final payload/mark.


Verification before committing the approved change: 575 owning controller,
Amstrad-class and Spectrum-runtime tests passed; 62 ignored tests receive no
credit. Strict Clippy on all targets for those three crates passed. The boot
invariant deliberately locked to v9 initially failed with actual 10/expected 9;
it now locks the approved v10, alongside atomic old-v9 rejection. No failing
check was bypassed.
