//! Chipset-agnostic board-level glue shared by every Amiga machine crate.
//!
//! These types and constants are byte-identical across the OCS / ECS /
//! AGA machine crates. They were relocated here from the three crates
//! (#34, unified-driver replatform) so the shared per-CCK driver can
//! reference them once instead of three times. None of them depend on
//! the chipset variant: the blitter-bus adaptor sees only the shared
//! chip-RAM `Memory`, the CPU bus-transaction value types are plain
//! data, and the master-clock divisors are the same on every Amiga.

use crate::memory::Memory;
use motorola_68000::bus::{BusStatus, DataPortSize, TransferSize};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// Ticks per Agnus colour clock. A CCK (HRM beam-coordinate unit) is
/// two master/4 ticks — one tick per lores pixel.
pub const TICKS_PER_CCK: u64 = 2;

/// PAL Amiga system-tick rate (master clock / 4).
pub const PAL_SYSTEM_TICK_HZ: u64 = 7_093_790;

/// NTSC Amiga system-tick rate (master clock / 4).
pub const NTSC_SYSTEM_TICK_HZ: u64 = 7_159_090;

/// CIA E-clock divider: real CIA E-clock runs at master/40 = 0.71 MHz.
/// Our primary tick unit is master/4 (= 68000 CPU clock = lores pixel
/// rate), so CIAs fire once every 10 ticks. Confirmed by HRM register
/// map: "CIAA timer A (.709379 MHz PAL)" = master/40 exactly.
pub const CIA_E_CLOCK_DIVISOR: u64 = 10;

/// Serializable timing state for an asynchronous accelerator's
/// synchronized motherboard bridge.
///
/// The accelerator-local bus does not use this state. A non-local CPU cycle
/// must first cross the synchronizer, then perform one motherboard access,
/// then return the latched response on a later motherboard slot. Retaining
/// the response is important for save states: restoring after a
/// side-effecting write must not dispatch that write a second time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SynchronizedMotherboardBridge {
    phase: BridgePhase,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum BridgePhase {
    #[default]
    Idle,
    AwaitingSlot,
    AddressAccepted,
    ResponsePending(BusStatus),
}

/// Stable diagnostic name for a synchronized motherboard-bridge phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotherboardBridgeDiagnosticPhase {
    /// No accelerator transaction is crossing the bridge.
    Idle,
    /// A transaction is waiting for its first motherboard slot.
    AwaitingSlot,
    /// The address has crossed the synchronizer and awaits motherboard
    /// dispatch.
    AddressAccepted,
    /// A completed motherboard result is retained for a later return slot.
    ResponsePending,
}

/// Stable diagnostic classification of a retained motherboard response.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotherboardBridgeResponseKind {
    /// A completed 16-bit motherboard response.
    Ready,
    /// A terminal motherboard bus error.
    Error,
    /// A completed dynamic-sized response.
    ReadySized,
}

/// Bounded diagnostic representation of one retained bridge response.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotherboardBridgeResponseDiagnosticSnapshot {
    /// Response classification.
    pub kind: MotherboardBridgeResponseKind,
    /// Data returned by a conventional 16-bit response.
    pub word_data: Option<u16>,
    /// Physical D31-D0 image returned by a dynamic-sized response.
    pub sized_data: Option<u32>,
    /// Dynamic-sized responder width.
    pub sized_port: Option<DataPortSize>,
}

/// Complete side-effect-free view of synchronized bridge progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotherboardBridgeDiagnosticSnapshot {
    /// Current synchronizer phase.
    pub phase: MotherboardBridgeDiagnosticPhase,
    /// Completed response retained until a future motherboard slot.
    pub latched_response: Option<MotherboardBridgeResponseDiagnosticSnapshot>,
}

/// What the shared driver should do after polling a synchronized bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotherboardBridgeAction {
    /// Keep the processor waiting; no motherboard access is admitted.
    Wait,
    /// Attempt the motherboard access on this slot.
    Access,
    /// Deliver a previously latched response to the processor.
    Complete(BusStatus),
}

