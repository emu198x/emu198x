//! The expansion port's memory-map lines, ROMDIS and MAP, as inputs a
//! peripheral drives.
//!
//! The board decodes every 6502 access in the ULA (Mike Brown, *The Oric
//! 1/Atmos Unofficial ULA Guide* v1.02, "Address Decode"). Two expansion
//! lines change that decode:
//!
//! - **MAP** is a ULA input (pin 26, active low). Asserted, it turns
//!   `$C000-$FFFF` into "a normal RAM address allowing access to the 'shadow'
//!   ram", and stops `$0000-$BFFF` being a RAM access at all, "to allow
//!   external peripherals to drive the data bus for data reads". The ULA
//!   asserts the DRAM's CAS, which performs a write, "only if this is
//!   actually a RAM write", so a write the decode does not route to RAM
//!   changes nothing.
//! - **ROMDIS** stops the internal ROM driving the bus. It is not a ULA input
//!   (Brown lists the decoder's inputs as A8-A15, the 1 MHz clock and MAP),
//!   so with MAP released the ULA still decodes `$C000-$FFFF` as ROM and
//!   makes no RAM access: the bus is left to whatever the peripheral puts on
//!   it. The Microdisc's own EPROM answers there.
//!
//! So reading the RAM under the ROM needs MAP, and a peripheral that wants
//! its own ROM in part of the top 16 KB and the shadow RAM in the rest
//! decodes the address and asserts MAP only for the RAM part. That decoding
//! belongs to the peripheral; the board only honours the levels. *The Oric-1
//! Companion* (p. 111) puts it from the user's side: the ROM "can be
//! effectively blotted out by appropriate external control signals from the
//! expansion bus".
//!
//! A `set_*` call is the peripheral's drive, `true` meaning asserted. No
//! source in the reference library gives ROMDIS's electrical polarity or
//! connector pin, so the API names the logical state rather than a level.
//! Nothing on the board drives either line, so a bare machine holds both
//! released and boots its ROM. Neither line is a register the 6502 can
//! reach; only the peripheral moves them.
//!
//! Where nothing drives the bus — ROMDIS without MAP at `$C000-$FFFF`, or MAP
//! below `$C000` — a read returns `$FF`, the same value this crate returns for
//! absent RAM. No source states what an Oric's floating bus reads.

use serde::{Deserialize, Serialize};

use super::OricAtmos;

/// The expansion port's memory-map inputs, `true` meaning asserted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ExpansionLines {
    pub(crate) romdis: bool,
    pub(crate) map: bool,
}

/// Where the board routes one 6502 access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BusTarget {
    /// The VIA's page, `$0300-$03FF`. MAP does not gate it: Brown's decoder
    /// takes I/O select low for page 3 with no MAP term.
    Via,
    /// The internal BASIC ROM.
    Rom,
    /// DRAM at the access's own address.
    Ram,
    /// Nothing on the board answers; the expansion peripheral may.
    Expansion,
}

impl ExpansionLines {
    /// Decode `addr` the way the ULA and the ROM's enable see it.
    pub(crate) fn decode(self, addr: u16) -> BusTarget {
        match addr {
            0x0300..=0x03FF => BusTarget::Via,
            0xC000..=0xFFFF if self.map => BusTarget::Ram,
            0xC000..=0xFFFF if self.romdis => BusTarget::Expansion,
            0xC000..=0xFFFF => BusTarget::Rom,
            _ if self.map => BusTarget::Expansion,
            _ => BusTarget::Ram,
        }
    }
}

impl OricAtmos {
    /// Drive the expansion port's ROMDIS line. Asserted, the internal ROM
    /// stops answering at `$C000-$FFFF`; the RAM beneath appears only with
    /// MAP asserted as well (`expansion_port.rs` gives the decode).
    pub fn set_expansion_romdis(&mut self, asserted: bool) {
        self.expansion.romdis = asserted;
    }

    /// Whether a peripheral is holding ROMDIS asserted.
    #[must_use]
    pub fn expansion_romdis(&self) -> bool {
        self.expansion.romdis
    }

    /// Drive the expansion port's MAP line into the ULA. Asserted,
    /// `$C000-$FFFF` reads and writes the RAM under the ROM, and
    /// `$0000-$BFFF` (bar the VIA's page) is left for the peripheral.
    pub fn set_expansion_map(&mut self, asserted: bool) {
        self.expansion.map = asserted;
    }

    /// Whether a peripheral is holding MAP asserted.
    #[must_use]
    pub fn expansion_map(&self) -> bool {
        self.expansion.map
    }
}

#[cfg(test)]
mod tests {
    use crate::{OricAtmos, OricModel};

    /// A 16 KB ROM whose every byte reads `$4C` apart from the vectors,
    /// which send the 6502 to `$C000` (`JMP $C000`).
    fn rom() -> Vec<u8> {
        let mut rom = vec![0x4C; 0x4000];
        rom[1] = 0x00;
        rom[2] = 0xC0;
        for vector in [0x3FFA, 0x3FFC, 0x3FFE] {
            rom[vector] = 0x00;
            rom[vector + 1] = 0xC0;
        }
        rom
    }

    /// Fill the RAM under the ROM with a pattern distinct from the ROM's.
    fn fill_shadow_ram(sys: &mut OricAtmos) {
        sys.set_expansion_map(true);
        for addr in 0xC000..=0xFFFFu16 {
            sys.mem_write(addr, (addr >> 8) as u8 ^ 0xA5);
        }
        sys.set_expansion_map(false);
    }

