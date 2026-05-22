//! Cross-variant format-load matrix.
//!
//! Proves every Spectrum format loads on every in-scope variant where
//! it's expected to. Synthetic minimal fixtures, no external files; the
//! test asserts the **wiring** (parser → apply / load → live runtime),
//! not gameplay. Lifts SOLID criterion 3 (Formats) from "we know it
//! works on 48K, the variant crates exist" toward
//! "the matrix is green by construction".
//!
//! Format / variant pairings follow the formats' machine-class targets:
//!
//! - **TAP / TZX** — universal; all 8 variants.
//! - **SNA-48K / Z80-v1** — 48K-class (16K, 48K, Spectrum+).
//! - **SNA-128K / Z80-v2** — 128K-class (128K, +2, +2A, +2B, +3).
//! - **DSK** — +3 only; asserts `load_media` succeeds. The actual
//!   disk-load path is pinned at
//!   `knowledge/decisions/spectrum-plus3-disk-loading-incomplete.md`.
//!
//! Cross-class loads (48K SNA on 128K, etc.) are out of scope here.
//!
//! See `docs/brainstorms/2026-05-08-spectrum-format-matrix-brainstorm.md`
//! for the brainstormed design.

use common_sinclair_zx_spectrum::snapshot::SnapshotModel;
use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet};
use format_sinclair_zx_spectrum_sna::parse_sna;
use format_sinclair_zx_spectrum_z80::parse_z80;
use machine_sinclair_zx_spectrum_16k::Spectrum16K;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus::SpectrumPlus;
use machine_sinclair_zx_spectrum_plus2::SpectrumPlus2;
use machine_sinclair_zx_spectrum_plus2a::SpectrumPlus2A;
use machine_sinclair_zx_spectrum_plus2b::SpectrumPlus2B;
use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;
use runtime_sinclair_zx_spectrum::{
    Model, Spectrum16kRuntime, Spectrum48kRuntime, Spectrum128kRuntime, SpectrumMachine,
    SpectrumPlus2ARuntime, SpectrumPlus2BRuntime, SpectrumPlus2Runtime, SpectrumPlus3Runtime,
    SpectrumPlusRuntime,
};

// ─── Synthetic fixture builders ────────────────────────────────────

/// Minimal 48K `.sna`: 27-byte header + 49 152 zeroed RAM = 49 179 bytes.
fn minimal_sna_48k() -> Vec<u8> {
    let mut data = vec![0u8; 49_179];
    // Border at offset 26 — defaults to 0, fine.
    // SP at offset 23–24 must point into RAM so the parser can pop PC.
    // Use SP=$4002 → PC bytes at ram[2..4]; PC=$8000 (in ROM, harmless).
    data[23] = 0x02;
    data[24] = 0x40;
    data[27 + 2] = 0x00;
    data[27 + 3] = 0x80;
    data
}

/// Minimal 128K `.sna`: 49 179 + 4 + 5 × 16 384 = 131 103 bytes.
fn minimal_sna_128k() -> Vec<u8> {
    let mut data = vec![0u8; 131_103];
    // PC at offset 49 179 (post-48K-block, byte 0 of the 4-byte 128K
    // extension). Set to $8000 so the parsed snapshot has a usable PC.
    data[49_179] = 0x00;
    data[49_180] = 0x80;
    // port_7ffd at offset 49 181 — current_bank = 0 (bits 0..2). Leave 0.
    // 49 182 = TR-DOS flag, ignored by the parser.
    data
}

/// Minimal `.z80` v1 snapshot: 30-byte header + 49 152 uncompressed
/// RAM. PC is at offset 6–7 in v1 and **must be non-zero**, otherwise
/// the parser routes the file to the v2/v3 path.
fn minimal_z80_v1() -> Vec<u8> {
    let mut data = vec![0u8; 30 + 49_152];
    data[6] = 0x00;
    data[7] = 0x80; // PC = $8000
    // byte 12 holds border / R-bit-7 / compression flags; leave 0 (no
    // compression, R bit 7 = 0, border = 0). The parser coerces 0xFF
    // to 1 — leaving 0 is safe.
    data
}