impl SynchronizedMotherboardBridge {
    /// Observe one active-CPU edge.
    ///
    /// Only edges carrying `motherboard_slot = true` advance the bridge.
    /// An admitted access remains in [`BridgePhase::AddressAccepted`] until
    /// the driver either latches a response or retries after motherboard
    /// arbitration.
    pub fn poll(&mut self, motherboard_slot: bool) -> MotherboardBridgeAction {
        match (self.phase, motherboard_slot) {
            (BridgePhase::Idle, true) => {
                self.phase = BridgePhase::AddressAccepted;
                MotherboardBridgeAction::Wait
            }
            (BridgePhase::Idle, false) => {
                self.phase = BridgePhase::AwaitingSlot;
                MotherboardBridgeAction::Wait
            }
            (BridgePhase::AwaitingSlot, true) => {
                self.phase = BridgePhase::AddressAccepted;
                MotherboardBridgeAction::Wait
            }
            (BridgePhase::AddressAccepted, true) => MotherboardBridgeAction::Access,
            (BridgePhase::ResponsePending(status), true) => {
                self.phase = BridgePhase::Idle;
                MotherboardBridgeAction::Complete(status)
            }
            (
                BridgePhase::AwaitingSlot
                | BridgePhase::AddressAccepted
                | BridgePhase::ResponsePending(_),
                false,
            ) => MotherboardBridgeAction::Wait,
        }
    }

    /// Retain the result of the admitted motherboard access.
    ///
    /// The result is returned to the processor by a later
    /// [`Self::poll`] call. `Wait` is not a completed access; callers should
    /// leave the bridge in its accepted-address phase and retry instead.
    pub fn latch_response(&mut self, status: BusStatus) {
        assert!(
            !matches!(status, BusStatus::Wait),
            "a synchronized bridge cannot latch an incomplete response"
        );
        assert!(
            matches!(self.phase, BridgePhase::AddressAccepted),
            "a synchronized bridge can only latch an admitted access"
        );
        self.phase = BridgePhase::ResponsePending(status);
    }

    /// Return the bridge to its power-on state.
    pub fn reset(&mut self) {
        self.phase = BridgePhase::Idle;
    }

    /// Whether no motherboard transaction is being synchronized.
    #[must_use]
    pub const fn is_idle(&self) -> bool {
        matches!(self.phase, BridgePhase::Idle)
    }

    /// Whether pending bridge work is paired with a waiting processor bus
    /// cycle.
    ///
    /// An idle bridge imposes no constraint. Every other phase belongs to
    /// the processor cycle that initiated it and must not survive after that
    /// cycle has completed or been replaced.
    #[must_use]
    pub const fn is_coherent_with_waiting_cpu(self, cpu_waiting: bool) -> bool {
        self.is_idle() || cpu_waiting
    }

    /// Capture the exact bridge phase and any retained response without
    /// advancing synchronization.
    #[must_use]
    pub fn diagnostic_snapshot(self) -> MotherboardBridgeDiagnosticSnapshot {
        let (phase, latched_response) = match self.phase {
            BridgePhase::Idle => (MotherboardBridgeDiagnosticPhase::Idle, None),
            BridgePhase::AwaitingSlot => (MotherboardBridgeDiagnosticPhase::AwaitingSlot, None),
            BridgePhase::AddressAccepted => {
                (MotherboardBridgeDiagnosticPhase::AddressAccepted, None)
            }
            BridgePhase::ResponsePending(status) => (
                MotherboardBridgeDiagnosticPhase::ResponsePending,
                Some(match status {
                    BusStatus::Ready(data) => MotherboardBridgeResponseDiagnosticSnapshot {
                        kind: MotherboardBridgeResponseKind::Ready,
                        word_data: Some(data),
                        sized_data: None,
                        sized_port: None,
                    },
                    BusStatus::Error => MotherboardBridgeResponseDiagnosticSnapshot {
                        kind: MotherboardBridgeResponseKind::Error,
                        word_data: None,
                        sized_data: None,
                        sized_port: None,
                    },
                    BusStatus::ReadySized { data, port } => {
                        MotherboardBridgeResponseDiagnosticSnapshot {
                            kind: MotherboardBridgeResponseKind::ReadySized,
                            word_data: None,
                            sized_data: Some(data),
                            sized_port: Some(port),
                        }
                    }
                    BusStatus::Wait => {
                        unreachable!("bridge invariants forbid a retained Wait response")
                    }
                }),
            ),
        };
        MotherboardBridgeDiagnosticSnapshot {
            phase,
            latched_response,
        }
    }
}

