//! The Mattel Aquarius as a [`MachineApp`]: its flags, runtime, and report
//! fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant, build_variant_or_blank};
use runtime_mattel_aquarius::{AquariusRuntime, AquariusSessionQueryProvider, Model};
use serde_json::{Map, Value};

#[cfg(test)]
pub const FRAME_TICKS: u64 = Model::Aquarius.frame_ticks();

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Aquarius {
    pub firmware: FirmwareOverrides,
    pub cart: Option<PathBuf>,
    pub expansion_kb: usize,
}

impl MachineApp for Aquarius {
    type Runtime = AquariusRuntime;
    type Query = AquariusSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-mattel-aquarius";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --bios PATH     Aquarius BASIC ROM (8 KB); default
                    ~/.emu198x/roms/mattel-aquarius/aquarius.rom
                    (or set EMU198X_AQUARIUS_BIOS)
    --char PATH     Aquarius character ROM (2 KB); default
                    ~/.emu198x/roms/mattel-aquarius/aquarius-char.rom
                    (or set EMU198X_AQUARIUS_CHAR)
    --rom ID=PATH   pin mattel-aquarius-rom or mattel-aquarius-char-rom (repeatable)
    --rom-dir DIR   firmware directory (or set EMU198X_AQUARIUS_ROM_DIR)
    --cart PATH     cartridge ROM (mapped at $E000-$FFFF, up to 8 KB)
    --expansion-kb N
                    RAM expansion in KB (0..=16) [default: 0]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Aquarius keyboard
    Shift / Ctrl    the two Aquarius shift keys
    Arrow keys      hand controller disc (player 1)
    Alt             hand controller fire";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--bios" | "--char" => {
                let id = if flag == "--bios" {
                    runtime_mattel_aquarius::BIOS_FIRMWARE_ID
                } else {
                    runtime_mattel_aquarius::CHAR_FIRMWARE_ID
                };
                self.firmware.by_id.insert(id.to_owned(), args.path(flag)?);
            }
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    Model::Aquarius.variant_id(),
                    &Model::Aquarius.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--cart" => self.cart = Some(args.path(flag)?),
            "--expansion-kb" => self.expansion_kb = args.parse(flag, "a non-negative integer")?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        Model::Aquarius.frame_ticks()
    }

    fn query_provider(&self) -> AquariusSessionQueryProvider {
        AquariusSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<AquariusRuntime, LaunchError> {
        let mut runtime = build_variant::<AquariusRuntime>(Model::Aquarius, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime.set_expansion_kb(self.expansion_kb);
        Ok(runtime)
    }

    fn build_mcp_runtime(&self) -> Result<AquariusRuntime, LaunchError> {
        let mut runtime =
            build_variant_or_blank(Model::Aquarius, &self.firmware, AquariusRuntime::blank)
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime.set_expansion_kb(self.expansion_kb);
        Ok(runtime)
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

    fn report(&self, runtime: &AquariusRuntime, report: &mut Map<String, Value>) {
        let bios_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("bios_loaded".to_owned(), bios_loaded.into());
        report.insert("cart_loaded".to_owned(), runtime.cartridge_loaded().into());
        report.insert("frames_run".to_owned(), frame_count.into());
        report.insert("expansion_kb".to_owned(), runtime.expansion_kb().into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::HeadlessSession;

    #[test]
    fn one_budgeted_frame_is_one_machine_frame() {
        let runtime = AquariusRuntime::new(Model::Aquarius, vec![0; 8 * 1024]).expect("blank BIOS");
        let mut session = HeadlessSession::new(runtime, FRAME_TICKS);
        session.run_frames(1).expect("one frame");
        assert_eq!(
            session.machine().machine().expect("machine").frame_count(),
            1
        );
        session.run_frames(4).expect("four more");
        assert_eq!(
            session.machine().machine().expect("machine").frame_count(),
            5
        );
    }
    use emu198x_shell::launch::{Mode, Parsed, parse};
    use std::path::Path;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, .. } = parse::<Aquarius>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.firmware.by_id.is_empty());
        assert!(app.cart.is_none());
        assert_eq!(app.expansion_kb, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<Aquarius>(&args(&[
            "--bios",
            "/tmp/aq.rom",
            "--cart",
            "/tmp/game",
            "--expansion-kb",
            "16",
            "--frames",
            "60",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_mattel_aquarius::BIOS_FIRMWARE_ID)
                .map(PathBuf::as_path),
            Some(Path::new("/tmp/aq.rom"))
        );
        assert_eq!(app.expansion_kb, 16);
        assert_eq!(common.frames, 60);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_bios_char_cart_expansion_scale_video() {
        let parsed = parse::<Aquarius>(&args(&[
            "--bios",
            "aq.rom",
            "--char",
            "aq-char.rom",
            "--cart",
            "game.bin",
            "--expansion-kb",
            "16",
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
            app.firmware
                .by_id
                .get(runtime_mattel_aquarius::BIOS_FIRMWARE_ID),
            Some(&PathBuf::from("aq.rom"))
        );
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_mattel_aquarius::CHAR_FIRMWARE_ID),
            Some(&PathBuf::from("aq-char.rom"))
        );
        assert_eq!(app.cart, Some(PathBuf::from("game.bin")));
        assert_eq!(app.expansion_kb, 16);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_expansion_is_a_usage_error() {
        let err = parse::<Aquarius>(&args(&["--expansion-kb", "lots"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage(
                "--expansion-kb expects a non-negative integer, got lots".to_owned()
            )
        );
    }
}
