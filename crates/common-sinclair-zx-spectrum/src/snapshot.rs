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

pub use format_sinclair_zx_spectrum_z80::Z80Snapshot;
use zilog_z80::Z80;

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
pub fn apply_z80_registers(z80: &mut Z80, snap: &Z80Snapshot) {
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
pub fn apply_128k_bank_pages<M: Paged128kMemory>(snap: &Z80Snapshot, memory: &mut M) {
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
pub fn apply_ay_registers(snap: &Z80Snapshot, ay: &mut gi_ay_3_8912::Ay3_8912) {
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

    #[test]
    fn snapshot_bank_target_maps_pages_correctly() {
        assert_eq!(SnapshotBankTarget::for_page(8), Some(SnapshotBankTarget::Bank5At4000));
        assert_eq!(SnapshotBankTarget::for_page(5), Some(SnapshotBankTarget::Bank2At8000));
        assert_eq!(SnapshotBankTarget::for_page(3), Some(SnapshotBankTarget::BankedAtC000(0)));
        assert_eq!(SnapshotBankTarget::for_page(10), Some(SnapshotBankTarget::BankedAtC000(7)));
        assert_eq!(SnapshotBankTarget::for_page(2), None); // ROM page
        assert_eq!(SnapshotBankTarget::for_page(11), None); // reserved
    }

    #[test]
    fn snapshot_bank_target_bases_are_fixed() {
        assert_eq!(SnapshotBankTarget::Bank5At4000.base(), 0x4000);
        assert_eq!(SnapshotBankTarget::Bank2At8000.base(), 0x8000);
        assert_eq!(SnapshotBankTarget::BankedAtC000(4).base(), 0xC000);
    }
}