/// Minimal `.z80` v2 snapshot for a 128K-class machine. v1's PC slot
/// (offset 6–7) is zero so the parser routes to the v2/v3 path; the
/// extended header carries the real PC. hw_mode=3 selects 128K, and
/// eight uncompressed banks follow — pages 8 / 5 / 3 / 4 / 6 / 7 / 9
/// / 10 cover banks 5 / 2 / 0 / 1 / 3 / 4 / 6 / 7. A 48K-mode v2
/// snapshot (hw_mode=0, three pages) is left for a follow-up if we
/// want explicit Z80-v2-of-48K coverage on 48K-class variants.
fn minimal_z80_v2_128k() -> Vec<u8> {
    let mut data = vec![0u8; 30];
    data.push(23); // ext_len = 23
    data.push(0);
    data.push(0x00); // pc lo
    data.push(0x80); // pc hi
    data.push(3); // hw_mode = 3 (128K)
    data.extend(std::iter::repeat_n(0u8, 21));
    for page_num in [8u8, 5, 3, 4, 6, 7, 9, 10] {
        data.push(0xFF);
        data.push(0xFF);
        data.push(page_num);
        data.extend(std::iter::repeat_n(0u8, 16_384));
    }
    data
}

/// Minimal `.tap`: one 19-byte standard-speed block (length-prefixed,
/// flag 0x00 = header, 17 zero bytes, checksum 0x00). Same shape used
/// by `tests/variants.rs`'s tape-load assertions.
fn minimal_tap() -> Vec<u8> {
    let mut tap = vec![0x13, 0x00];
    tap.push(0x00);
    tap.extend_from_slice(&[0; 17]);
    tap.push(0x00);
    tap
}

/// Minimal `.tzx`: ZXTape! magic + version 1.20, no blocks. Lifted
/// from `tests/variants.rs`.
fn minimal_tzx() -> Vec<u8> {
    let mut tzx = b"ZXTape!\x1a".to_vec();
    tzx.push(1);
    tzx.push(20);
    tzx
}

/// Minimal `.dsk`: 256-byte standard-DSK header + 256-byte track
/// header reporting zero sectors. The format parser accepts this
/// (returns a `DiskTrack::default()` for zero-sector tracks); the +3
/// FDC accepts it too.
fn minimal_dsk() -> Vec<u8> {
    let header_len = 256;
    let track_header_len = 256;
    let mut buf = vec![0u8; header_len + track_header_len];
    buf[..b"MV - CPC".len()].copy_from_slice(b"MV - CPC");
    buf[0x30] = 1; // tracks per side
    buf[0x31] = 1; // sides
    let track_size = (track_header_len as u16).to_le_bytes();
    buf[0x32] = track_size[0];
    buf[0x33] = track_size[1];
    let t = header_len;
    buf[t..t + b"Track-Info\r\n".len()].copy_from_slice(b"Track-Info\r\n");
    // 0 sectors → parse_track returns DiskTrack::default(); no further
    // sector-data layout needed.
    buf[t + 0x15] = 0;
    buf
}

// ─── Assertion helpers ─────────────────────────────────────────────

fn load_tape_into<M: SpectrumMachine>(
    runtime: &mut runtime_sinclair_zx_spectrum::SpectrumRuntime<M>,
    bytes: &[u8],
) {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, bytes));
    MachineCore::load_media(runtime, &media).expect("tape media should load");
    // No assertion on tape_is_loaded here — the minimal TZX fixture
    // has zero blocks (just the magic header) so the tape player has
    // nothing to play, even though the parse + load succeeded. The
    // wiring assertion is "load_media returns Ok"; the assertion
    // that tape blocks land in the player belongs to the format
    // crates' own tests.
}

fn load_disk_into<M: SpectrumMachine>(
    runtime: &mut runtime_sinclair_zx_spectrum::SpectrumRuntime<M>,
    bytes: &[u8],
) {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("disk-a", MediaKind::Disk, bytes));
    MachineCore::load_media(runtime, &media).expect("disk media should load");
}

// ─── 48K-class: 16K / 48K / Spectrum+ × {TAP, TZX, SNA-48K, Z80-v1} ──

