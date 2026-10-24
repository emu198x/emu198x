//! Konami VRC2b/c and VRC4a–e (Mappers 21, 23 and 25): 8 KiB PRG
//! banking with a swappable window on the VRC4, 1 KiB CHR banking from
//! registers written in two halves, mirroring, and on the VRC4 the VRC
//! IRQ counter.
//!
//! The chips are one design wired to the cartridge in several ways. Each
//! board connects two CPU address lines to the chip's register-select
//! inputs A0 and A1, and the mapper numbers group boards that share a
//! connector, not a wiring:
//!
//! | Mapper | Submapper | Board | A0 | A1 |
//! |---|---|---|---|---|
//! | 21 | 1 | VRC4a | A1 | A2 |
//! | 21 | 2 | VRC4c | A6 | A7 |
//! | 23 | 1 | VRC4f | A0 | A1 |
//! | 23 | 2 | VRC4e | A2 | A3 |
//! | 23 | 3 | VRC2b | A0 | A1 |
//! | 25 | 1 | VRC4b | A1 | A0 |
//! | 25 | 2 | VRC4d | A3 | A2 |
//! | 25 | 3 | VRC2c | A1 | A0 |
//!
//! Without a NES 2.0 submapper both wirings a mapper number can mean are
//! ORed together, which every known game tolerates because each writes
//! only with its own lines. Mapper 23 without a submapper keeps the VRC2's
//! register map (VRC2b is the common case) but runs the IRQ and decodes
//! two mirroring bits, so VRC4e games still get their interrupts.
//!
//! The VRC2a (mapper 22) has its own module, [`super::vrc2a`], since its
//! CHR registers drop their low bit; it shares the address decoding and
//! CHR register layout from here.
//!
//! Facts from the NESdev wiki "VRC2 and VRC4" page and Mesen2's
//! `VRC2_4.h` (GPL-3, read for behaviour only).

use serde::{Deserialize, Serialize};

use super::vrc_irq::VrcIrq;
use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// Which CPU address bits drive the chip's A0 and A1 inputs.
pub(crate) type Lines = (u8, u8);

/// Decode a register address for a board whose A0/A1 come from `lines`:
/// the page stays, and the low two bits become A1:A0. Several wirings are
/// ORed, which is how an unknown board is decoded.
pub(crate) fn register(addr: u16, lines: &[Lines]) -> u16 {
    let mut a0 = 0;
    let mut a1 = 0;
    for &(bit0, bit1) in lines {
        a0 |= (addr >> bit0) & 1;
        a1 |= (addr >> bit1) & 1;
    }
    (addr & 0xF000) | (a1 << 1) | a0
}

/// The CHR register a decoded `$B000–$EFFF` write reaches: its slot, and
/// whether it is the high half (A0 set) or the low four bits.
pub(crate) fn chr_register(reg: u16) -> Option<(usize, bool)> {
    let page = usize::from(reg >> 12);
    if !(0xB..=0xE).contains(&page) {
        return None;
    }
    Some(((page - 0xB) * 2 + usize::from((reg >> 1) & 1), reg & 1 != 0))
}

/// Konami VRC2b/c and VRC4, mappers 21, 23 and 25.
#[derive(Clone, Serialize, Deserialize)]
pub struct Vrc4 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: Vec<u8>,
    lines: Vec<Lines>,
    /// The VRC4's register map: PRG swap mode at `$9002` and two
    /// mirroring bits. Off for a VRC2.
    vrc4: bool,
    /// The IRQ counter runs (VRC4, or a mapper 23 board of unknown kind).
    irq_present: bool,
    /// Mask on the mirroring value: one bit on a known VRC2.
    mirroring_mask: u8,
    /// A known VRC2 without PRG RAM has a one-bit latch at `$6000`.
    latch_only: bool,
    latch: u8,
    prg_regs: [u8; 2],
    prg_swap: bool,
    chr_regs: [u16; 8],
    mirroring: Mirroring,
    irq: VrcIrq,
}

