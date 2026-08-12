//! `.szx` (ZX-State) snapshot parser.
//!
//! SZX is Spectaculator's chunked snapshot format, and the one several
//! accuracy test programs ship in *exclusively* — including
//! `ZX Spectrum Timing Tests - 128K`, the direct counterpart of the
//! `timingTests48k.sna` the timing survey already runs, by the same
//! authors. Without a reader for it the 128K has no program-level timing
//! oracle at all.
//!
//! ## The format
//!
//! An 8-byte header, then a flat sequence of length-prefixed chunks:
//!
//! ```text
//! header   "ZXST", u8 major, u8 minor, u8 machineId, u8 flags
//! chunk    char[4] id, u32 size (LE), u8 data[size]
//! ```
//!
//! Chunks may appear in any order and unknown ones are skipped by design —
//! the format is extensible, and a reader that rejected what it did not
//! recognise would break on every snapshot carrying a peripheral it has
//! never heard of. This parser reads the four that describe machine state
//! and ignores the rest, which is why a file with `MFCE` (Multiface) or
//! `PLTT` (palette) in it loads without complaint.
//!
//! | chunk | carries |
//! |---|---|
//! | `Z80R` | CPU registers, `I`, `R`, `IFF1`/`IFF2`, `IM` |
//! | `SPCR` | border, `$7FFD`, `$1FFD`/`EFF7`, last `$FE` |
//! | `RAMP` | one 16 KiB RAM page, optionally zlib-compressed |
//! | `AY\0\0` | AY-3-8912 selected register and the sixteen registers |
//!
//! Deliberately not read: `ROM ` (custom ROM images — the machine crates
//! load their own, and honouring a snapshot's ROM would silently swap the
//! firmware under a test), `TAPE`, `KEYB`, `CRTR`, and the peripheral
//! chunks. `DSK`/`+3` disk state has no home in `Snapshot` either.

use flate2::read::ZlibDecoder;
use format_sinclair_zx_spectrum_snapshot::{Snapshot, SnapshotModel};
use std::io::Read;

/// Bytes in one RAM page.
const PAGE_LEN: usize = 16_384;

/// `ZXSTRF_COMPRESSED` — the page body is a zlib stream, not raw bytes.
const RAMP_FLAG_COMPRESSED: u16 = 1;

/// Parse a `.szx` snapshot.
///
/// # Errors
///
/// Returns a description if the magic is wrong, the file is truncated
/// mid-chunk, the machine is one `Snapshot` cannot represent, or a RAM
/// page fails to decompress to exactly 16 KiB.
pub fn parse_szx(data: &[u8]) -> Result<Snapshot, String> {
    if data.len() < 8 {
        return Err(format!(
            "SZX too short: {} bytes, need at least 8",
            data.len()
        ));
    }
    if &data[0..4] != b"ZXST" {
        return Err(format!(
            "not an SZX snapshot: magic is {:02X?}, expected \"ZXST\"",
            &data[0..4]
        ));
    }
    let major = data[4];
    let minor = data[5];
    let machine_id = data[6];
    let model = model_from_machine_id(machine_id)?;

    let mut snap = empty_snapshot(model);
    let mut seen_z80r = false;
    let mut offset = 8usize;

    while offset < data.len() {
        if offset + 8 > data.len() {
            return Err(format!(
                "SZX truncated: {} trailing bytes at offset {offset} are too few for a chunk header",
                data.len() - offset
            ));
        }
        let id = &data[offset..offset + 4];
        let size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;
        let body_start = offset + 8;
        let body_end = body_start
            .checked_add(size)
            .ok_or_else(|| format!("SZX chunk at {offset} declares an absurd size {size}"))?;
        if body_end > data.len() {
            return Err(format!(
                "SZX chunk {} at offset {offset} declares {size} bytes but only {} remain",
                String::from_utf8_lossy(id).trim_end_matches('\0'),
                data.len() - body_start
            ));
        }
        let body = &data[body_start..body_end];

        match id {
            b"Z80R" => {
                read_z80_regs(body, &mut snap)?;
                seen_z80r = true;
            }
            b"SPCR" => read_spectrum_regs(body, &mut snap, model)?,
            b"RAMP" => {
                let (bank, bytes) = read_ram_page(body)?;
                if let Some(page) = snapshot_page_for_bank(bank, model) {
                    snap.pages.push((page, bytes));
                }
            }
            b"AY\0\0" => read_ay(body, &mut snap)?,
            // Every other chunk is someone else's business. See the module
            // docs: skipping the unknown is what the format is for.
            _ => {}
        }

        offset = body_end;
    }

    if !seen_z80r {
        return Err(format!(
            "SZX v{major}.{minor} has no Z80R chunk, so it carries no CPU state"
        ));
    }
    if snap.pages.is_empty() {
        return Err("SZX has no RAMP chunks, so it carries no memory".to_string());
    }

    Ok(snap)
}

