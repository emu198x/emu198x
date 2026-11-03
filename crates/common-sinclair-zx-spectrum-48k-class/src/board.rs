//! Physical PCB and fitted ULA identity, independent of RAM population or case.
//!
//! Sinclair's Issue 3/3B, 4AB, 5 and 6A schematics and service supplement 1
//! establish these circuits. Issue 5's drawing and parts table document different
//! ULA fittings; both remain explicit. Smith, chapter 24, distinguishes the
//! early 5C fixes and 6C revisions. The current ULA library qualifies functional
//! 5C/6C behaviour, not sub-cycle DRAM margins or analog component differences.

use emu198x_ferranti_ula_6c001e::UlaRevision;

/// Implemented physical board/fitting combinations. Declaration order is saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PhysicalBoard {
    /// Issue 1, 5C102E with dead-cockroach fix, without the later A0 spider.
    Issue1,
    /// Issue 2, 5C112E with the A0 spider.
    Issue2,
    /// Issue 3 with Ferranti 6C001E-6.
    Issue3,
    /// Issue 3B with Ferranti 6C001E-6.
    Issue3B,
    /// Issue 4A with Ferranti 6C001E-7.
    Issue4A,
    /// Issue 4B with Ferranti 6C001E-7.
    Issue4B,
    /// Issue 5 fitting shown on the circuit drawing.
    Issue5Ula6c001e6,
    /// Issue 5 fitting named in the service parts list.
    Issue5Ula6c001e7,
    /// Issue 6A with Ferranti 6C001E-7; AMI SAGA is not qualified here.
    Issue6A,
}

impl PhysicalBoard {
    /// Functional ULA family used by the existing native model.
    #[must_use]
    pub const fn ula_family(self) -> UlaRevision {
        match self {
            Self::Issue1 | Self::Issue2 => UlaRevision::Ferranti5C,
            _ => UlaRevision::Ferranti6C,
        }
    }

    /// Whether the CPU A0/IORQ spider circuit is fitted.
    #[must_use]
    pub const fn has_spider(self) -> bool {
        !matches!(self, Self::Issue1)
    }

    /// Whether the board uses the ZX8401 rather than discrete memory logic.
    #[must_use]
    pub const fn has_zx8401(self) -> bool {
        matches!(
            self,
            Self::Issue5Ula6c001e6 | Self::Issue5Ula6c001e7 | Self::Issue6A
        )
    }

    /// Fitted part, distinct even where the current functional ULA model agrees.
    #[must_use]
    pub const fn ula_part(self) -> &'static str {
        match self {
            Self::Issue1 => "Ferranti 5C102E",
            Self::Issue2 => "Ferranti 5C112E",
            Self::Issue3 | Self::Issue3B | Self::Issue5Ula6c001e6 => "Ferranti 6C001E-6",
            _ => "Ferranti 6C001E-7",
        }
    }

    /// PCB issue label; two Issue 5 fittings share the PCB but not their owner.
    #[must_use]
    pub const fn issue(self) -> &'static str {
        match self {
            Self::Issue1 => "issue1",
            Self::Issue2 => "issue2",
            Self::Issue3 => "issue3",
            Self::Issue3B => "issue3b",
            Self::Issue4A => "issue4a",
            Self::Issue4B => "issue4b",
            Self::Issue5Ula6c001e6 | Self::Issue5Ula6c001e7 => "issue5",
            Self::Issue6A => "issue6a",
        }
    }

    /// Resolve an ordinary CPU memory access at the existing settled bus point.
    ///
    /// The healthy RAM is stored in CPU-visible order. Its bank bit A14 remains
    /// in that logical map; the low seven row/column address pins come from the
    /// actual mux. The display DMA pins feed CPU accesses to lower RAM; the
    /// ULA overrides that bus during its own fetches in the shared ownership
    /// path. TS1/TS2 affect only MA7 and do not select a different healthy byte
    /// in this logical storage map. No physical type-strap waveform is claimed.
    /// Each mux phase is evaluated without ticking or adding propagation time.
    pub(crate) fn cpu_memory_address(self, address: u16, write: bool) -> Option<u16> {
        if address < 0x4000 || !self.has_zx8401() {
            return Some(address);
        }
        let input = emu198x_mullard_zx8401::Inputs {
            address,
            ts1: false,
            ts2: false,
            dras_n: false,
            rd_n: write,
            wr_n: !write,
            mreq_n: false,
            mreq_delayed_n: false,
        };
        let column = emu198x_mullard_zx8401::evaluate(input);
        if address >= 0x8000 && column.cas_n {
            return None;
        }
        let row = emu198x_mullard_zx8401::evaluate(emu198x_mullard_zx8401::Inputs {
            mreq_delayed_n: true,
            dras_n: true,
            ..input
        });
        if address < 0x8000 {
            return Some(0x4000 | u16::from(row.dma) | (u16::from(column.dma) << 7));
        }
        Some(
            0x8000
                | (address & 0x4000)
                | u16::from(row.ma & 0x7f)
                | (u16::from(column.ma & 0x7f) << 7),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zx8401_mux_and_cas_preserve_every_cpu_visible_ram_cell() {
        for board in [
            PhysicalBoard::Issue5Ula6c001e6,
            PhysicalBoard::Issue5Ula6c001e7,
            PhysicalBoard::Issue6A,
        ] {
            for address in 0..=u16::MAX {
                for write in [false, true] {
                    assert_eq!(board.cpu_memory_address(address, write), Some(address));
                }
            }
        }
    }
}
