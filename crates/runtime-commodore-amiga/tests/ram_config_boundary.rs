//! Moving the family RAM type must retain existing import and save contracts.

use common_commodore_amiga::RamConfig;
use machine_commodore_amiga_a1200_ntsc::RamConfig as AgaNtscRamConfig;
use machine_commodore_amiga_a1200_pal::RamConfig as AgaRamConfig;
use machine_commodore_amiga_ecs::RamConfig as EcsRamConfig;
use machine_commodore_amiga_ocs::RamConfig as OcsRamConfig;

#[test]
fn public_reexports_remain_the_same_type() {
    for config in [
        RamConfig::default(),
        RamConfig::a501_trapdoor(),
        RamConfig::a500_plus(),
        RamConfig::a500_maxed(),
        RamConfig {
            chip_kb: 2048,
            slow_kb: 0,
            fast_kb: 0,
        },
    ] {
        let ocs: OcsRamConfig = config;
        let ecs: EcsRamConfig = ocs;
        let aga: AgaRamConfig = ecs;
        let ntsc: AgaNtscRamConfig = aga;
        let runtime: runtime_commodore_amiga::RamConfig = ntsc;
        assert_eq!(runtime, config);
    }
}

#[test]
fn saved_layout_remains_three_ordered_u32_fields() -> Result<(), postcard::Error> {
    // The original OCS-owned serde struct encoded these three fields in order.
    // Postcard tuples have no added length/tag, so this also checks an old
    // representation decodes through every public re-export after the move.
    for fields in [
        (512, 0, 0),
        (512, 512, 0),
        (1024, 512, 8192),
        (2048, 0, 0),
        (256, 1536, 64),
        (u32::MAX, 1, 17),
    ] {
        let old_bytes = postcard::to_allocvec(&fields)?;
        let config = RamConfig {
            chip_kb: fields.0,
            slow_kb: fields.1,
            fast_kb: fields.2,
        };
        assert_eq!(postcard::to_allocvec(&config)?, old_bytes);
        assert_eq!(postcard::from_bytes::<OcsRamConfig>(&old_bytes)?, config);
        assert_eq!(postcard::from_bytes::<EcsRamConfig>(&old_bytes)?, config);
        assert_eq!(postcard::from_bytes::<AgaRamConfig>(&old_bytes)?, config);
        assert_eq!(
            postcard::from_bytes::<AgaNtscRamConfig>(&old_bytes)?,
            config
        );
    }
    Ok(())
}