/// Translate an SZX RAM **bank** number into the page number
/// `Snapshot::pages` is defined in.
///
/// SZX numbers `RAMP` chunks by raw RAM bank — 0..7 on a 128K, and 5/2/0
/// on a 48K for `$4000`/`$8000`/`$C000`. The shared appliers in
/// `common-sinclair-zx-spectrum::snapshot` do **not** use that numbering;
/// they use `.z80` v2/v3's, because that is the format they were written
/// for. Emitting raw banks means `apply_128k_bank_pages` reads bank 0 as
/// a ROM page and drops it, shifts the rest by three, and silently loads a
/// machine with five of its eight banks in the wrong place.
///
/// That is not hypothetical — it is what this parser did until the
/// 128K timing suite was booted and five banks came back wrong.
///
/// | class | applier | mapping |
/// |---|---|---|
/// | 128K-family | `apply_128k_bank_pages` | `page = bank + 3` |
/// | 16K/48K/+ | `apply_48k_pages` | bank 5 → 8, 2 → 4, 0 → 5 |
///
/// The two disagree with each other — page 4 is bank 1 under one and
/// `$8000` under the other — which is why this is keyed on the model
/// rather than done once.
fn snapshot_page_for_bank(bank: u8, model: SnapshotModel) -> Option<u8> {
    match model {
        SnapshotModel::Spectrum48K => match bank {
            5 => Some(8),
            2 => Some(4),
            0 => Some(5),
            // A 48K machine has three banks; anything else is malformed.
            _ => None,
        },
        _ => {
            if bank > 7 {
                return None;
            }
            Some(bank + 3)
        }
    }
}

/// `ZXSTMID_*`, from the ZX-State specification.
fn model_from_machine_id(id: u8) -> Result<SnapshotModel, String> {
    Ok(match id {
        // 16K is a 48K with less RAM paged in; the snapshot's own pages
        // decide what exists, and no machine crate wants a distinct model.
        0 | 1 | 15 => SnapshotModel::Spectrum48K,
        2 | 16 => SnapshotModel::Spectrum128K,
        3 => SnapshotModel::SpectrumPlus2,
        4 => SnapshotModel::SpectrumPlus2A,
        5 | 6 => SnapshotModel::SpectrumPlus3,
        7 | 13 | 14 => SnapshotModel::Pentagon128,
        10 => SnapshotModel::Scorpion256,
        other => {
            return Err(format!(
                "SZX machine id {other} is not a Spectrum this workspace models"
            ));
        }
    })
}

fn empty_snapshot(model: SnapshotModel) -> Snapshot {
    Snapshot {
        af: 0,
        bc: 0,
        de: 0,
        hl: 0,
        af_alt: 0,
        bc_alt: 0,
        de_alt: 0,
        hl_alt: 0,
        ix: 0,
        iy: 0,
        sp: 0,
        pc: 0,
        i: 0,
        r: 0,
        im: 1,
        iff1: false,
        iff2: false,
        border: 0,
        model,
        port_7ffd: 0,
        port_1ffd: 0,
        ay_register: 0,
        ay_regs: [0; 16],
        pages: Vec::new(),
    }
}

fn word(body: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([body[at], body[at + 1]])
}