impl Vrc4 {
    /// Construct the board for an iNES mapper number (21, 23 or 25) and
    /// NES 2.0 submapper (0 when unknown).
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mapper: u16, submapper: u8) -> Self {
        // (wirings, VRC4 register map)
        let (lines, vrc4): (Vec<Lines>, bool) = match (mapper, submapper) {
            (21, 1) => (vec![(1, 2)], true),
            (21, 2) => (vec![(6, 7)], true),
            (21, _) => (vec![(1, 2), (6, 7)], true),
            (23, 1) => (vec![(0, 1)], true),
            (23, 2) => (vec![(2, 3)], true),
            (23, 3) => (vec![(0, 1)], false),
            (23, _) => (vec![(0, 1), (2, 3)], false),
            (25, 1) => (vec![(1, 0)], true),
            (25, 2) => (vec![(3, 2)], true),
            (25, 3) => (vec![(1, 0)], false),
            (_, _) => (vec![(1, 0), (3, 2)], true),
        };
        let known_vrc2 = !vrc4 && submapper == 3;
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram { vec![0; 8192] } else { chr_data };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: if known_vrc2 {
                Vec::new()
            } else {
                vec![0; 8192]
            },
            lines,
            vrc4,
            irq_present: !known_vrc2,
            mirroring_mask: if known_vrc2 { 0x01 } else { 0x03 },
            latch_only: known_vrc2,
            latch: 0,
            prg_regs: [0, 1],
            prg_swap: false,
            chr_regs: [0; 8],
            mirroring: Mirroring::Vertical,
            irq: VrcIrq::default(),
        }
    }

    fn prg_8k_count(&self) -> usize {
        (self.prg_rom.len() / 8192).max(1)
    }

    fn read_prg_8k(&self, bank: usize, offset: u16) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + usize::from(offset & 0x1FFF)]
    }

    fn prg_bank(&self, addr: u16) -> usize {
        let second_last = self.prg_8k_count().saturating_sub(2);
        match (addr >> 13) & 3 {
            0 if self.prg_swap => second_last,
            0 => usize::from(self.prg_regs[0]),
            1 => usize::from(self.prg_regs[1]),
            2 if self.prg_swap => usize::from(self.prg_regs[0]),
            2 => second_last,
            _ => self.prg_8k_count() - 1,
        }
    }

    fn chr_index(&self, addr: u16) -> usize {
        let slot = usize::from((addr & 0x1FFF) / 0x0400);
        (usize::from(self.chr_regs[slot]) * 1024 + usize::from(addr & 0x03FF)) % self.chr.len()
    }
}

