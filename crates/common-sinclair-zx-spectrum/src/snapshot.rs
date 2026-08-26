//! Shared `.z80` snapshot application helpers.
//!
//! Every 128K-class Spectrum machine (128K, +2A/+3, Pentagon, Scorpion)
//! has a near-identical `apply_snapshot` body:
//!
//! 1. Copy the saved Z80 register file onto the CPU.
//! 2. Replay the border write.
//! 3. For each `(page, data)` tuple in `snap.pages`, bank the right RAM
//!    page into `$C000` and copy its 16 KB through the machine's normal
//!    memory bus (so ROM writes are silently ignored and banks 5/2 land
//!    at their fixed slots).
//! 4. Commit the final `$7FFD` paging state (and `$1FFD` on +2A/+3).
//! 5. Replay the AY-3-8912 register file and leave the selected register
//!    at whatever the snapshot saved.
//!
//! Before this module, each machine hand-rolled its own ~50-line copy.
//! These helpers keep the variation (which paging ports exist, which
//! ULA receives the border, which RAM bank 5/2 live in) at the call
//! site and share everything else.
//!
//! The helpers take closures rather than a trait so machines don't have
//! to plumb a new trait impl for every variant. The pattern is:
//!
//! ```ignore
//! use common_sinclair_zx_spectrum::snapshot::{
//!     apply_z80_registers, apply_128k_bank_pages, apply_ay_registers,
//! };
//!
//! apply_z80_registers(&mut self.z80, snap);
//! self.ula.write_fe(snap.border);
//! apply_128k_bank_pages(
//!     snap,
//!     |v| self.memory.write_7ffd(v),
//!     |a, v| self.memory.write(a, v),
//! );
//! self.memory.write_7ffd(snap.port_7ffd);
//! apply_ay_registers(
//!     snap,
//!     |r| self.ay.select_register(r),
//!     |v| self.ay.write_data(v),
//! );
//! self.ay.select_register(snap.ay_register);
//! ```

use emu198x_zilog_z80::Z80;
pub use format_sinclair_zx_spectrum_snapshot::{Snapshot, SnapshotModel};

use crate::memory::MemoryBus;

/// Minimal trait a 128K-family memory map exposes to snapshot loaders.
/// Machines that page RAM at `$C000` via port `$7FFD` implement it by
/// forwarding to their own inherent `write_7ffd` method.
pub trait Paged128kMemory: MemoryBus {
    fn write_7ffd(&mut self, val: u8);
}

/// Copy the register file from a `.z80` snapshot onto a Z80.
///
/// Handles every register the snapshot format stores: AF/BC/DE/HL and
/// their primes, IX/IY, SP, PC, I, R, IM, IFF1, IFF2. Does not touch
/// memory or peripherals.
pub fn apply_z80_registers(z80: &mut Z80, snap: &Snapshot) {
    z80.regs.af = snap.af;
    z80.regs.bc = snap.bc;
    z80.regs.de = snap.de;
    z80.regs.hl = snap.hl;
    z80.regs.af_alt = snap.af_alt;
    z80.regs.bc_alt = snap.bc_alt;
    z80.regs.de_alt = snap.de_alt;
    z80.regs.hl_alt = snap.hl_alt;
    z80.regs.ix = snap.ix;
    z80.regs.iy = snap.iy;
    z80.regs.sp = snap.sp;
    z80.regs.pc = snap.pc;
    z80.regs.i = snap.i;
    z80.regs.r = snap.r;
    z80.regs.im = snap.im;
    z80.regs.iff1 = snap.iff1;
    z80.regs.iff2 = snap.iff2;
}

/// Which memory slot a `.z80` snapshot's page (3..=10) belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotBankTarget {
    /// Page 8 on 48K / 128K / Pentagon / Scorpion / Plus — bank 5 sits
    /// permanently at `$4000`.
    Bank5At4000,
    /// Page 5 on 128K / Plus — bank 2 sits at `$8000`.
    Bank2At8000,
    /// Page 3, 4, 6, 7, 9, 10 — banks 0, 1, 3, 4, 6, 7 respectively.
    /// Caller pages the bank in by writing to `$7FFD` and then copies
    /// the 16 KB through the normal bus at `$C000`.
    BankedAtC000(u8),
}