impl<'de> Deserialize<'de> for SynchronizedMotherboardBridge {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct StoredBridge {
            phase: BridgePhase,
        }

        let stored = StoredBridge::deserialize(deserializer)?;
        if matches!(stored.phase, BridgePhase::ResponsePending(BusStatus::Wait)) {
            return Err(D::Error::custom(
                "a synchronized bridge cannot retain an incomplete response",
            ));
        }
        Ok(Self {
            phase: stored.phase,
        })
    }
}

/// `BlitterBus` adaptor over chip RAM. The blitter sees chip RAM only,
/// via Agnus DMA, and addresses wrap at the 2 MiB chip-RAM boundary.
pub struct ChipRamBus<'a>(pub &'a mut Memory);

impl commodore_agnus_ocs::BlitterBus for ChipRamBus<'_> {
    fn read_word(&mut self, addr: u32) -> u16 {
        self.0.read_chip_ram_word(addr)
    }
    fn write_word(&mut self, addr: u32, val: u16) {
        self.0.write_word(addr & 0x001F_FFFE, val);
    }
}

/// Snapshotted out of `cpu.state.BusCycle` once per servicing pass so
/// chip-select handlers can operate on plain values instead of holding
/// a borrow on `&mut self.cpu`. `data` is `0` for reads.
#[derive(Clone, Copy)]
pub struct BusTransaction {
    pub addr: u32,
    pub is_read: bool,
    pub is_word: bool,
    pub data: u16,
}

/// What a chip-select arm produced for one [`BusTransaction`].
///
/// `Byte` and `Word` describe what the chip drove on the data lines;
/// the dispatcher applies the byte-lane extraction rule once.
/// `WriteAck` is the write-side equivalent — the chip absorbed the
/// write and the dispatcher returns `Ready(0)`.
///
/// Every reachable cycle ultimately gets handled (Memory's fallback
/// always claims the cycle, returning chip RAM, slow RAM, ROM, or
/// floating-bus from `last_bus_value`), so a "no chip drove anything"
/// variant is unreachable in this model.
#[derive(Clone, Copy)]
pub enum BusResponse {
    /// Chip drove an 8-bit value. Always returned in the low 8 bits.
    Byte(u8),
    /// Chip drove a 16-bit value. For byte reads the dispatcher
    /// extracts the byte lane: even address (UDS) → high byte, odd
    /// (LDS) → low byte, both delivered in the low 8 bits.
    Word(u16),
    /// Write completed; bus_status becomes `Ready(0)`.
    WriteAck,
}

/// One MC68020/MC68030 physical data phase presented to a responder.
///
/// This is deliberately separate from [`BusTransaction`]. The legacy type
/// describes an already-decomposed byte/word access and is used by every
/// existing MC68000-shaped machine path. A sized transaction preserves the
/// current SIZ value and the physical D31-D0 write image until a responder
/// reports its width.
#[derive(Clone, Copy)]
pub struct SizedBusTransaction {
    /// Current physical phase address.
    pub addr: u32,
    /// `true` for a read phase, `false` for a write phase.
    pub is_read: bool,
    /// Current SIZ value: bytes remaining in the logical operand.
    pub remaining: TransferSize,
    /// Physical D31-D0 image driven by the processor during a write.
    pub data: u32,
}

