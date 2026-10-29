//! The MSX1 and MSX2 as a [`MachineApp`]: its flags, runtime, and report
//! fields.
//!
//! MSX chip state — VDP, the AY-3-8910 PSG (`ay.*`), the 8255 PPI — is read
//! over MCP through the generic `query` tool as query paths (`vdp`, `ay`,
//! `ppi`, and their leaves), not bespoke tools; the one MSX-specific addition
//! to the tool set is the shared AY-watch verbs.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, build_variant, build_variant_or_blank};
use runtime_msx::{
    BIOS_FIRMWARE_ID, MapperType, Model, MsxRuntime, MsxSessionQueryProvider, SUB_ROM_FIRMWARE_ID,
};
use serde_json::{Map, Value};

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Msx {
    pub firmware: FirmwareOverrides,
    pub cart: Option<PathBuf>,
    pub mapper: MapperType,
    pub cart2: Option<PathBuf>,
    pub mapper2: MapperType,
    pub model: Model,
}

impl Default for Msx {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            cart: None,
            mapper: MapperType::Plain,
            cart2: None,
            mapper2: MapperType::Plain,
            model: Model::SonyHb10p,
        }
    }
}

impl Msx {
    fn install_cartridges(&self, runtime: &mut MsxRuntime) -> Result<(), LaunchError> {
        if let Some(path) = &self.cart {
            runtime.insert_cartridge1(read_rom(path, "--cart")?, self.mapper);
        }
        if let Some(path) = &self.cart2 {
            runtime.insert_cartridge2(read_rom(path, "--cart2")?, self.mapper2);
        }
        Ok(())
    }
}

fn parse_mapper(flag: &str, value: &str) -> Result<MapperType, LaunchError> {
    Ok(match value {
        "plain" => MapperType::Plain,
        "konami" => MapperType::Konami,
        "konami-scc" => MapperType::KonamiScc,
        "ascii8" => MapperType::Ascii8,
        "ascii16" => MapperType::Ascii16,
        "fm-pac" => MapperType::FmPac,
        "sound-cartridge" => MapperType::SoundCartridge,
        other => {
            return Err(LaunchError::Usage(format!(
                "{flag} expects plain|konami|konami-scc|ascii8|ascii16|fm-pac|sound-cartridge, got {other}"
            )));
        }
    })
}

