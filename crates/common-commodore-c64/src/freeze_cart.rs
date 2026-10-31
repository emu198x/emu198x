//! Freeze-button cartridges (Action Replay, Final Cartridge III).
//!
//! These carts share a shape the plain [`crate::memory`] `Cartridge` does not
//! model: a freeze button wired to `/NMI`, onboard RAM, and their own control
//! registers. [`FreezeCart`] is the single device slot the machine wires in —
//! it forwards the common surface (line state, ROM/RAM windows, I/O registers,
//! the freeze button) to the specific cartridge.

use serde::{Deserialize, Serialize};

use crate::action_replay::ActionReplay;
use crate::final_cartridge3::FinalCartridge3;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum FreezeCart {
    ActionReplay(ActionReplay),
    FinalCartridge3(FinalCartridge3),
}

impl FreezeCart {
    /// `(exrom_asserted, game_asserted)` for the current state.
    #[must_use]
    pub(crate) fn lines(&self) -> (bool, bool) {
        match self {
            Self::ActionReplay(c) => c.lines(),
            Self::FinalCartridge3(c) => c.lines(),
        }
    }

    /// Ultimax mode: GAME asserted, EXROM not.
    #[must_use]
    pub(crate) fn ultimax(&self) -> bool {
        match self {
            Self::ActionReplay(c) => c.ultimax(),
            Self::FinalCartridge3(c) => c.ultimax(),
        }
    }

    /// Whether the freeze NMI is currently asserted.
    #[must_use]
    pub(crate) fn nmi_asserted(&self) -> bool {
        match self {
            Self::ActionReplay(c) => c.nmi_asserted(),
            Self::FinalCartridge3(c) => c.nmi_asserted(),
        }
    }

    /// Press the freeze button.
    pub(crate) fn freeze(&mut self) {
        match self {
            Self::ActionReplay(c) => c.freeze(),
            Self::FinalCartridge3(c) => c.freeze(),
        }
    }

    pub(crate) fn roml_read(&self, addr: u16) -> u8 {
        match self {
            Self::ActionReplay(c) => c.roml_read(addr),
            Self::FinalCartridge3(c) => c.roml_read(addr),
        }
    }

    pub(crate) fn roml_store(&mut self, addr: u16, value: u8) {
        match self {
            Self::ActionReplay(c) => c.roml_store(addr, value),
            // Final Cartridge III has no onboard RAM — ROML is read-only.
            Self::FinalCartridge3(_) => {}
        }
    }

    pub(crate) fn romh_read(&self, addr: u16) -> u8 {
        match self {
            Self::ActionReplay(c) => c.romh_read(addr),
            Self::FinalCartridge3(c) => c.romh_read(addr),
        }
    }

    #[must_use]
    pub(crate) fn romh_peek(&self, addr: u16) -> u8 {
        match self {
            Self::ActionReplay(c) => c.romh_peek(addr),
            Self::FinalCartridge3(c) => c.romh_peek(addr),
        }
    }

    #[must_use]
    pub(crate) fn io1_read(&self, addr: u16) -> u8 {
        match self {
            Self::ActionReplay(c) => c.io1_read(),
            Self::FinalCartridge3(c) => c.io1_read(addr),
        }
    }

    pub(crate) fn io1_write(&mut self, value: u8) {
        match self {
            Self::ActionReplay(c) => c.io1_write(value),
            // FC3's only register is at $DFFF (I/O-2); I/O-1 is read-only ROM.
            Self::FinalCartridge3(_) => {}
        }
    }

    #[must_use]
    pub(crate) fn io2_read(&self, addr: u16) -> u8 {
        match self {
            Self::ActionReplay(c) => c.io2_read(addr),
            Self::FinalCartridge3(c) => c.io2_read(addr),
        }
    }

    pub(crate) fn io2_write(&mut self, addr: u16, value: u8) {
        match self {
            Self::ActionReplay(c) => c.io2_write(addr, value),
            Self::FinalCartridge3(c) => c.io2_write(addr, value),
        }
    }

    /// `(register, bank)` for debug surfaces.
    #[must_use]
    pub(crate) fn registers(&self) -> (u8, u8) {
        match self {
            Self::ActionReplay(c) => c.registers(),
            Self::FinalCartridge3(c) => c.registers(),
        }
    }
}