/// `ZXSTZ80REGS` — 37 bytes.
fn read_z80_regs(body: &[u8], snap: &mut Snapshot) -> Result<(), String> {
    // The trailing `dwCyclesStart`, `chHoldIntReqCycles`, `chFlags` and
    // `wMemPtr` are not read: `Snapshot` has nowhere to put a T-state
    // position, and a machine that resumed at one would need the whole
    // frame's raster state to go with it. 29 bytes is what this parser
    // needs, so a v1.0 file that stops short of 37 still loads.
    const NEEDED: usize = 29;
    if body.len() < NEEDED {
        return Err(format!(
            "SZX Z80R chunk is {} bytes, need at least {NEEDED}",
            body.len()
        ));
    }
    snap.af = word(body, 0);
    snap.bc = word(body, 2);
    snap.de = word(body, 4);
    snap.hl = word(body, 6);
    snap.af_alt = word(body, 8);
    snap.bc_alt = word(body, 10);
    snap.de_alt = word(body, 12);
    snap.hl_alt = word(body, 14);
    snap.ix = word(body, 16);
    snap.iy = word(body, 18);
    snap.sp = word(body, 20);
    snap.pc = word(body, 22);
    snap.i = body[24];
    snap.r = body[25];
    snap.iff1 = body[26] != 0;
    snap.iff2 = body[27] != 0;
    snap.im = body[28];
    Ok(())
}

/// `ZXSTSPECREGS` — 8 bytes.
fn read_spectrum_regs(
    body: &[u8],
    snap: &mut Snapshot,
    model: SnapshotModel,
) -> Result<(), String> {
    if body.len() < 4 {
        return Err(format!(
            "SZX SPCR chunk is {} bytes, need at least 4",
            body.len()
        ));
    }
    snap.border = body[0] & 0x07;
    snap.port_7ffd = body[1];
    // Byte 2 is a union: `ch1ffd` on the Amstrad machines, `chEff7`
    // elsewhere. Reading it as `$1FFD` on a Pentagon would page banks that
    // machine does not have.
    snap.port_1ffd = match model {
        SnapshotModel::SpectrumPlus2A | SnapshotModel::SpectrumPlus3 => body[2],
        _ => 0,
    };
    Ok(())
}

/// `ZXSTRAMPAGE` — `u16` flags, `u8` **bank** number, then the page.
fn read_ram_page(body: &[u8]) -> Result<(u8, Vec<u8>), String> {
    if body.len() < 3 {
        return Err(format!(
            "SZX RAMP chunk is {} bytes, need at least 3",
            body.len()
        ));
    }
    let flags = word(body, 0);
    let page_no = body[2];
    let payload = &body[3..];

    let bytes = if flags & RAMP_FLAG_COMPRESSED != 0 {
        let mut out = Vec::with_capacity(PAGE_LEN);
        ZlibDecoder::new(payload)
            .read_to_end(&mut out)
            .map_err(|e| format!("SZX RAMP page {page_no} failed to decompress: {e}"))?;
        out
    } else {
        payload.to_vec()
    };

    if bytes.len() != PAGE_LEN {
        return Err(format!(
            "SZX RAMP page {page_no} is {} bytes, expected {PAGE_LEN}",
            bytes.len()
        ));
    }
    Ok((page_no, bytes))
}