/// Completion produced by one evidence-backed dynamic-sized responder.
#[derive(Clone, Copy)]
pub struct SizedBusResponse {
    /// Physical D31-D0 image driven during a read; ignored for writes.
    pub data: u32,
    /// Responder width encoded by DSACK1/DSACK0.
    pub port: DataPortSize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synchronized_bridge_advances_only_on_motherboard_slots() {
        let mut bridge = SynchronizedMotherboardBridge::default();

        assert_eq!(
            bridge.poll(false),
            MotherboardBridgeAction::Wait,
            "a request can arrive between motherboard slots"
        );
        assert_eq!(bridge.poll(false), MotherboardBridgeAction::Wait);
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Wait);
        assert_eq!(bridge.poll(false), MotherboardBridgeAction::Wait);
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Access);

        bridge.latch_response(BusStatus::Ready(0x1234));
        assert_eq!(bridge.poll(false), MotherboardBridgeAction::Wait);
        assert_eq!(
            bridge.poll(true),
            MotherboardBridgeAction::Complete(BusStatus::Ready(0x1234))
        );
        assert!(bridge.is_idle());
    }

    #[test]
    fn synchronized_bridge_diagnostics_track_phase_and_latched_response() {
        let mut bridge = SynchronizedMotherboardBridge::default();
        assert_eq!(
            bridge.diagnostic_snapshot(),
            MotherboardBridgeDiagnosticSnapshot {
                phase: MotherboardBridgeDiagnosticPhase::Idle,
                latched_response: None,
            },
        );

        assert_eq!(bridge.poll(false), MotherboardBridgeAction::Wait);
        assert_eq!(
            bridge.diagnostic_snapshot().phase,
            MotherboardBridgeDiagnosticPhase::AwaitingSlot,
        );
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Wait);
        assert_eq!(
            bridge.diagnostic_snapshot().phase,
            MotherboardBridgeDiagnosticPhase::AddressAccepted,
        );
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Access);
        bridge.latch_response(BusStatus::ReadySized {
            data: 0x1234_5678,
            port: DataPortSize::Word,
        });
        assert_eq!(
            bridge.diagnostic_snapshot(),
            MotherboardBridgeDiagnosticSnapshot {
                phase: MotherboardBridgeDiagnosticPhase::ResponsePending,
                latched_response: Some(MotherboardBridgeResponseDiagnosticSnapshot {
                    kind: MotherboardBridgeResponseKind::ReadySized,
                    word_data: None,
                    sized_data: Some(0x1234_5678),
                    sized_port: Some(DataPortSize::Word),
                }),
            },
        );
    }

    #[test]
    fn synchronized_bridge_retries_an_unanswered_access() {
        let mut bridge = SynchronizedMotherboardBridge::default();

        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Wait);
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Access);
        assert_eq!(
            bridge.poll(true),
            MotherboardBridgeAction::Access,
            "motherboard arbitration can defer the admitted access"
        );
    }

    #[test]
    fn synchronized_bridge_rejects_a_serialized_wait_response() {
        let invalid = SynchronizedMotherboardBridge {
            phase: BridgePhase::ResponsePending(BusStatus::Wait),
        };
        let bytes = postcard::to_allocvec(&invalid).expect("serialize forged bridge");

        assert!(
            postcard::from_bytes::<SynchronizedMotherboardBridge>(&bytes).is_err(),
            "an incomplete response is not a reachable pending phase"
        );
    }

    #[test]
    fn synchronized_bridge_serializes_a_pending_response() {
        let mut bridge = SynchronizedMotherboardBridge::default();
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Wait);
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Access);
        bridge.latch_response(BusStatus::ReadySized {
            data: 0x1234_5678,
            port: DataPortSize::Word,
        });

        let encoded = postcard::to_allocvec(&bridge).expect("serialize bridge");
        let mut restored: SynchronizedMotherboardBridge =
            postcard::from_bytes(&encoded).expect("deserialize bridge");

        assert_eq!(
            restored.poll(true),
            MotherboardBridgeAction::Complete(BusStatus::ReadySized {
                data: 0x1234_5678,
                port: DataPortSize::Word,
            })
        );
        assert!(restored.is_idle());
    }

    #[test]
    fn synchronized_bridge_reset_discards_an_in_flight_response() {
        let mut bridge = SynchronizedMotherboardBridge::default();
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Wait);
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Access);
        bridge.latch_response(BusStatus::Ready(0xA55A));

        bridge.reset();

        assert!(bridge.is_idle());
        assert_eq!(bridge.poll(true), MotherboardBridgeAction::Wait);
    }
}