impl SnapshotBankTarget {
    /// Classify a snapshot page into its target slot.
    ///
    /// Returns `None` for pages that aren't RAM (pages 0..=2 are ROM,
    /// pages 11+ are reserved).
    #[must_use]
    pub fn for_page(page: u8) -> Option<Self> {
        let bank = page.checked_sub(3)?;
        if bank > 7 {
            return None;
        }
        match bank {
            5 => Some(Self::Bank5At4000),
            2 => Some(Self::Bank2At8000),
            _ => Some(Self::BankedAtC000(bank)),
        }
    }

    /// The `$C000`-relative base address this target lives at.
    #[must_use]
    pub const fn base(self) -> u16 {
        match self {
            Self::Bank5At4000 => 0x4000,
            Self::Bank2At8000 => 0x8000,
            Self::BankedAtC000(_) => 0xC000,
        }
    }
}

/// Apply the 48K-mode page layout from a `.sna` or `.z80` snapshot.
///
/// The .z80 v2/v3 spec maps 48K-mode pages by *region*, not by bank
/// number: page 8 → `$4000`, page 4 → `$8000`, page 5 → `$C000`. This
/// is distinct from the bank-numbering convention used by 128K-mode
/// snapshots (where page 4 is bank 1 and page 5 is bank 2). The `.sna`
/// 48K parser produces the same `(8, 4, 5)` triple.
///
/// Used by 16K / 48K / Spectrum+ runtimes whose memory map exposes the
/// three RAM regions at their fixed CPU-visible addresses. The
/// `MemoryBus::write` path is the same code the CPU uses, so ROM
/// region writes are silently dropped.
pub fn apply_48k_pages<M: MemoryBus>(snap: &Snapshot, memory: &mut M) {
    for (page, data) in &snap.pages {
        let Some(base) = page_48k_base(*page) else {
            continue;
        };
        for (i, &byte) in data.iter().enumerate() {
            memory.write(base.wrapping_add(i as u16), byte);
        }
    }
}

const fn page_48k_base(page: u8) -> Option<u16> {
    match page {
        8 => Some(0x4000),
        4 => Some(0x8000),
        5 => Some(0xC000),
        _ => None,
    }
}

/// Apply the 128K-family `$7FFD` page layout from a `.z80` snapshot.
///
/// Takes `&mut M` directly (rather than closures) so the caller only
/// holds one live mutable borrow of the memory struct at a time — the
/// borrow checker rejects the two-closure variant because both closures
/// would capture `&mut self.memory` for the duration of the call.
///
/// After this call the snapshot's per-page data is in RAM but the final
/// `$7FFD` state has been overwritten as a side effect — the caller
/// must then write `snap.port_7ffd` to restore it.
///
/// Used by: 128K, +2A/+3, Pentagon, Scorpion.
pub fn apply_128k_bank_pages<M: Paged128kMemory>(snap: &Snapshot, memory: &mut M) {
    for (page, data) in &snap.pages {
        let Some(target) = SnapshotBankTarget::for_page(*page) else {
            continue;
        };
        if let SnapshotBankTarget::BankedAtC000(bank) = target {
            memory.write_7ffd(bank);
        }
        let base = target.base();
        for (i, &byte) in data.iter().enumerate() {
            memory.write(base.wrapping_add(i as u16), byte);
        }
    }
}

/// Replay an AY-3-8912 register file from a `.z80` snapshot.
///
/// Takes `&mut Ay3_8912` directly for the same borrow-checker reason
/// as `apply_128k_bank_pages`. Writes all 16 registers in index order
/// and leaves the AY with `snap.ay_register` selected so subsequent
/// `IN`/`OUT` on the data port target the same register the snapshot
/// captured.
pub fn apply_ay_registers(snap: &Snapshot, ay: &mut gi_ay_3_8912::Ay3_8912) {
    for (reg, &val) in snap.ay_regs.iter().enumerate() {
        ay.select_register(reg as u8);
        ay.write_data(val);
    }
    ay.select_register(snap.ay_register);
}