impl Mapper for Vrc4 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x6FFF if self.latch_only => (addr >> 8) as u8 & 0xFE | self.latch,
            0x6000..=0x7FFF if !self.prg_ram.is_empty() => {
                self.prg_ram[usize::from(addr & 0x1FFF) % self.prg_ram.len()]
            }
            0x8000..=0xFFFF => self.read_prg_8k(self.prg_bank(addr), addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            if self.latch_only {
                self.latch = value & 1;
            } else if !self.prg_ram.is_empty() {
                let len = self.prg_ram.len();
                self.prg_ram[usize::from(addr & 0x1FFF) % len] = value;
            }
            return;
        }
        if addr < 0x8000 {
            return;
        }
        let reg = register(addr, &self.lines);
        match reg {
            0x8000..=0x8003 => self.prg_regs[0] = value & 0x1F,
            0x9002 | 0x9003 if self.vrc4 => self.prg_swap = value & 0x02 != 0,
            0x9000..=0x9003 => {
                self.mirroring = match value & self.mirroring_mask {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    _ => Mirroring::SingleScreenUpper,
                };
            }
            0xA000..=0xA003 => self.prg_regs[1] = value & 0x1F,
            0xB000..=0xEFFF => {
                if let Some((slot, high)) = chr_register(reg) {
                    let old = self.chr_regs[slot];
                    self.chr_regs[slot] = if high {
                        (old & 0x0F) | (u16::from(value & 0x1F) << 4)
                    } else {
                        (old & 0x1F0) | u16::from(value & 0x0F)
                    };
                }
            }
            0xF000 => self.irq.set_reload_nibble(value, false),
            0xF001 => self.irq.set_reload_nibble(value, true),
            0xF002 => self.irq.set_control(value),
            0xF003 => self.irq.acknowledge(),
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[self.chr_index(addr)]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            let index = self.chr_index(addr);
            self.chr[index] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq.pending()
    }

    fn cpu_tick(&mut self) {
        if self.irq_present {
            self.irq.cpu_tick();
        }
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Vrc4(Box::new(self.clone()))
    }

    fn save_ram(&self) -> &[u8] {
        &self.prg_ram
    }

    fn restore_save_ram(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.prg_ram.len());
        self.prg_ram[..n].copy_from_slice(&bytes[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(mapper: u16, submapper: u8) -> Vrc4 {
        let prg: Vec<u8> = (0..32 * 8192).map(|i| (i / 8192) as u8).collect();
        let chr: Vec<u8> = (0..512 * 1024).map(|i| (i / 1024) as u8).collect();
        Vrc4::new(prg, chr, mapper, submapper)
    }

    /// The CPU address that reaches register `reg` (A1:A0 in its low
    /// bits) through one wiring.
    fn wired(reg: u16, (bit0, bit1): Lines) -> u16 {
        (reg & 0xF000) | ((reg & 1) << bit0) | (((reg >> 1) & 1) << bit1)
    }

    fn chr_bank(v: &mut Vrc4, slot: u16) -> usize {
        // The CHR fixture stores the low byte of the bank number.
        let lo = usize::from(v.chr_read(slot * 0x400));
        let full = usize::from(v.chr_regs[usize::from(slot)]);
        assert_eq!(full & 0xFF, lo);
        full
    }

    #[test]
    fn every_wiring_reaches_the_same_registers() {
        let wirings: [(u16, u8, Lines); 8] = [
            (21, 1, (1, 2)),
            (21, 2, (6, 7)),
            (23, 1, (0, 1)),
            (23, 2, (2, 3)),
            (25, 1, (1, 0)),
            (25, 2, (3, 2)),
            (21, 0, (6, 7)),
            (25, 0, (3, 2)),
        ];
        for (mapper, sub, lines) in wirings {
            let mut v = board(mapper, sub);
            v.cpu_write(wired(0x8000, lines), 5);
            v.cpu_write(wired(0xA000, lines), 6);
            assert_eq!(v.cpu_read(0x8000), 5, "{mapper}.{sub}");
            assert_eq!(v.cpu_read(0xA000), 6, "{mapper}.{sub}");
            assert_eq!(v.cpu_read(0xC000), 30, "{mapper}.{sub}");
            // The high CHR half at A0 set, then the low half.
            v.cpu_write(wired(0xD003, lines), 0x1A);
            v.cpu_write(wired(0xD002, lines), 0x07);
            assert_eq!(chr_bank(&mut v, 5), 0x1A7, "{mapper}.{sub}: 9-bit CHR bank");
            v.cpu_write(wired(0x9000, lines), 3);
            assert_eq!(
                v.mirroring(),
                Mirroring::SingleScreenUpper,
                "{mapper}.{sub}"
            );
        }
    }

    #[test]
    fn the_vrc4_swap_mode_moves_the_switchable_window_to_c000() {
        let mut v = board(21, 1);
        v.cpu_write(0x8000, 4);
        v.cpu_write(0x9004, 0x02); // $9002 on VRC4a: A1 = A2
        assert_eq!(v.cpu_read(0x8000), 30, "second-last bank at $8000");
        assert_eq!(v.cpu_read(0xC000), 4);
        assert_eq!(v.cpu_read(0xE000), 31);
        v.cpu_write(0x9004, 0);
        assert_eq!(v.cpu_read(0x8000), 4);
    }

    #[test]
    fn the_irq_reload_is_written_in_nibbles() {
        let mut v = board(25, 1);
        v.cpu_write(0xF000, 0x0E); // low nibble
        v.cpu_write(0xF002, 0x0F); // $F001 on VRC4b: A0 = A1, high nibble
        v.cpu_write(0xF001, 0x06); // $F002: enable, cycle mode
        v.cpu_tick();
        assert!(!v.irq_pending(), "$FE -> $FF");
        v.cpu_tick();
        assert!(v.irq_pending(), "$FF wraps");
        v.cpu_write(0xF003, 0); // acknowledge
        assert!(!v.irq_pending());
    }

    #[test]
    fn a_known_vrc2_has_one_mirroring_bit_a_latch_and_no_irq() {
        let mut v = board(23, 3);
        v.cpu_write(0x9000, 3);
        assert_eq!(v.mirroring(), Mirroring::Horizontal);
        v.cpu_write(0x9002, 0x02);
        assert_eq!(v.cpu_read(0x8000), 0, "no swap mode on a VRC2");
        v.cpu_write(0x6000, 1);
        assert_eq!(v.cpu_read(0x6000) & 1, 1, "microwire latch");
        assert!(v.save_ram().is_empty());
        v.cpu_write(0xF000, 0x0F);
        v.cpu_write(0xF001, 0x0F);
        v.cpu_write(0xF002, 0x06); // would enable in cycle mode
        for _ in 0..1000 {
            v.cpu_tick();
        }
        assert!(!v.irq_pending());
    }

    #[test]
    fn an_unknown_mapper_23_board_keeps_vrc2_registers_but_runs_the_irq() {
        let mut v = board(23, 0);
        v.cpu_write(0x6000, 0x42);
        assert_eq!(v.cpu_read(0x6000), 0x42, "PRG RAM");
        v.cpu_write(0xF000, 0x0F); // VRC4e reload low via A2/A3 = $F000
        v.cpu_write(0xF004, 0x0F); // $F001 via A2
        v.cpu_write(0xF008, 0x06); // $F002 via A3
        v.cpu_tick();
        v.cpu_tick();
        assert!(v.irq_pending());
        v.cpu_write(0x9008, 0x03); // $9002 is mirroring on the VRC2 map
        assert_eq!(v.mirroring(), Mirroring::SingleScreenUpper);
    }

    #[test]
    fn register_decoding_ors_wirings_and_finds_chr_slots() {
        assert_eq!(register(0x9004, &[(2, 3)]), 0x9001);
        assert_eq!(register(0x9080, &[(1, 2), (6, 7)]), 0x9002);
        assert_eq!(chr_register(0xB000), Some((0, false)));
        assert_eq!(chr_register(0xE003), Some((7, true)));
        assert_eq!(chr_register(0xA000), None);
    }
}
