//! Sega Master System profile catalogue.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, MachineId, MachineProfile, MediaKind, MediaSlot,
    ProfileId, Region, WritebackPolicy, known_capability,
};
use runtime_sega_master_system_class::{SmsRuntime, SmsVariant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// Master System NTSC.
    SmsNtsc,
    /// Japanese Master System NTSC.
    SmsJapanNtsc,
    /// Master System PAL.
    SmsPal,
    /// Early Master System (NTSC) with the 315-5124 VDP.
    Sms1Ntsc,
    /// Early Master System (PAL) with the 315-5124 VDP.
    Sms1Pal,
}

impl Model {
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::SmsNtsc => "sega-master-system-ntsc",
            Self::SmsJapanNtsc => "sega-master-system-japan-ntsc",
            Self::SmsPal => "sega-master-system-pal",
            Self::Sms1Ntsc => "sega-master-system-1-ntsc",
            Self::Sms1Pal => "sega-master-system-1-pal",
        }
    }

    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        self.model_id()
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::SmsNtsc => "Sega Master System (NTSC)",
            Self::SmsJapanNtsc => "Sega Master System (Japan, NTSC)",
            Self::SmsPal => "Sega Master System (PAL)",
            Self::Sms1Ntsc => "Sega Master System 1 (NTSC)",
            Self::Sms1Pal => "Sega Master System 1 (PAL)",
        }
    }

    #[must_use]
    pub const fn region(self) -> Region {
        match self {
            Self::SmsPal | Self::Sms1Pal => Region::Pal,
            Self::SmsNtsc | Self::SmsJapanNtsc | Self::Sms1Ntsc => Region::Ntsc,
        }
    }

    /// The Z80's rate on this model.
    #[must_use]
    pub const fn z80_hz(self) -> u64 {
        match self {
            Self::SmsNtsc | Self::SmsJapanNtsc | Self::Sms1Ntsc => NTSC_Z80_HZ,
            Self::SmsPal | Self::Sms1Pal => PAL_Z80_HZ,
        }
    }

    /// What distinguishes this model from its siblings.
    ///
    /// The early Master System carries a 315-5124 VDP and the later one a
    /// 315-5246. Almost all software runs the same on both, because the
    /// difference is a set of register bits the earlier chip ANDs with the
    /// VRAM address bus and every commercial title sets them; what needs the
    /// earlier chip is software that closes one of those gates deliberately.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::SmsNtsc | Self::SmsPal => {
                "Sega Master System — Z80A + Sega VDP (315-5246) + SN76489, 8 KB RAM, Sega mapper cartridge boot."
            }
            Self::SmsJapanNtsc => {
                "Japanese Sega Master System — Z80A + Sega VDP (315-5246) + SN76489, 8 KB RAM, and Japanese I/O-chip behaviour."
            }
            Self::Sms1Ntsc | Self::Sms1Pal => {
                "Early Sega Master System — Z80A + Sega VDP (315-5124) + SN76489, 8 KB RAM, Sega mapper cartridge boot."
            }
        }
    }

    #[must_use]
    pub const fn variant(self) -> SmsVariant {
        match self {
            Self::SmsNtsc => SmsVariant::SmsNtsc,
            Self::SmsJapanNtsc => SmsVariant::SmsJapanNtsc,
            Self::SmsPal => SmsVariant::SmsPal,
            Self::Sms1Ntsc => SmsVariant::Sms1Ntsc,
            Self::Sms1Pal => SmsVariant::Sms1Pal,
        }
    }
}

/// Z80 rate on an NTSC Master System: a 53693175 Hz master clock over fifteen.
///
/// The same 3.579545 MHz as the colour subcarrier, which is where the master
/// clock comes from — fifteen times it.
const NTSC_Z80_HZ: u64 = 3_579_545;

/// Z80 rate on a PAL Master System: a 53203424 Hz master clock over fifteen.
///
/// Both models were built to the same divisors and differ only in the crystal,
/// so this is not the NTSC figure with a tolerance around it — it is 0.92%
/// slower, and a PAL machine given the NTSC rate runs that much fast. MAME's
/// `sms.cpp` states the master clock ("12 * subcarrier freq. (4.43361875MHz)")
/// and both divisors; the VDP takes master over ten, which is why
/// `sega_vdp::PAL_DOT_CLOCK_HZ` is 1.5 times this.
///
/// `machine-sega-master-system` has held this figure as `PAL_PSG_CLOCK_HZ`
/// since the PSG landed — the sound chip runs at the CPU rate — so the two
/// constants disagreed inside one machine until #1088.
const PAL_Z80_HZ: u64 = 3_546_893;

#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::SmsNtsc),
        profile_for(Model::SmsJapanNtsc),
        profile_for(Model::SmsPal),
        profile_for(Model::Sms1Ntsc),
        profile_for(Model::Sms1Pal),
    ]
}

#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    MachineProfile {
        machine_id: MachineId::from("sega-master-system"),
        profile_id: ProfileId::from(model.profile_id()),
        display_name: model.display_name().into(),
        family: Family::Other,
        region: model.region(),
        release_year: 1985,
        summary: model.summary().into(),
        clock: ClockDesc::new("z80-tstate", ClockRate::from_hz(model.z80_hz())),
        firmware: vec![],
        media_slots: vec![MediaSlot::new(
            "cartridge-1",
            "Cartridge Slot",
            MediaKind::Cartridge,
            true,
            WritebackPolicy::SidecarOnly,
        )],
        capabilities: CapabilitySet::with_all([
            known_capability("controller-input"),
            known_capability("scripted-input"),
        ]),
    }
}