/// `ZXSTAYBLOCK` — flags, selected register, then sixteen registers.
fn read_ay(body: &[u8], snap: &mut Snapshot) -> Result<(), String> {
    const NEEDED: usize = 18;
    if body.len() < NEEDED {
        return Err(format!(
            "SZX AY chunk is {} bytes, need {NEEDED}",
            body.len()
        ));
    }
    snap.ay_register = body[1];
    snap.ay_regs.copy_from_slice(&body[2..18]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn z80r_body() -> Vec<u8> {
        let mut b = Vec::new();
        for w in [
            0x1234u16, 0x2345, 0x3456, 0x4567, 0x5678, 0x6789, 0x789A, 0x89AB, 0x9ABC, 0xABCD,
            0xBCDE, 0xCDEF,
        ] {
            b.extend_from_slice(&w.to_le_bytes());
        }
        b.push(0x3F); // I
        b.push(0x7E); // R
        b.push(1); // IFF1
        b.push(0); // IFF2
        b.push(2); // IM
        b.extend_from_slice(&0u32.to_le_bytes()); // dwCyclesStart
        b.push(0);
        b.push(0);
        b.extend_from_slice(&0u16.to_le_bytes());
        b
    }

    fn header(machine_id: u8) -> Vec<u8> {
        vec![b'Z', b'X', b'S', b'T', 1, 4, machine_id, 0]
    }

    fn ramp(page: u8, fill: u8, compressed: bool) -> Vec<u8> {
        let raw = vec![fill; PAGE_LEN];
        let mut body = Vec::new();
        body.extend_from_slice(&(u16::from(compressed)).to_le_bytes());
        body.push(page);
        if compressed {
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
            enc.write_all(&raw).expect("encode");
            body.extend_from_slice(&enc.finish().expect("finish"));
        } else {
            body.extend_from_slice(&raw);
        }
        chunk(b"RAMP", &body)
    }

    fn minimal_128k(compressed: bool) -> Vec<u8> {
        let mut f = header(2);
        f.extend_from_slice(&chunk(b"Z80R", &z80r_body()));
        f.extend_from_slice(&chunk(b"SPCR", &[0x02, 0x11, 0x00, 0xFF, 0, 0, 0, 0]));
        for page in 0..8u8 {
            f.extend_from_slice(&ramp(page, page, compressed));
        }
        let mut ay = vec![0u8, 0x07];
        ay.extend_from_slice(&(0..16u8).collect::<Vec<_>>());
        f.extend_from_slice(&chunk(b"AY\0\0", &ay));
        f
    }

    #[test]
    fn reads_registers_paging_and_pages() {
        let snap = parse_szx(&minimal_128k(true)).expect("parse");
        assert_eq!(snap.model, SnapshotModel::Spectrum128K);
        assert_eq!(snap.af, 0x1234);
        assert_eq!(snap.pc, 0xCDEF);
        assert_eq!(snap.i, 0x3F);
        assert_eq!(snap.r, 0x7E);
        assert!(snap.iff1);
        assert!(!snap.iff2);
        assert_eq!(snap.im, 2);
        assert_eq!(snap.border, 2);
        assert_eq!(snap.port_7ffd, 0x11);
        assert_eq!(snap.ay_register, 0x07);
        assert_eq!(snap.ay_regs[3], 3);
        assert_eq!(snap.pages.len(), 8);
        // Bank 5 is emitted as page 8 for the 128K applier — see
        // `snapshot_page_for_bank`.
        let bank5 = snap
            .pages
            .iter()
            .find(|(n, _)| *n == 5 + 3)
            .expect("bank 5 present");
        assert!(bank5.1.iter().all(|&b| b == 5));
    }

    /// Compressed and uncompressed pages must produce identical memory.
    /// The flag is one bit and getting it backwards would still "work" on
    /// files that happened to be raw.
    #[test]
    fn compression_is_transparent() {
        let a = parse_szx(&minimal_128k(true)).expect("compressed");
        let b = parse_szx(&minimal_128k(false)).expect("uncompressed");
        assert_eq!(a.pages, b.pages);
    }

    /// Unknown chunks are skipped, not rejected. This is the property that
    /// makes the parser survive real files — the 128K timing suite carries
    /// a Multiface chunk, and SpecEmu writes a creator block.
    #[test]
    fn unknown_chunks_are_skipped() {
        let mut f = header(2);
        f.extend_from_slice(&chunk(b"CRTR", b"SpecEmu\0padding padding padding"));
        f.extend_from_slice(&chunk(b"Z80R", &z80r_body()));
        f.extend_from_slice(&ramp(5, 0xAA, true));
        f.extend_from_slice(&chunk(b"MFCE", &[0u8; 33]));
        let snap = parse_szx(&f).expect("parse");
        assert_eq!(snap.pc, 0xCDEF);
        assert_eq!(snap.pages.len(), 1);
    }

    /// `$1FFD` is a union with `EFF7` and must only be read on the
    /// machines that have it. A Pentagon snapshot whose `EFF7` happened to
    /// be non-zero would otherwise page ROM in the +3's special mode.
    #[test]
    fn the_1ffd_union_is_read_only_on_amstrad_machines() {
        let spcr = [0u8, 0x10, 0x07, 0xFF, 0, 0, 0, 0];
        for (machine_id, expect) in [(4u8, 0x07u8), (5, 0x07), (2, 0), (7, 0)] {
            let mut f = header(machine_id);
            f.extend_from_slice(&chunk(b"Z80R", &z80r_body()));
            f.extend_from_slice(&chunk(b"SPCR", &spcr));
            f.extend_from_slice(&ramp(0, 0, true));
            let snap = parse_szx(&f).expect("parse");
            assert_eq!(
                snap.port_1ffd, expect,
                "machine id {machine_id} read the SPCR union wrongly"
            );
        }
    }

    /// The bank-to-page translation, against the appliers it feeds.
    ///
    /// This is the defect that shipped in the first draft and was caught
    /// only by booting a real snapshot: raw SZX bank numbers handed
    /// straight to `apply_128k_bank_pages` load five of eight banks into
    /// the wrong slots and drop the rest, with no error anywhere.
    #[test]
    fn banks_are_translated_into_the_page_numbering_the_appliers_use() {
        // 128K family: page = bank + 3, so banks 0..7 become pages 3..10.
        for model in [
            SnapshotModel::Spectrum128K,
            SnapshotModel::SpectrumPlus2,
            SnapshotModel::SpectrumPlus2A,
            SnapshotModel::SpectrumPlus3,
            SnapshotModel::Pentagon128,
            SnapshotModel::Scorpion256,
        ] {
            for bank in 0..8u8 {
                assert_eq!(
                    snapshot_page_for_bank(bank, model),
                    Some(bank + 3),
                    "{model:?} bank {bank}"
                );
            }
            assert_eq!(snapshot_page_for_bank(8, model), None, "{model:?} bank 8");
        }

        // 48K: the .z80 region scheme, which is a different mapping and
        // not merely an offset.
        assert_eq!(
            snapshot_page_for_bank(5, SnapshotModel::Spectrum48K),
            Some(8)
        );
        assert_eq!(
            snapshot_page_for_bank(2, SnapshotModel::Spectrum48K),
            Some(4)
        );
        assert_eq!(
            snapshot_page_for_bank(0, SnapshotModel::Spectrum48K),
            Some(5)
        );
        assert_eq!(snapshot_page_for_bank(1, SnapshotModel::Spectrum48K), None);
    }

    /// A 48K SZX must come out as the `(8, 4, 5)` triple the `.sna` and
    /// `.z80` 48K parsers produce, because `apply_48k_pages` accepts
    /// nothing else.
    #[test]
    fn a_48k_snapshot_produces_the_region_triple() {
        let mut f = header(1);
        f.extend_from_slice(&chunk(b"Z80R", &z80r_body()));
        for bank in [5u8, 2, 0] {
            f.extend_from_slice(&ramp(bank, bank, true));
        }
        let snap = parse_szx(&f).expect("parse");
        let mut pages: Vec<u8> = snap.pages.iter().map(|(n, _)| *n).collect();
        pages.sort_unstable();
        assert_eq!(pages, vec![4, 5, 8]);

        // And the contents must follow their region, not their number.
        let at_4000 = snap.pages.iter().find(|(n, _)| *n == 8).expect("$4000");
        assert!(at_4000.1.iter().all(|&b| b == 5), "$4000 must hold bank 5");
    }

    #[test]
    fn rejects_bad_magic_and_truncation() {
        assert!(parse_szx(b"NOPE0000").is_err());
        assert!(parse_szx(&[]).is_err());

        // A chunk claiming more bytes than the file holds.
        let mut f = header(2);
        f.extend_from_slice(b"Z80R");
        f.extend_from_slice(&9999u32.to_le_bytes());
        assert!(parse_szx(&f).is_err());

        // No CPU state at all.
        let mut f = header(2);
        f.extend_from_slice(&ramp(0, 0, true));
        assert!(parse_szx(&f).is_err());
    }

    /// A page that decompresses to the wrong length is a corrupt file, not
    /// a short read to be padded.
    #[test]
    fn rejects_a_page_that_is_not_16k() {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&[0u8; 100]).expect("encode");
        let mut body = vec![1u8, 0, 5];
        body.extend_from_slice(&enc.finish().expect("finish"));

        let mut f = header(2);
        f.extend_from_slice(&chunk(b"Z80R", &z80r_body()));
        f.extend_from_slice(&chunk(b"RAMP", &body));
        let err = parse_szx(&f).expect_err("short page must fail");
        assert!(err.contains("16384"), "unhelpful error: {err}");
    }
}
