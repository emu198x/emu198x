// Explicit feature-selected factory: each browser artifact links one family.
use super::*;
pub(super) fn create(
    family: &str,
    variant: &str,
    firmware: &FirmwareSet<'_>,
) -> Result<Box<dyn Host>, String> {
    match family {
        #[cfg(feature = "acorn-atom")]
        "acorn-atom" => build::<runtime_acorn_atom::AtomRuntime>(variant, firmware),
        #[cfg(feature = "acorn-bbc-micro")]
        "acorn-bbc-micro" => {
            use runtime_acorn_bbc_micro::BbcMicroRuntime as R;
            let model =
                R::model_from_id(variant).ok_or_else(|| format!("unknown variant: {variant}"))?;
            let runtime =
                R::from_firmware_with_basic(model, firmware).map_err(|e| e.to_string())?;
            Ok(Box::new(RuntimeHost {
                web: WebMachine::new(runtime),
            }))
        }
        #[cfg(feature = "acorn-electron")]
        "acorn-electron" => build::<runtime_acorn_electron::ElectronRuntime>(variant, firmware),
        #[cfg(feature = "amstrad-cpc")]
        "amstrad-cpc" => build::<runtime_amstrad_cpc::AmstradCpcRuntime>(variant, firmware),
        #[cfg(feature = "atari-2600")]
        "atari-2600" => build::<runtime_atari_2600::Atari2600Runtime>(variant, firmware),
        #[cfg(feature = "atari-5200")]
        "atari-5200" => build::<runtime_atari_5200::Atari5200Runtime>(variant, firmware),
        #[cfg(feature = "atari-7800")]
        "atari-7800" => build::<runtime_atari_7800::Atari7800Runtime>(variant, firmware),
        #[cfg(feature = "atari-800xl")]
        "atari-800xl" => build::<runtime_atari_800xl::Atari800xlRuntime>(variant, firmware),
        #[cfg(feature = "coleco-colecovision")]
        "coleco-colecovision" => build::<runtime_coleco_colecovision::CvRuntime>(variant, firmware),
        #[cfg(feature = "commodore-amiga")]
        "commodore-amiga" => build::<runtime_commodore_amiga::AmigaRuntimeKind>(variant, firmware),
        #[cfg(feature = "commodore-c64")]
        "commodore-c64" => build::<runtime_commodore_c64::C64Runtime>(variant, firmware),
        #[cfg(feature = "commodore-pet")]
        "commodore-pet" => build::<runtime_commodore_pet::PetRuntime>(variant, firmware),
        #[cfg(feature = "commodore-vic-20")]
        "commodore-vic-20" => build::<runtime_commodore_vic_20::Vic20Runtime>(variant, firmware),
        #[cfg(feature = "dragon")]
        "dragon" => build::<runtime_dragon::DragonRuntime>(variant, firmware),
        #[cfg(feature = "jupiter-ace")]
        "jupiter-ace" => build::<runtime_jupiter_ace::JupiterAceRuntime>(variant, firmware),
        #[cfg(feature = "mattel-aquarius")]
        "mattel-aquarius" => build::<runtime_mattel_aquarius::AquariusRuntime>(variant, firmware),
        #[cfg(feature = "memotech-mtx")]
        "memotech-mtx" => build::<runtime_memotech_mtx::MtxRuntime>(variant, firmware),
        #[cfg(feature = "msx")]
        "msx" => build::<runtime_msx::MsxRuntime>(variant, firmware),
        #[cfg(feature = "nintendo-game-boy")]
        "nintendo-game-boy" => {
            build::<runtime_nintendo_game_boy::GameBoyRuntime>(variant, firmware)
        }
        #[cfg(feature = "nintendo-nes")]
        "nintendo-nes" => build::<runtime_nintendo_nes::NesRuntime>(variant, firmware),
        #[cfg(feature = "oric-atmos")]
        "oric-atmos" => build::<runtime_oric_atmos::OricRuntime>(variant, firmware),
        #[cfg(feature = "sega-game-gear")]
        "sega-game-gear" => build::<runtime_sega_game_gear::SmsRuntime>(variant, firmware),
        #[cfg(feature = "sega-master-system")]
        "sega-master-system" => build::<runtime_sega_master_system::SmsRuntime>(variant, firmware),
        #[cfg(feature = "sega-sg-1000")]
        "sega-sg-1000" => build::<runtime_sega_sg_1000::Sg1000Runtime>(variant, firmware),
        #[cfg(feature = "sinclair-zx-spectrum")]
        "sinclair-zx-spectrum" => {
            build::<runtime_sinclair_zx_spectrum::SpectrumRuntimeKind>(variant, firmware)
        }
        #[cfg(feature = "sinclair-zx80")]
        "sinclair-zx80" => build::<runtime_sinclair_zx80::Zx80Runtime>(variant, firmware),
        #[cfg(feature = "sinclair-zx81")]
        "sinclair-zx81" => build::<runtime_sinclair_zx81::Zx81Runtime>(variant, firmware),
        #[cfg(feature = "sord-m5")]
        "sord-m5" => build::<runtime_sord_m5::M5Runtime>(variant, firmware),
        #[cfg(feature = "spectravideo-svi-328")]
        "spectravideo-svi-328" => {
            build::<runtime_spectravideo_svi_328::Svi328Runtime>(variant, firmware)
        }
        #[cfg(feature = "tatung-einstein")]
        "tatung-einstein" => build::<runtime_tatung_einstein::EinsteinRuntime>(variant, firmware),
        _ => Err(format!("family is not compiled into this module: {family}")),
    }
}
pub(super) fn catalogue() -> Vec<Value> {
    let mut result = Vec::new();
    #[cfg(feature = "acorn-atom")]
    result.extend(profiles::<runtime_acorn_atom::AtomRuntime>("acorn-atom"));
    #[cfg(feature = "acorn-bbc-micro")]
    result.extend(profiles::<runtime_acorn_bbc_micro::BbcMicroRuntime>(
        "acorn-bbc-micro",
    ));
    #[cfg(feature = "acorn-electron")]
    result.extend(profiles::<runtime_acorn_electron::ElectronRuntime>(
        "acorn-electron",
    ));
    #[cfg(feature = "amstrad-cpc")]
    result.extend(profiles::<runtime_amstrad_cpc::AmstradCpcRuntime>(
        "amstrad-cpc",
    ));
    #[cfg(feature = "atari-2600")]
    result.extend(profiles::<runtime_atari_2600::Atari2600Runtime>(
        "atari-2600",
    ));
    #[cfg(feature = "atari-5200")]
    result.extend(profiles::<runtime_atari_5200::Atari5200Runtime>(
        "atari-5200",
    ));
    #[cfg(feature = "atari-7800")]
    result.extend(profiles::<runtime_atari_7800::Atari7800Runtime>(
        "atari-7800",
    ));
    #[cfg(feature = "atari-800xl")]
    result.extend(profiles::<runtime_atari_800xl::Atari800xlRuntime>(
        "atari-800xl",
    ));
    #[cfg(feature = "coleco-colecovision")]
    result.extend(profiles::<runtime_coleco_colecovision::CvRuntime>(
        "coleco-colecovision",
    ));
    #[cfg(feature = "commodore-amiga")]
    result.extend(profiles::<runtime_commodore_amiga::AmigaRuntimeKind>(
        "commodore-amiga",
    ));
    #[cfg(feature = "commodore-c64")]
    result.extend(profiles::<runtime_commodore_c64::C64Runtime>(
        "commodore-c64",
    ));
    #[cfg(feature = "commodore-pet")]
    result.extend(profiles::<runtime_commodore_pet::PetRuntime>(
        "commodore-pet",
    ));
    #[cfg(feature = "commodore-vic-20")]
    result.extend(profiles::<runtime_commodore_vic_20::Vic20Runtime>(
        "commodore-vic-20",
    ));
    #[cfg(feature = "dragon")]
    result.extend(profiles::<runtime_dragon::DragonRuntime>("dragon"));
    #[cfg(feature = "jupiter-ace")]
    result.extend(profiles::<runtime_jupiter_ace::JupiterAceRuntime>(
        "jupiter-ace",
    ));
    #[cfg(feature = "mattel-aquarius")]
    result.extend(profiles::<runtime_mattel_aquarius::AquariusRuntime>(
        "mattel-aquarius",
    ));
    #[cfg(feature = "memotech-mtx")]
    result.extend(profiles::<runtime_memotech_mtx::MtxRuntime>("memotech-mtx"));
    #[cfg(feature = "msx")]
    result.extend(profiles::<runtime_msx::MsxRuntime>("msx"));
    #[cfg(feature = "nintendo-game-boy")]
    result.extend(profiles::<runtime_nintendo_game_boy::GameBoyRuntime>(
        "nintendo-game-boy",
    ));
    #[cfg(feature = "nintendo-nes")]
    result.extend(profiles::<runtime_nintendo_nes::NesRuntime>("nintendo-nes"));
    #[cfg(feature = "oric-atmos")]
    result.extend(profiles::<runtime_oric_atmos::OricRuntime>("oric-atmos"));
    #[cfg(feature = "sega-game-gear")]
    result.extend(profiles::<runtime_sega_game_gear::SmsRuntime>(
        "sega-game-gear",
    ));
    #[cfg(feature = "sega-master-system")]
    result.extend(profiles::<runtime_sega_master_system::SmsRuntime>(
        "sega-master-system",
    ));
    #[cfg(feature = "sega-sg-1000")]
    result.extend(profiles::<runtime_sega_sg_1000::Sg1000Runtime>(
        "sega-sg-1000",
    ));
    #[cfg(feature = "sinclair-zx-spectrum")]
    result.extend(
        profiles::<runtime_sinclair_zx_spectrum::SpectrumRuntimeKind>("sinclair-zx-spectrum"),
    );
    #[cfg(feature = "sinclair-zx80")]
    result.extend(profiles::<runtime_sinclair_zx80::Zx80Runtime>(
        "sinclair-zx80",
    ));
    #[cfg(feature = "sinclair-zx81")]
    result.extend(profiles::<runtime_sinclair_zx81::Zx81Runtime>(
        "sinclair-zx81",
    ));
    #[cfg(feature = "sord-m5")]
    result.extend(profiles::<runtime_sord_m5::M5Runtime>("sord-m5"));
    #[cfg(feature = "spectravideo-svi-328")]
    result.extend(profiles::<runtime_spectravideo_svi_328::Svi328Runtime>(
        "spectravideo-svi-328",
    ));
    #[cfg(feature = "tatung-einstein")]
    result.extend(profiles::<runtime_tatung_einstein::EinsteinRuntime>(
        "tatung-einstein",
    ));
    result
}
