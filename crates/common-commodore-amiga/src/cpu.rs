//! Runtime-selected Motorola 680x0 CPUs for Amiga machines.
//!
//! CPU choice is independent of chipset choice: stock machines and expansion
//! boards can pair the same OCS, ECS, or AGA chipset with different processors.
//! [`ActiveCpu`] keeps that finite set as a serializable value while exposing
//! the shared [`Cpu68000`] bus-protocol state required by the board driver.

use std::ops::{Deref, DerefMut};

use motorola_68000::{Cpu68000, CpuModel};
use motorola_68010::Cpu68010;
use motorola_68020::Cpu68020;
use motorola_68030::Cpu68030;
use motorola_68040::Cpu68040;
use serde::{Deserialize, Serialize};

/// The processor currently driving an Amiga machine's CPU bus.
///
/// EC variants intentionally retain a distinct enum discriminant even where
/// they currently share an implementation type with the full processor. The
/// discriminant is architectural configuration and supplies the correct
/// [`CpuModel`] while those implementations continue to converge.
#[derive(Clone, Serialize, Deserialize)]
pub enum ActiveCpu {
    /// Motorola MC68000.
    M68000(Cpu68000),
    /// Motorola MC68010.
    M68010(Cpu68010),
    /// Motorola MC68EC020, as fitted to the A1200 and CD32.
    M68EC020(Cpu68020),
    /// Motorola MC68020.
    M68020(Cpu68020),
    /// Motorola MC68EC030, with no on-chip MMU.
    M68EC030(Cpu68030),
    /// Motorola MC68030.
    M68030(Cpu68030),
    /// Motorola MC68040.
    M68040(Cpu68040),
}

impl ActiveCpu {
    /// Construct one of the processor models represented by this enum.
    ///
    /// Models whose execution wrapper has not joined [`ActiveCpu`] return
    /// `None`. Keeping this match here prevents each chipset machine from
    /// independently translating configuration into processor state.
    #[must_use]
    pub fn from_model(model: CpuModel) -> Option<Self> {
        match model {
            CpuModel::M68000 => Some(Self::M68000(Cpu68000::new())),
            CpuModel::M68010 => Some(Self::M68010(Cpu68010::new())),
            CpuModel::M68EC020 => Some(Self::M68EC020(Cpu68020::new())),
            CpuModel::M68020 => Some(Self::M68020(Cpu68020::new())),
            CpuModel::M68EC030 => Some(Self::M68EC030(Cpu68030::new())),
            CpuModel::M68030 => Some(Self::M68030(Cpu68030::new())),
            CpuModel::M68040 => Some(Self::M68040(Cpu68040::new())),
            CpuModel::M68LC030
            | CpuModel::M68EC040
            | CpuModel::M68LC040
            | CpuModel::M68EC060
            | CpuModel::M68LC060
            | CpuModel::M68060 => None,
        }
    }

    /// Return the configured processor model.
    #[must_use]
    pub const fn model(&self) -> CpuModel {
        match self {
            Self::M68000(_) => CpuModel::M68000,
            Self::M68010(_) => CpuModel::M68010,
            Self::M68EC020(_) => CpuModel::M68EC020,
            Self::M68020(_) => CpuModel::M68020,
            Self::M68EC030(_) => CpuModel::M68EC030,
            Self::M68030(_) => CpuModel::M68030,
            Self::M68040(_) => CpuModel::M68040,
        }
    }

    /// Borrow the common MC68000 bus-protocol and register state.
    #[must_use]
    pub const fn as_base(&self) -> &Cpu68000 {
        match self {
            Self::M68000(cpu) => cpu,
            Self::M68010(cpu) => cpu.as_inner(),
            Self::M68EC020(cpu) | Self::M68020(cpu) => cpu.as_inner().as_inner(),
            Self::M68EC030(cpu) | Self::M68030(cpu) => cpu.as_inner().as_inner().as_inner(),
            Self::M68040(cpu) => cpu.as_inner().as_inner().as_inner().as_inner(),
        }
    }

    /// Mutably borrow the common MC68000 bus-protocol and register state.
    #[must_use]
    pub const fn as_base_mut(&mut self) -> &mut Cpu68000 {
        match self {
            Self::M68000(cpu) => cpu,
            Self::M68010(cpu) => cpu.as_inner_mut(),
            Self::M68EC020(cpu) | Self::M68020(cpu) => cpu.as_inner_mut().as_inner_mut(),
            Self::M68EC030(cpu) | Self::M68030(cpu) => {
                cpu.as_inner_mut().as_inner_mut().as_inner_mut()
            }
            Self::M68040(cpu) => cpu
                .as_inner_mut()
                .as_inner_mut()
                .as_inner_mut()
                .as_inner_mut(),
        }
    }

    /// Confirm that processor-family-only state matches the enum variant.
    ///
    /// The shared core serializes the optional instruction-cache contents so
    /// warm 68020+ caches survive snapshots. A 68000 or 68010 must never carry
    /// that cache, while every currently supported 68020+ wrapper installs it.
    /// Snapshot restore uses this check to reject forged or corrupt payloads
    /// before they can enable impossible cached instruction fetches.
    #[must_use]
    pub fn variant_state_is_coherent(&self) -> bool {
        match self {
            Self::M68000(_) | Self::M68010(_) => self.as_base().variant_icache.is_none(),
            Self::M68EC020(_)
            | Self::M68020(_)
            | Self::M68EC030(_)
            | Self::M68030(_)
            | Self::M68040(_) => self.as_base().variant_icache.is_some(),
        }
    }