impl MachineApp for Msx {
    type Runtime = MsxRuntime;
    type Query = MsxSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-msx";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --bios PATH     MSX main ROM (32 KB); default
                    ~/.emu198x/roms/microsoft-msx/msx.rom, or msx2.rom on an
                    MSX2 (or set EMU198X_MSX_BIOS)
    --sub-rom PATH  MSX2 SUB-ROM (16 KB); default msx2ext.rom
                    (or set EMU198X_MSX_SUB_ROM)
    --rom PATH|ID=PATH  pin firmware (msx1-bios, msx2-sub-rom)
    --rom-dir DIR   firmware directory (or EMU198X_MSX_ROM_DIR)
    --model ID      sony-hb-10p | toshiba-hx-10 | mitsubishi-ml-f80 |
                    yamaha-cx5m | mitsubishi-ml-fx1 | sony-hb-10 |
                    philips-vg-8235 | sony-hb-f1 | panasonic-fs-a1
                    [default: sony-hb-10p]
    --cart PATH     cartridge ROM (slot 1)
    --mapper KIND   cartridge mapper: plain | konami | konami-scc | ascii8 |
                    ascii16 | fm-pac | sound-cartridge [default: plain]
    --cart2 PATH    cartridge ROM (slot 2)
    --mapper2 KIND  slot-2 mapper [default: plain]
    --region MODE   retired; select a named board with --model";
    const CONTROLS: &'static str = "    Close window    quit (or use the platform Quit shortcut)
    Cmd/Ctrl+Shift+R  hard reset
    Cmd/Ctrl+P      pause / resume
    A-Z 0-9 etc.    the MSX keyboard (cursor keys are real MSX keys)
    Shift / Ctrl    the MSX SHIFT / CTRL keys (Alt = GRAPH)
    Gamepad         joystick (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--bios" => self.firmware.pin(BIOS_FIRMWARE_ID, args.path(flag)?),
            "--sub-rom" => self.firmware.pin(SUB_ROM_FIRMWARE_ID, args.path(flag)?),
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model.variant_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown or retired MSX model `{id}`; select a named board: {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--cart" => self.cart = Some(args.path(flag)?),
            "--mapper" => self.mapper = parse_mapper(flag, &args.value(flag)?)?,
            "--cart2" => self.cart2 = Some(args.path(flag)?),
            "--mapper2" => self.mapper2 = parse_mapper(flag, &args.value(flag)?)?,
            "--region" => {
                let _ = args.value(flag)?;
                return Err(LaunchError::Usage(
                    "generic regional MSX presets were retired; select a named board with --model"
                        .into(),
                ));
            }

            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> MsxSessionQueryProvider {
        MsxSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<MsxRuntime, LaunchError> {
        let mut runtime = build_variant::<MsxRuntime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.install_cartridges(&mut runtime)?;
        Ok(runtime)
    }

    fn build_mcp_runtime(&self) -> Result<MsxRuntime, LaunchError> {
        let mut runtime =
            build_variant_or_blank::<MsxRuntime>(self.model, &self.firmware, MsxRuntime::blank)
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.install_cartridges(&mut runtime)?;
        Ok(runtime)
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &MsxRuntime, report: &mut Map<String, Value>) {
        let bios_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("bios_loaded".to_owned(), bios_loaded.into());
        report.insert(
            "cart1_loaded".to_owned(),
            runtime.cart1_bytes().is_some().into(),
        );
        report.insert(
            "cart2_loaded".to_owned(),
            runtime.cart2_bytes().is_some().into(),
        );
        report.insert("frames_run".to_owned(), frame_count.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};
    use std::path::Path;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, common, .. } = parse::<Msx>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.firmware.by_id.is_empty());
        assert!(app.cart.is_none());
        assert!(matches!(app.mapper, MapperType::Plain));
        assert!(app.cart2.is_none());
        assert!(matches!(app.mapper2, MapperType::Plain));
        assert_eq!(app.model, Model::SonyHb10p);
        assert_eq!(common.frames, 0);
        assert!(common.script.is_none());
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<Msx>(&args(&[
            "--bios",
            "/tmp/msx.rom",
            "--cart",
            "/tmp/game.rom",
            "--mapper",
            "konami-scc",
            "--cart2",
            "/tmp/other.rom",
            "--mapper2",
            "ascii16",
            "--model",
            "sony-hb-10p",
            "--frames",
            "120",
            "--screenshot",
            "/tmp/shot.png",
            "--audio-capture",
            "/tmp/audio.wav",
            "--script",
            "/tmp/steps.json",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(BIOS_FIRMWARE_ID)
                .map(PathBuf::as_path),
            Some(Path::new("/tmp/msx.rom"))
        );
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/game.rom")));
        assert!(matches!(app.mapper, MapperType::KonamiScc));
        assert_eq!(app.cart2.as_deref(), Some(Path::new("/tmp/other.rom")));
        assert!(matches!(app.mapper2, MapperType::Ascii16));
        assert_eq!(app.model, Model::SonyHb10p);
        assert_eq!(common.frames, 120);
        assert_eq!(
            common.screenshot.as_deref(),
            Some(Path::new("/tmp/shot.png"))
        );
        assert_eq!(
            common.audio_capture.as_deref(),
            Some(Path::new("/tmp/audio.wav"))
        );
        assert_eq!(common.script.as_deref(), Some(Path::new("/tmp/steps.json")));
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_bios_cart_mapper_board_scale_video() {
        let parsed = parse::<Msx>(&args(&[
            "--bios",
            "msx.rom",
            "--cart",
            "nemesis.rom",
            "--mapper",
            "konami",
            "--model",
            "sony-hb-10p",
            "--scale",
            "4",
            "--video",
            "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware.by_id.get(BIOS_FIRMWARE_ID),
            Some(&PathBuf::from("msx.rom"))
        );
        assert_eq!(app.cart, Some(PathBuf::from("nemesis.rom")));
        assert_eq!(app.mapper, MapperType::Konami);
        assert_eq!(app.model, Model::SonyHb10p);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn parse_cli_accepts_an_msx2_and_its_sub_rom() {
        let parsed = parse::<Msx>(&args(&[
            "--model",
            "philips-vg-8235",
            "--sub-rom",
            "/tmp/msx2ext.rom",
        ]))
        .expect("parses");
        let Parsed::Run { app, .. } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.model, Model::PhilipsVg8235);
        assert!(app.model.is_msx2());
        assert_eq!(
            app.firmware.by_id.get(SUB_ROM_FIRMWARE_ID),
            Some(&PathBuf::from("/tmp/msx2ext.rom"))
        );
    }

    #[test]
    fn a_bad_mapper_names_its_flag() {
        let err = parse::<Msx>(&args(&["--mapper2", "megarom"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage(
                "--mapper2 expects plain|konami|konami-scc|ascii8|ascii16|fm-pac|sound-cartridge, got megarom".to_owned()
            )
        );
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::SonyHb10.frame_ticks(), 228 * 262);
        assert_eq!(Model::SonyHb10p.frame_ticks(), 228 * 313);
    }
    #[test]
    fn retired_generic_presets_and_region_switches_are_errors() {
        for id in [
            "microsoft-msx1-ntsc",
            "microsoft-msx1-pal",
            "microsoft-msx2-ntsc",
            "microsoft-msx2-pal",
            "msx1-ntsc",
            "msx1-pal",
            "msx2-ntsc",
            "msx2-pal",
            "ntsc",
            "pal",
        ] {
            assert!(parse::<Msx>(&args(&["--model", id])).is_err(), "{id}");
        }
        for region in ["ntsc", "pal"] {
            assert!(parse::<Msx>(&args(&["--region", region])).is_err());
        }
    }
}