// Deliberately no single-shot `apply_z80_snapshot(&mut self, snap)` —
// the borrow checker won't let a single helper hold simultaneous
// `&mut` closures into disjoint fields of the same machine. Callers
// sequence the three sub-helpers above instead, which gives a clear
// per-step borrow pattern:
//
// ```ignore
// apply_z80_registers(&mut self.z80, snap);
// self.ula.write_fe(snap.border);
// apply_128k_bank_pages(snap, |v| self.memory.write_7ffd(v), |a, v| self.memory.write(a, v));
// self.memory.write_7ffd(snap.port_7ffd);
// // +2A/+3 adds `self.memory.write_1ffd(snap.port_1ffd);` here.
// apply_ay_registers(snap, |r| self.ay.select_register(r), |v| self.ay.write_data(v));
// self.ay.select_register(snap.ay_register);
// ```

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory `Paged128kMemory` stub that records every write and the
    /// `$7FFD` paging history. Backing array is one byte per address.
    struct StubMemory {
        ram: Vec<u8>,
        paged: Vec<u8>,
    }

    impl StubMemory {
        fn new() -> Self {
            Self {
                ram: vec![0u8; 0x10000],
                paged: Vec::new(),
            }
        }
    }

    impl MemoryBus for StubMemory {
        fn read(&self, addr: u16) -> u8 {
            self.ram[addr as usize]
        }
        fn write(&mut self, addr: u16, value: u8) {
            self.ram[addr as usize] = value;
        }
        fn is_contended(&self, _addr: u16) -> bool {
            false
        }
    }

    impl Paged128kMemory for StubMemory {
        fn write_7ffd(&mut self, val: u8) {
            self.paged.push(val);
        }
    }

    /// Build a fully-populated `Snapshot` for register-restore tests.
    fn populated_snapshot() -> Snapshot {
        Snapshot {
            af: 0x1122,
            bc: 0x3344,
            de: 0x5566,
            hl: 0x7788,
            af_alt: 0x99AA,
            bc_alt: 0xBBCC,
            de_alt: 0xDDEE,
            hl_alt: 0xFF00,
            ix: 0xCAFE,
            iy: 0xBEEF,
            sp: 0xFEDC,
            pc: 0x1234,
            i: 0x3F,
            r: 0x80,
            im: 2,
            iff1: true,
            iff2: false,
            border: 5,
            model: SnapshotModel::Spectrum128K,
            port_7ffd: 0x10,
            port_1ffd: 0x04,
            ay_register: 0x07,
            ay_regs: [0; 16],
            pages: Vec::new(),
        }
    }

    #[test]
    fn snapshot_bank_target_maps_pages_correctly() {
        assert_eq!(
            SnapshotBankTarget::for_page(8),
            Some(SnapshotBankTarget::Bank5At4000)
        );
        assert_eq!(
            SnapshotBankTarget::for_page(5),
            Some(SnapshotBankTarget::Bank2At8000)
        );
        assert_eq!(
            SnapshotBankTarget::for_page(3),
            Some(SnapshotBankTarget::BankedAtC000(0))
        );
        assert_eq!(
            SnapshotBankTarget::for_page(10),
            Some(SnapshotBankTarget::BankedAtC000(7))
        );
        assert_eq!(SnapshotBankTarget::for_page(2), None); // ROM page
        assert_eq!(SnapshotBankTarget::for_page(11), None); // reserved
    }

    #[test]
    fn snapshot_bank_target_bases_are_fixed() {
        assert_eq!(SnapshotBankTarget::Bank5At4000.base(), 0x4000);
        assert_eq!(SnapshotBankTarget::Bank2At8000.base(), 0x8000);
        assert_eq!(SnapshotBankTarget::BankedAtC000(4).base(), 0xC000);
    }

    #[test]
    fn snapshot_bank_target_for_page_zero_is_rom() {
        // Page 0 corresponds to bank checked_sub(3) underflow → None.
        assert_eq!(SnapshotBankTarget::for_page(0), None);
        assert_eq!(SnapshotBankTarget::for_page(1), None);
    }

    #[test]
    fn snapshot_bank_target_for_page_above_10_is_none() {
        // Page 11 already covered by the existing test, but lock down the
        // edge with the explicit "bank > 7" comparison too.
        assert_eq!(SnapshotBankTarget::for_page(11), None);
        assert_eq!(SnapshotBankTarget::for_page(255), None);
    }

    #[test]
    fn apply_z80_registers_copies_every_field() {
        let snap = populated_snapshot();
        let mut z80 = Z80::default();
        // Pre-populate with sentinels so we can confirm overwrite.
        z80.regs.af = 0x0000;
        z80.regs.pc = 0x0000;
        z80.regs.sp = 0x0000;

        apply_z80_registers(&mut z80, &snap);

        assert_eq!(z80.regs.af, 0x1122);
        assert_eq!(z80.regs.bc, 0x3344);
        assert_eq!(z80.regs.de, 0x5566);
        assert_eq!(z80.regs.hl, 0x7788);
        assert_eq!(z80.regs.af_alt, 0x99AA);
        assert_eq!(z80.regs.bc_alt, 0xBBCC);
        assert_eq!(z80.regs.de_alt, 0xDDEE);
        assert_eq!(z80.regs.hl_alt, 0xFF00);
        assert_eq!(z80.regs.ix, 0xCAFE);
        assert_eq!(z80.regs.iy, 0xBEEF);
        assert_eq!(z80.regs.sp, 0xFEDC);
        assert_eq!(z80.regs.pc, 0x1234);
        assert_eq!(z80.regs.i, 0x3F);
        assert_eq!(z80.regs.r, 0x80);
        assert_eq!(z80.regs.im, 2);
        assert!(z80.regs.iff1);
        assert!(!z80.regs.iff2);
    }

    #[test]
    fn apply_128k_bank_pages_skips_rom_and_reserved_pages() {
        let mut snap = populated_snapshot();
        // Page 0 (ROM) and page 11 (reserved) — both should be ignored.
        snap.pages.push((0, vec![0xAA; 16384]));
        snap.pages.push((11, vec![0xBB; 16384]));
        let mut mem = StubMemory::new();
        apply_128k_bank_pages(&snap, &mut mem);
        // No writes performed (RAM stays zero), no 7FFD reconfiguration.
        assert!(mem.ram.iter().all(|&b| b == 0));
        assert!(mem.paged.is_empty());
    }

    #[test]
    fn apply_128k_bank_pages_routes_bank5_and_bank2_to_fixed_slots() {
        let mut snap = populated_snapshot();
        let mut bank5 = vec![0u8; 16384];
        bank5[0] = 0x55;
        bank5[16383] = 0x5F;
        let mut bank2 = vec![0u8; 16384];
        bank2[0] = 0x22;
        bank2[16383] = 0x2F;
        snap.pages.push((8, bank5)); // bank 5 → $4000
        snap.pages.push((5, bank2)); // bank 2 → $8000

        let mut mem = StubMemory::new();
        apply_128k_bank_pages(&snap, &mut mem);

        // Fixed-slot pages do NOT page through $7FFD.
        assert!(mem.paged.is_empty());
        assert_eq!(mem.ram[0x4000], 0x55);
        assert_eq!(mem.ram[0x7FFF], 0x5F);
        assert_eq!(mem.ram[0x8000], 0x22);
        assert_eq!(mem.ram[0xBFFF], 0x2F);
    }

    #[test]
    fn apply_128k_bank_pages_pages_banked_targets_through_7ffd() {
        let mut snap = populated_snapshot();
        // Page 3 == bank 0; page 10 == bank 7.
        let mut b0 = vec![0u8; 16384];
        b0[0] = 0xB0;
        let mut b7 = vec![0u8; 16384];
        b7[0] = 0xB7;
        snap.pages.push((3, b0));
        snap.pages.push((10, b7));

        let mut mem = StubMemory::new();
        apply_128k_bank_pages(&snap, &mut mem);

        // Each banked page issues exactly one $7FFD write before copy.
        assert_eq!(mem.paged, vec![0, 7]);
        // Both copies land at $C000 (the same window).
        assert_eq!(mem.ram[0xC000], 0xB7); // last write wins
    }

    #[test]
    fn apply_ay_registers_replays_full_register_file_then_selects() {
        // Build a stand-in AY chip and feed it a known register file. Use
        // values that survive AY's per-register write masks (4-bit safe).
        // R7 is the mixer/IO-direction byte — set bits 6 and 7 high so
        // both IO ports are in output mode, otherwise `read_data` on
        // R14 / R15 returns the input-pin mask (0xFF by default) instead
        // of the stored register value. See `gi-ay-3-8912::Ay3_8912::
        // read_data` — the port read path returns the pin state, not
        // the stored byte, mirroring real silicon behaviour.
        let mut snap = populated_snapshot();
        for (i, b) in snap.ay_regs.iter_mut().enumerate() {
            *b = i as u8; // 0..15, all within every register's mask width
        }
        snap.ay_regs[7] |= 0xC0; // bits 6 + 7 = port A/B output
        snap.ay_register = 0x0B;

        let mut ay = gi_ay_3_8912::Ay3_8912::new(1_773_400, 44_100, 882);
        apply_ay_registers(&snap, &mut ay);

        for i in 0..16u8 {
            ay.select_register(i);
            // R7 was forced to 0xC7 above (mixer bits + IO direction
            // bits). Other registers carry their literal index.
            let expected = if i == 7 { 0xC7 } else { i };
            assert_eq!(ay.read_data(), expected, "register {i} round-trip",);
        }
        // After helper completes, snap.ay_register stays selected.
        ay.select_register(snap.ay_register);
        assert_eq!(ay.read_data(), 0x0B);
    }
}
