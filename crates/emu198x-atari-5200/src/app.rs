//! The Atari 5200 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant};
use runtime_atari_5200::{Atari5200Runtime, Atari5200SessionQueryProvider, Model};
use serde_json::{Map, Value};

/// Launch options; the runtime owns the single NTSC model and BIOS catalogue.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Atari5200 {
    pub cart: Option<PathBuf>,
    pub firmware: FirmwareOverrides,
}

impl MachineApp for Atari5200 {
    type Runtime = Atari5200Runtime;
    type Query = Atari5200SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-atari-5200";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     Atari 5200 cartridge ROM (required; a bare PATH is accepted too)
    --bios PATH     Atari 5200 BIOS ROM (2 KB); default
                    ~/.emu198x/roms/atari-5200/bios.rom or 5200.rom (or set EMU198X_A5200_BIOS)
    --rom PATH|ID=PATH pin the optional BIOS (firmware ID: atari-5200-bios)
    --rom-dir DIR   BIOS directory (or set EMU198X_A5200_ROM_DIR)
    --region MODE   ntsc (the only standard the 5200 shipped in) [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      analogue stick (player 1)
    Z / X           fire
    Enter           Start    Backspace  Pause    Delete  Reset (keypad)
    0-9             keypad digits
    Numpad * / /    keypad * and # keys";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
            "--bios" => self
                .firmware
                .pin(runtime_atari_5200::BIOS_FIRMWARE_ID, args.path(flag)?),
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    Model::A5200Ntsc.variant_id(),
                    &Model::A5200Ntsc.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--region" => {
                match args.value(flag)?.as_str() {
                    "ntsc" => {}
                    "pal" => {
                        return Err(LaunchError::Usage(
                            "the Atari 5200 shipped NTSC only — Atari's CX5200 Field Service \
                             Manual has a PAL GTIA in one as a part to replace, not a region \
                             to select"
                                .to_owned(),
                        ));
                    }
                    other => {
                        return Err(LaunchError::Usage(format!(
                            "--region expects ntsc, got {other}"
                        )));
                    }
                };
            }
            _ if flag.starts_with('-') => return Ok(false),
            _ if self.cart.is_none() => self.cart = Some(PathBuf::from(flag)),
            _ => {
                return Err(LaunchError::Usage(
                    "only one positional cart path is supported".to_owned(),
                ));
            }
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        Model::A5200Ntsc.frame_ticks()
    }

    fn query_provider(&self) -> Atari5200SessionQueryProvider {
        Atari5200SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Atari5200Runtime, LaunchError> {
        if self.cart.is_none() {
            return Err(LaunchError::Run(
                "provide a cartridge with --cart PATH".to_owned(),
            ));
        }
        let runtime = self.build_mcp_runtime()?;
        if !runtime.bios_loaded() {
            eprintln!(
                "warning: no 5200 BIOS found; pass --bios PATH or stage bios.rom / 5200.rom in the BIOS directory"
            );
        }
        Ok(runtime)
    }

    /// BIOS lookup is shared with normal launch; MCP may wait for a cartridge.
    fn build_mcp_runtime(&self) -> Result<Atari5200Runtime, LaunchError> {
        build_variant::<Atari5200Runtime>(Model::A5200Ntsc, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.cart else {
            return Ok(Vec::new());
        };
        Ok(vec![(
            "cartridge-1".to_owned(),
            MediaKind::Cartridge,
            read_rom(path, "--cart")?,
        )])
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &Atari5200Runtime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("bios_loaded".to_owned(), runtime.bios_loaded().into());
        report.insert("cart_loaded".to_owned(), cart_loaded.into());
        report.insert("frames_run".to_owned(), frame_count.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, .. } = parse::<Atari5200>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.cart.is_none());
        assert_eq!(app.frame_ticks(), Model::A5200Ntsc.frame_ticks());
    }

    #[test]
    fn parse_cli_accepts_cart_bios_region_scale_video() {
        let parsed = parse::<Atari5200>(&args(&[
            "--cart", "game.a52", "--bios", "5200.rom", "--region", "ntsc", "--scale", "4",
            "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.a52")));
        assert_eq!(
            app.firmware.by_id.get(runtime_atari_5200::BIOS_FIRMWARE_ID),
            Some(&PathBuf::from("5200.rom"))
        );
        assert_eq!(app.frame_ticks(), Model::A5200Ntsc.frame_ticks());
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let Parsed::Run { app, .. } = parse::<Atari5200>(&args(&["game.a52"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.a52")));
    }

    #[test]
    fn pal_is_refused_with_the_reason() {
        let err = parse::<Atari5200>(&args(&["--region", "pal"])).expect_err("rejects");
        assert!(matches!(err, LaunchError::Usage(msg) if msg.contains("NTSC only")));
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::A5200Ntsc.frame_ticks(), 262 * 228);
    }
}