/// A runtime with no cartridge inserted.
///
/// A free function rather than an inherent constructor: `SmsRuntime` belongs
/// to the class crate, so this crate cannot hang an `impl` off it.
#[must_use]
pub fn blank(model: Model) -> SmsRuntime {
    SmsRuntime::blank(profile_for(model), model.variant(), model.model_id())
}

/// A runtime with `cart_rom` inserted.
#[must_use]
pub fn with_cartridge(model: Model, cart_rom: Vec<u8>) -> SmsRuntime {
    SmsRuntime::new(
        profile_for(model),
        model.variant(),
        model.model_id(),
        cart_rom,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1149: `VdpVariant` was stored and never read, so the two Master
    /// System VDPs behaved identically. Now that they do not, the profile has
    /// to reach the right one — and this is the join that would silently give
    /// an early machine the later chip.
    #[test]
    fn each_model_builds_the_vdp_revision_it_names() {
        use sega_vdp::VdpVariant;
        for (model, expected) in [
            (Model::SmsNtsc, VdpVariant::Sms2),
            (Model::SmsJapanNtsc, VdpVariant::Sms2),
            (Model::SmsPal, VdpVariant::Sms2),
            (Model::Sms1Ntsc, VdpVariant::Sms1),
            (Model::Sms1Pal, VdpVariant::Sms1),
        ] {
            let runtime = with_cartridge(model, vec![0; 0x8000]);
            let machine = runtime.machine().expect("cartridge inserted");
            assert_eq!(
                machine.vdp().variant(),
                expected,
                "{} should carry a {expected:?} VDP",
                model.model_id()
            );
        }
    }

    #[test]
    fn cartridge_slot_uses_sidecar_writeback() {
        for profile in profiles() {
            assert_eq!(
                profile.media_slots[0].writeback,
                WritebackPolicy::SidecarOnly
            );
        }
    }

    /// The revision and the television standard are independent axes, so
    /// there are four models and not three.
    #[test]
    fn revision_and_region_vary_independently() {
        for (model, region) in [
            (Model::Sms1Ntsc, Region::Ntsc),
            (Model::Sms1Pal, Region::Pal),
        ] {
            let profile = profile_for(model);
            assert_eq!(profile.region, region);
            assert_eq!(profile.clock.rate.numerator_hz, model.z80_hz());
        }
        assert_ne!(
            profile_for(Model::Sms1Ntsc).clock.rate.numerator_hz,
            profile_for(Model::Sms1Pal).clock.rate.numerator_hz,
            "an early PAL machine runs at the PAL rate, not the NTSC one"
        );
    }

    #[test]
    fn profile_ids_are_unique() {
        let profiles = profiles();
        let mut ids: Vec<&str> = profiles.iter().map(|p| p.profile_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn pal_profile_uses_pal_region() {
        let p = profile_for(Model::SmsPal);
        assert_eq!(p.region, Region::Pal);
    }

    /// #1088: both models reported the NTSC Z80 rate, so a PAL machine ran
    /// 0.92% fast — every frame, and anything a script timed in t-states.
    #[test]
    fn the_two_regions_do_not_share_a_clock() {
        let ntsc = profile_for(Model::SmsNtsc);
        let pal = profile_for(Model::SmsPal);
        assert_ne!(
            ntsc.clock.rate.numerator_hz, pal.clock.rate.numerator_hz,
            "a PAL machine and an NTSC one cannot run at the same rate"
        );
        assert_eq!(ntsc.clock.rate.numerator_hz, 3_579_545);
        assert_eq!(pal.clock.rate.numerator_hz, 3_546_893);
    }

    /// Each rate is its own machine's master clock over fifteen. Stating the
    /// masters here is what makes the two figures checkable rather than merely
    /// different: 53693175 is fifteen times the NTSC colour subcarrier and
    /// 53203425 is twelve times the PAL one.
    ///
    /// The tolerance is a couple of hertz, because 3546893 is the figure the
    /// rest of the fleet carries and the exact quotient is 3546895. A rounding
    /// of that size is not the class of error this guards — #1088 was 32652 Hz
    /// out, and any repeat of it would be too.
    #[test]
    fn each_region_takes_its_rate_from_its_own_master_clock() {
        for (model, master) in [
            (Model::SmsNtsc, 53_693_175.0_f64),
            (Model::SmsPal, 53_203_425.0_f64),
        ] {
            let derived = master / 15.0;
            let stated = model.z80_hz() as f64;
            assert!(
                (stated - derived).abs() < 3.0,
                "{model:?} states {stated} Hz where its master gives {derived}"
            );
        }
    }

    /// The whole point of #998: one crate, one machine, and its id written
    /// as a literal where a scan of the workspace can see it. A named
    /// constant would read better and defeat the scan just as thoroughly as
    /// the variable this crate was split to remove.
    #[test]
    fn every_profile_declares_the_same_single_machine() {
        for profile in profiles() {
            assert_eq!(profile.machine_id.as_str(), "sega-master-system");
        }
    }
}