    const PROBES: [u16; 4] = [0xC000, 0xDFFF, 0xE000, 0xFFFF];

    fn shadow_byte(addr: u16) -> u8 {
        (addr >> 8) as u8 ^ 0xA5
    }

    #[test]
    fn with_the_lines_released_the_top_16k_reads_rom() {
        let mut sys = OricAtmos::new(rom(), OricModel::Atmos);
        fill_shadow_ram(&mut sys);
        for addr in PROBES {
            assert_eq!(sys.mem_read(addr), sys.rom[usize::from(addr - 0xC000)]);
            assert_eq!(sys.peek(addr), sys.rom[usize::from(addr - 0xC000)]);
        }
    }

    /// MAP is the ULA input that makes `$C000-$FFFF` a RAM access (Brown,
    /// "Address Decode").
    #[test]
    fn map_reads_the_ram_under_the_rom() {
        let mut sys = OricAtmos::new(rom(), OricModel::Atmos);
        fill_shadow_ram(&mut sys);
        sys.set_expansion_romdis(true);
        sys.set_expansion_map(true);
        for addr in PROBES {
            assert_eq!(sys.mem_read(addr), shadow_byte(addr), "{addr:04X}");
            assert_eq!(sys.peek(addr), shadow_byte(addr), "{addr:04X}");
        }
    }

    /// ROMDIS only silences the ROM. The ULA still decodes the top 16 KB as
    /// ROM, so no RAM cycle runs and nothing on the board drives the bus.
    #[test]
    fn romdis_alone_silences_the_rom_without_exposing_ram() {
        let mut sys = OricAtmos::new(rom(), OricModel::Atmos);
        fill_shadow_ram(&mut sys);
        sys.set_expansion_romdis(true);
        for addr in PROBES {
            let read = sys.mem_read(addr);
            assert_ne!(read, sys.rom[usize::from(addr - 0xC000)], "{addr:04X}");
            assert_ne!(read, shadow_byte(addr), "{addr:04X}");
            assert_eq!(read, 0xFF, "{addr:04X}: an undriven bus");
        }
    }

    #[test]
    fn releasing_the_lines_brings_the_rom_back() {
        let mut sys = OricAtmos::new(rom(), OricModel::Atmos);
        fill_shadow_ram(&mut sys);
        sys.set_expansion_romdis(true);
        sys.set_expansion_map(true);
        assert_eq!(sys.mem_read(0xC000), shadow_byte(0xC000));

        sys.set_expansion_map(false);
        sys.set_expansion_romdis(false);
        assert_eq!(sys.mem_read(0xC000), 0x4C);
        assert_eq!(sys.mem_read(0xFFFC), 0x00);
        assert_eq!(sys.mem_read(0xFFFD), 0xC0);
    }

    /// With MAP asserted the ULA makes no RAM access below `$C000`, leaving
    /// the bus to the peripheral; a write there is dropped. The VIA's page
    /// keeps its own select.
    #[test]
    fn map_takes_the_low_48k_off_the_ram() {
        let mut sys = OricAtmos::new(rom(), OricModel::Atmos);
        sys.mem_write(0x0400, 0x12);
        sys.mem_write(0x0303, 0x5A); // VIA DDRA

        sys.set_expansion_map(true);
        assert_eq!(sys.mem_read(0x0400), 0xFF);
        sys.mem_write(0x0400, 0x34);
        assert_eq!(sys.mem_read(0x0303), 0x5A, "the VIA still answers");

        sys.set_expansion_map(false);
        assert_eq!(
            sys.mem_read(0x0400),
            0x12,
            "the write under MAP was dropped"
        );
    }

    /// The decode is the CPU's, not only the debugger's: a 6502 reset with
    /// MAP asserted fetches its vector and code from the shadow RAM and
    /// stores into it.
    #[test]
    fn the_6502_runs_from_the_shadow_ram_under_map() {
        let mut sys = OricAtmos::new(rom(), OricModel::Atmos);
        sys.set_expansion_romdis(true);
        sys.set_expansion_map(true);
        // Reset vector -> $C100: LDA #$42 / STA $C200 / JMP $C105.
        for (addr, byte) in [
            (0xFFFC, 0x00),
            (0xFFFD, 0xC1),
            (0xC100, 0xA9),
            (0xC101, 0x42),
            (0xC102, 0x8D),
            (0xC103, 0x00),
            (0xC104, 0xC2),
            (0xC105, 0x4C),
            (0xC106, 0x05),
            (0xC107, 0xC1),
        ] {
            sys.mem_write(addr, byte);
        }
        sys.run_frame();
        assert_eq!(sys.peek(0xC200), 0x42);
        assert_eq!(sys.cpu().regs.pc & 0xFFF0, 0xC100);
    }

    #[test]
    fn the_lines_survive_a_snapshot() {
        let mut sys = OricAtmos::new(rom(), OricModel::Atmos);
        fill_shadow_ram(&mut sys);
        sys.set_expansion_romdis(true);
        sys.set_expansion_map(true);
        let bytes = postcard::to_allocvec(&sys).expect("encode snapshot");
        let mut restored: OricAtmos = postcard::from_bytes(&bytes).expect("decode snapshot");
        assert!(restored.expansion_romdis());
        assert!(restored.expansion_map());
        assert_eq!(restored.mem_read(0xC000), shadow_byte(0xC000));
    }
}