#[test]
fn spectrum_16k_loads_tap() {
    let mut runtime = Spectrum16kRuntime::new(Model::Spectrum16KPal, Spectrum16K::new());
    load_tape_into(&mut runtime, &minimal_tap());
}

#[test]
fn spectrum_16k_loads_tzx() {
    let mut runtime = Spectrum16kRuntime::new(Model::Spectrum16KPal, Spectrum16K::new());
    load_tape_into(&mut runtime, &minimal_tzx());
}

#[test]
fn spectrum_16k_loads_sna_48k() {
    let mut runtime = Spectrum16kRuntime::new(Model::Spectrum16KPal, Spectrum16K::new());
    let snap = parse_sna(&minimal_sna_48k()).expect("parse SNA 48K");
    assert_eq!(snap.model, SnapshotModel::Spectrum48K);
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_16k_loads_z80_v1() {
    let mut runtime = Spectrum16kRuntime::new(Model::Spectrum16KPal, Spectrum16K::new());
    let snap = parse_z80(&minimal_z80_v1()).expect("parse Z80 v1");
    assert_eq!(snap.model, SnapshotModel::Spectrum48K);
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_48k_loads_tap() {
    let mut runtime = Spectrum48kRuntime::new(Model::Spectrum48KPal, Spectrum48k::new());
    load_tape_into(&mut runtime, &minimal_tap());
}

#[test]
fn spectrum_48k_loads_tzx() {
    let mut runtime = Spectrum48kRuntime::new(Model::Spectrum48KPal, Spectrum48k::new());
    load_tape_into(&mut runtime, &minimal_tzx());
}

#[test]
fn spectrum_48k_loads_sna_48k() {
    let mut runtime = Spectrum48kRuntime::new(Model::Spectrum48KPal, Spectrum48k::new());
    let snap = parse_sna(&minimal_sna_48k()).expect("parse SNA 48K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_48k_loads_z80_v1() {
    let mut runtime = Spectrum48kRuntime::new(Model::Spectrum48KPal, Spectrum48k::new());
    let snap = parse_z80(&minimal_z80_v1()).expect("parse Z80 v1");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus_loads_tap() {
    let mut runtime = SpectrumPlusRuntime::new(Model::SpectrumPlus, SpectrumPlus::new());
    load_tape_into(&mut runtime, &minimal_tap());
}

#[test]
fn spectrum_plus_loads_tzx() {
    let mut runtime = SpectrumPlusRuntime::new(Model::SpectrumPlus, SpectrumPlus::new());
    load_tape_into(&mut runtime, &minimal_tzx());
}

#[test]
fn spectrum_plus_loads_sna_48k() {
    let mut runtime = SpectrumPlusRuntime::new(Model::SpectrumPlus, SpectrumPlus::new());
    let snap = parse_sna(&minimal_sna_48k()).expect("parse SNA 48K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus_loads_z80_v1() {
    let mut runtime = SpectrumPlusRuntime::new(Model::SpectrumPlus, SpectrumPlus::new());
    let snap = parse_z80(&minimal_z80_v1()).expect("parse Z80 v1");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

// ─── 128K-class: 128K / +2 × {TAP, TZX, SNA-128K, Z80-v2} ──────────

#[test]
fn spectrum_128k_loads_tap() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    load_tape_into(&mut runtime, &minimal_tap());
}

#[test]
fn spectrum_128k_loads_tzx() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    load_tape_into(&mut runtime, &minimal_tzx());
}

#[test]
fn spectrum_128k_loads_sna_128k() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    let snap = parse_sna(&minimal_sna_128k()).expect("parse SNA 128K");
    assert_eq!(snap.model, SnapshotModel::Spectrum128K);
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_128k_loads_z80_v2() {
    let mut runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, Spectrum128K::new());
    let snap = parse_z80(&minimal_z80_v2_128k()).expect("parse Z80 v2 128K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus2_loads_tap() {
    let mut runtime = SpectrumPlus2Runtime::new(Model::SpectrumPlus2, SpectrumPlus2::new());
    load_tape_into(&mut runtime, &minimal_tap());
}

#[test]
fn spectrum_plus2_loads_tzx() {
    let mut runtime = SpectrumPlus2Runtime::new(Model::SpectrumPlus2, SpectrumPlus2::new());
    load_tape_into(&mut runtime, &minimal_tzx());
}

#[test]
fn spectrum_plus2_loads_sna_128k() {
    let mut runtime = SpectrumPlus2Runtime::new(Model::SpectrumPlus2, SpectrumPlus2::new());
    let snap = parse_sna(&minimal_sna_128k()).expect("parse SNA 128K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus2_loads_z80_v2() {
    let mut runtime = SpectrumPlus2Runtime::new(Model::SpectrumPlus2, SpectrumPlus2::new());
    let snap = parse_z80(&minimal_z80_v2_128k()).expect("parse Z80 v2 128K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

// ─── Amstrad-class: +2A / +2B / +3 × {TAP, TZX, SNA-128K, Z80-v2} ──

#[test]
fn spectrum_plus2a_loads_tap() {
    let mut runtime = SpectrumPlus2ARuntime::new(Model::SpectrumPlus2A, SpectrumPlus2A::new());
    load_tape_into(&mut runtime, &minimal_tap());
}

#[test]
fn spectrum_plus2a_loads_tzx() {
    let mut runtime = SpectrumPlus2ARuntime::new(Model::SpectrumPlus2A, SpectrumPlus2A::new());
    load_tape_into(&mut runtime, &minimal_tzx());
}

#[test]
fn spectrum_plus2a_loads_sna_128k() {
    let mut runtime = SpectrumPlus2ARuntime::new(Model::SpectrumPlus2A, SpectrumPlus2A::new());
    let snap = parse_sna(&minimal_sna_128k()).expect("parse SNA 128K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus2a_loads_z80_v2() {
    let mut runtime = SpectrumPlus2ARuntime::new(Model::SpectrumPlus2A, SpectrumPlus2A::new());
    let snap = parse_z80(&minimal_z80_v2_128k()).expect("parse Z80 v2 128K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus2b_loads_tap() {
    let mut runtime = SpectrumPlus2BRuntime::new(Model::SpectrumPlus2B, SpectrumPlus2B::new());
    load_tape_into(&mut runtime, &minimal_tap());
}

#[test]
fn spectrum_plus2b_loads_tzx() {
    let mut runtime = SpectrumPlus2BRuntime::new(Model::SpectrumPlus2B, SpectrumPlus2B::new());
    load_tape_into(&mut runtime, &minimal_tzx());
}

#[test]
fn spectrum_plus2b_loads_sna_128k() {
    let mut runtime = SpectrumPlus2BRuntime::new(Model::SpectrumPlus2B, SpectrumPlus2B::new());
    let snap = parse_sna(&minimal_sna_128k()).expect("parse SNA 128K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus2b_loads_z80_v2() {
    let mut runtime = SpectrumPlus2BRuntime::new(Model::SpectrumPlus2B, SpectrumPlus2B::new());
    let snap = parse_z80(&minimal_z80_v2_128k()).expect("parse Z80 v2 128K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus3_loads_tap() {
    let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
    load_tape_into(&mut runtime, &minimal_tap());
}

#[test]
fn spectrum_plus3_loads_tzx() {
    let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
    load_tape_into(&mut runtime, &minimal_tzx());
}

#[test]
fn spectrum_plus3_loads_sna_128k() {
    let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
    let snap = parse_sna(&minimal_sna_128k()).expect("parse SNA 128K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus3_loads_z80_v2() {
    let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
    let snap = parse_z80(&minimal_z80_v2_128k()).expect("parse Z80 v2 128K");
    SpectrumMachine::apply_snapshot(runtime.machine_mut(), &snap);
}

#[test]
fn spectrum_plus3_loads_dsk() {
    // The +3 is the only in-scope variant with a disk slot. The DSK
    // parses, reaches `fdc.insert_disk`, and `load_media` returns Ok.
    // The actual disk-load path through the +3 BIOS hangs at the
    // Loader screen — pinned at
    // `knowledge/decisions/spectrum-plus3-disk-loading-incomplete.md` —
    // so this test only asserts the format-load wiring, not boot
    // completion.
    let mut runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, SpectrumPlus3::new());
    load_disk_into(&mut runtime, &minimal_dsk());
}
