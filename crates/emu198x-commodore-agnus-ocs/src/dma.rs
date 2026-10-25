//! Saved DMA stages owned by Agnus's single slot authority.
//!
//! This module retains decisions; it does not decode beam positions or choose
//! priorities. Display requests cross reservation, addressing and service.
//! Other admitted requests enter addressing directly. RAM is accessed by the
//! machine only when the resulting descriptor reaches service.

use serde::{Deserialize, Serialize};

use crate::BlitterDmaOp;

/// Timing register driven by Agnus over the same RGA service as other DMA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DmaStrobe {
    Equalisation,
    VerticalBlank,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayDmaChannel {
    Bitplane(u8),
    Sprite {
        channel: u8,
        second_word: bool,
        control: bool,
    },
}

/// Display identity and sequencer mode at reservation. Actual width/lanes
/// follow the immediate FMODE mirror at service, including intervening writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayDmaReservation {
    pub channel: DisplayDmaChannel,
    pub width_words: u8,
    pub fmode: u16,
    /// This request belongs to the terminal sequence; sample MOD in addressing.
    pub add_modulo: bool,
}

impl DisplayDmaReservation {
    #[must_use]
    pub const fn rga_register(self) -> u16 {
        match self.channel {
            DisplayDmaChannel::Bitplane(plane) => 0x110 + 2 * plane as u16,
            DisplayDmaChannel::Sprite {
                channel,
                second_word,
                control,
            } => {
                0x140
                    + 8 * channel as u16
                    + if control { 0 } else { 4 }
                    + if second_word { 2 } else { 0 }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DmaTransferTarget {
    Display {
        reservation: DisplayDmaReservation,
        /// Signed MOD sampled at addressing; service adds its actual width.
        pointer_modulo: i32,
    },
    /// A display reservation and fixed refresh drove the same RGA cell.
    /// Its register is the wired AND. Bitplanes are still unaddressed when
    /// fixed DMAL arrives; sprites already retain their reservation pointer.
    DisplayRefresh {
        reservation: DisplayDmaReservation,
        fixed_register: u16,
    },
    Copper {
        instruction_word: u8,
    },
    Blitter {
        operation: BlitterDmaOp,
        write_value: Option<u16>,
    },
    /// Logical internal phase carried over RGA without a memory transaction.
    BlitterInternal {
        operation: BlitterDmaOp,
        allocated: bool,
    },
    /// Final buffered D transfer; logical main completion has already emitted.
    BlitterFinalWrite {
        value: u16,
    },
    Disk {
        slot: u8,
        write: bool,
    },
    Audio {
        channel: u8,
        reload: bool,
    },
    Refresh,
    Strobe(DmaStrobe),
}

/// An immutable address/operation carried from admission to memory service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmaTransfer {
    pub target: DmaTransferTarget,
    pub address: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DmaAddressStage {
    Display(DisplayDmaReservation),
    Transfer(DmaTransfer),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmaPipeline {
    reservation: Option<DisplayDmaReservation>,
    address: Option<DmaAddressStage>,
    service: Option<DmaTransfer>,
    service_claimed: bool,
    cck_started: bool,
}

impl DmaPipeline {
    /// Reject malformed saved identities before a restored machine can index
    /// chip arrays. Memory addresses retain the concrete bus's normal masking.
    pub fn validate(&self) -> Result<(), String> {
        fn display(request: DisplayDmaReservation) -> Result<(), String> {
            let valid_channel = match request.channel {
                DisplayDmaChannel::Bitplane(plane) => plane < 8,
                DisplayDmaChannel::Sprite { channel, .. } => channel < 8,
            };
            if !valid_channel || !matches!(request.width_words, 1 | 2 | 4) {
                return Err("invalid saved display DMA reservation".into());
            }
            Ok(())
        }
        fn transfer(request: DmaTransfer) -> Result<(), String> {
            let valid = match request.target {
                DmaTransferTarget::Display { reservation, .. } => return display(reservation),
                DmaTransferTarget::DisplayRefresh {
                    reservation,
                    fixed_register,
                    ..
                } => {
                    display(reservation)?;
                    matches!(fixed_register, 0x38 | 0x3a | 0x3c | 0x3e | 0x1fe)
                }
                DmaTransferTarget::Copper { instruction_word } => matches!(instruction_word, 1 | 2),
                DmaTransferTarget::Blitter {
                    operation,
                    write_value,
                } => match operation {
                    BlitterDmaOp::WriteD => write_value.is_some(),
                    BlitterDmaOp::ReadA | BlitterDmaOp::ReadB | BlitterDmaOp::ReadC => {
                        write_value.is_none()
                    }
                    BlitterDmaOp::Internal => false,
                },
                DmaTransferTarget::Disk { slot, .. } => slot < 3,
                DmaTransferTarget::Audio { channel, .. } => channel < 4,
                DmaTransferTarget::BlitterInternal { .. } => request.address == 0,
                DmaTransferTarget::BlitterFinalWrite { .. }
                | DmaTransferTarget::Refresh
                | DmaTransferTarget::Strobe(_) => true,
            };
            if !valid {
                return Err("invalid saved DMA transfer target".into());
            }
            Ok(())
        }
        if let Some(request) = self.reservation {
            display(request)?;
        }
        if let Some(request) = self.address {
            match request {
                DmaAddressStage::Display(_) => {
                    return Err("unaddressed saved display DMA request".into());
                }
                DmaAddressStage::Transfer(request) => transfer(request)?,
            }
        }
        if let Some(request) = self.service {
            transfer(request)?;
        }
        Ok(())
    }

    /// Called only by the ordinary Agnus master-derived CCK edge.
    pub(crate) fn clock_boundary(&mut self) {
        assert!(
            self.service.is_none() || self.service_claimed,
            "DMA service missed before the next CCK"
        );
        self.cck_started = false;
        self.service = None;
        self.service_claimed = false;
    }

    /// Advance retained entries once. The caller addresses the returned
    /// display identity before admitting lower-priority requests to this cell.
    pub(crate) fn begin_cck(&mut self) -> Option<DisplayDmaReservation> {
        assert!(!self.cck_started, "DMA stages advanced twice in one CCK");
        self.cck_started = true;
        self.service_claimed = false;
        self.service = match self.address.take() {
            Some(DmaAddressStage::Transfer(transfer)) => Some(transfer),
            Some(DmaAddressStage::Display(_)) => {
                panic!("display DMA reached service without an addressed descriptor")
            }
            None => None,
        };
        let reservation = self.reservation.take();
        self.address = reservation.map(DmaAddressStage::Display);
        reservation
    }

    /// Retain the display request chosen by the single positional authority.
    pub(crate) fn reserve_display(&mut self, request: DisplayDmaReservation) -> bool {
        if self.reservation.is_some() {
            return false;
        }
        self.reservation = Some(request);
        true
    }

    /// Sample a reserved display pointer and its applicable signed modulo.
    pub(crate) fn address_display(&mut self, address: u32, pointer_modulo: i32) {
        let Some(DmaAddressStage::Display(reservation)) = self.address else {
            panic!("display addressing requires its retained reservation")
        };
        self.address = Some(DmaAddressStage::Transfer(DmaTransfer {
            target: DmaTransferTarget::Display {
                reservation,
                pointer_modulo,
            },
            address,
        }));
    }

    /// An occupied future cell cannot be claimed by another request. In
    /// particular, a display reservation remains occupied before addressing.
    pub(crate) fn admit(&mut self, transfer: DmaTransfer) -> bool {
        if self.address.is_some() {
            return false;
        }
        self.address = Some(DmaAddressStage::Transfer(transfer));
        true
    }

    /// Fixed DMAL is admitted before the following PT/MOD sample. A
    /// bitplane has no pointer yet; a sprite retained one at reservation.
    pub(crate) fn admit_refresh(&mut self, transfer: DmaTransfer, fixed_register: u16) -> bool {
        let display = match self.address {
            Some(DmaAddressStage::Display(reservation)) => Some((reservation, 0)),
            Some(DmaAddressStage::Transfer(DmaTransfer {
                target: DmaTransferTarget::Display { reservation, .. },
                address,
            })) => Some((reservation, address)),
            None => None,
            _ => return false,
        };
        self.address = Some(DmaAddressStage::Transfer(match display {
            Some((reservation, address)) => DmaTransfer {
                target: DmaTransferTarget::DisplayRefresh {
                    reservation,
                    fixed_register,
                },
                address: match reservation.channel {
                    DisplayDmaChannel::Bitplane(_) => transfer.address,
                    DisplayDmaChannel::Sprite { .. } => address | transfer.address,
                },
            },
            None => transfer,
        }));
        true
    }

    #[must_use]
    pub const fn reservation(&self) -> Option<DisplayDmaReservation> {
        self.reservation
    }

    #[must_use]
    pub const fn address(&self) -> Option<DmaAddressStage> {
        self.address
    }

    #[must_use]
    pub const fn service(&self) -> Option<DmaTransfer> {
        self.service
    }

    /// Whether the outgoing operation has retired in this CCK. The descriptor
    /// itself stays present for ownership through the second CPU phase.
    #[must_use]
    pub const fn service_was_claimed(&self) -> bool {
        self.service_claimed
    }

    #[must_use]
    pub const fn stages_started(&self) -> bool {
        self.cck_started
    }

    /// Claim the outgoing memory action once while retaining its descriptor
    /// for ownership inspection during the remainder of the CCK.
    pub(crate) fn claim_service(&mut self) -> Option<DmaTransfer> {
        if self.service_claimed {
            return None;
        }
        let transfer = self.service?;
        self.service_claimed = true;
        Some(transfer)
    }
}