    /// Advance the configured processor by one of its input-clock edges.
    pub fn tick(&mut self) {
        match self {
            Self::M68000(cpu) => cpu.tick(),
            Self::M68010(cpu) => cpu.tick(),
            Self::M68EC020(cpu) | Self::M68020(cpu) => cpu.tick(),
            Self::M68EC030(cpu) | Self::M68030(cpu) => cpu.tick(),
            Self::M68040(cpu) => cpu.tick(),
        }
    }

    /// Reset the configured processor to the supplied supervisor stack and PC.
    pub fn reset_to(&mut self, ssp: u32, pc: u32) {
        match self {
            Self::M68000(cpu) => cpu.reset_to(ssp, pc),
            Self::M68010(cpu) => cpu.reset_to(ssp, pc),
            Self::M68EC020(cpu) | Self::M68020(cpu) => cpu.reset_to(ssp, pc),
            Self::M68EC030(cpu) | Self::M68030(cpu) => cpu.reset_to(ssp, pc),
            Self::M68040(cpu) => cpu.reset_to(ssp, pc),
        }
    }
}

impl Default for ActiveCpu {
    fn default() -> Self {
        Self::M68000(Cpu68000::new())
    }
}

impl Deref for ActiveCpu {
    type Target = Cpu68000;

    fn deref(&self) -> &Self::Target {
        self.as_base()
    }
}

impl DerefMut for ActiveCpu {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_base_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motorola_68000::cpu::State;

    fn all_variants() -> [ActiveCpu; 7] {
        [
            ActiveCpu::M68000(Cpu68000::new()),
            ActiveCpu::M68010(Cpu68010::new()),
            ActiveCpu::M68EC020(Cpu68020::new()),
            ActiveCpu::M68020(Cpu68020::new()),
            ActiveCpu::M68EC030(Cpu68030::new()),
            ActiveCpu::M68030(Cpu68030::new()),
            ActiveCpu::M68040(Cpu68040::new()),
        ]
    }

    #[test]
    fn variants_report_their_configured_model() {
        let models = all_variants().map(|cpu| cpu.model());

        assert_eq!(
            models,
            [
                CpuModel::M68000,
                CpuModel::M68010,
                CpuModel::M68EC020,
                CpuModel::M68020,
                CpuModel::M68EC030,
                CpuModel::M68030,
                CpuModel::M68040,
            ]
        );
    }

    #[test]
    fn supported_models_construct_the_matching_variant() {
        for model in [
            CpuModel::M68000,
            CpuModel::M68010,
            CpuModel::M68EC020,
            CpuModel::M68020,
            CpuModel::M68EC030,
            CpuModel::M68030,
            CpuModel::M68040,
        ] {
            let cpu = ActiveCpu::from_model(model).expect("supported active CPU model");
            assert_eq!(cpu.model(), model);
        }

        assert!(ActiveCpu::from_model(CpuModel::M68LC030).is_none());
        assert!(ActiveCpu::from_model(CpuModel::M68060).is_none());
    }

    #[test]
    fn reset_and_tick_dispatch_to_every_variant() {
        for mut cpu in all_variants() {
            cpu.reset_to(0x0012_3456, 0x0000_1000);

            assert_eq!(cpu.regs.ssp, 0x0012_3456);
            assert_eq!(cpu.regs.pc, 0x0000_1000);
            assert!(matches!(cpu.state, State::Idle));

            cpu.tick();
            assert!(!matches!(cpu.state, State::Idle));
        }
    }

    #[test]
    fn deref_exposes_shared_cpu_state() {
        let mut cpu = ActiveCpu::M68040(Cpu68040::new());

        cpu.ipl = 5;

        assert_eq!(cpu.as_base().ipl, 5);
    }

    #[test]
    fn serde_preserves_every_variant_discriminant_and_state() {
        for mut cpu in all_variants() {
            let model = cpu.model();
            cpu.regs.d[3] = 0x1234_5678;
            let encoded = postcard::to_allocvec(&cpu).expect("serialize active CPU");
            let restored: ActiveCpu =
                postcard::from_bytes(&encoded).expect("deserialize active CPU");

            assert_eq!(restored.model(), model);
            assert_eq!(restored.regs.d[3], 0x1234_5678);
        }
    }

    #[test]
    fn variant_state_rejects_a_cache_on_an_mc68000() {
        let mut cpu = ActiveCpu::M68000(Cpu68000::new());
        cpu.as_base_mut().variant_icache = Some(motorola_68000::ICache::new());
        let encoded = postcard::to_allocvec(&cpu).expect("serialize forged active CPU");
        let restored: ActiveCpu =
            postcard::from_bytes(&encoded).expect("deserialize forged active CPU");

        assert!(!restored.variant_state_is_coherent());
    }

    #[test]
    fn constructed_variant_cache_shapes_are_coherent() {
        for cpu in all_variants() {
            assert!(
                cpu.variant_state_is_coherent(),
                "{:?} must carry the correct instruction-cache shape",
                cpu.model()
            );
        }
    }
}
