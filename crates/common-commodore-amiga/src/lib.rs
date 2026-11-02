//! Shared board-level substrate for Commodore Amiga machine crates.
//!
//! Holds the modules that are identical (or near-identical) across
//! the OCS / ECS / AGA chipset family: the CIA-A / CIA-B pair, the
//! copper, the RTC, the memory map, and the chipset-agnostic parts
//! of the Denise-side rendering.
//!
//! OCS machine wiring and the shared `common-commodore-amiga-ecs` /
//! `common-commodore-amiga-aga` wiring plug their specific Agnus + Denise
//! chips on top of this substrate. Fixed A500+, A600 and A1200 regional
//! machine owners select that shared wiring; OCS remains grouped.
//!
//! Mirrors the Spectrum's `common-sinclair-zx-spectrum` substrate
//! pattern. See Seam 1 of
//! `knowledge/decisions/amiga-full-family-architecture-review.md`.

pub mod board;
pub mod cia;
pub mod clock;
pub mod copper;
pub mod cpu;
pub mod denise;
pub mod denise_chip;
pub mod denise_counter;
pub mod denise_window;
pub mod driver;
pub mod memory;
mod ram_config;
pub mod rtc;

pub use board::{
    BusResponse, BusTransaction, CIA_E_CLOCK_DIVISOR, ChipRamBus, MotherboardBridgeAction,
    NTSC_SYSTEM_TICK_HZ, PAL_SYSTEM_TICK_HZ, SizedBusResponse, SizedBusTransaction,
    SynchronizedMotherboardBridge, TICKS_PER_CCK, WatchingChipRamBus,
};
pub use clock::{CpuClock, CpuDomainPhase};
pub use cpu::{ActiveCpu, ActiveCpuDiagnosticSnapshot};
pub use denise_chip::DeniseChip;
pub use driver::{
    AMIGA_CPU_BOUNDARY_QUEUE_CAPACITY, AmigaDriver, AmigaInputDiagnosticSnapshot,
    AmigaMemoryWriteRecord, AmigaMemoryWriteSource, AmigaSchedulerDiagnosticSnapshot,
    AmigaTrackStreamDiagnosticSnapshot, CpuBoundary,
};

pub use ram_config::RamConfig;
