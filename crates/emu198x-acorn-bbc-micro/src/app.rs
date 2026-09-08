//! The BBC Micro as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, build_variant, build_variant_or_blank, build_variant_with};
use runtime_acorn_bbc_micro::{BbcMicroRuntime, BbcMicroSessionQueryProvider, Model};
use serde_json::{Map, Value};

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bbc {
    pub firmware: FirmwareOverrides,
    /// `--sideways BANK=PATH`, repeatable: ROMs to install in banks 0..=15.
    pub sideways: Vec<(usize, PathBuf)>,
}

impl Bbc {
    /// Install the `--sideways` banks.
    pub(crate) fn insert_sideways_roms(
        &self,
        runtime: &mut BbcMicroRuntime,
    ) -> Result<(), LaunchError> {
        for (bank, path) in &self.sideways {
            let rom = read_rom(path, &format!("sideways ROM bank {bank}"))?;
            runtime.insert_sideways_rom(*bank, rom);
        }
        Ok(())
    }

    /// Resolve MOS/font once; selecting the default language is a launch policy.
    pub(crate) fn build_with_basic(&self) -> Result<BbcMicroRuntime, LaunchError> {
        build_variant_with::<BbcMicroRuntime>(Model::BbcModelB, &self.firmware, |firmware| {
            BbcMicroRuntime::from_firmware_with_basic(Model::BbcModelB, firmware)
        })
        .map_err(|err| LaunchError::Run(err.to_string()))
    }
}

fn parse_sideways(spec: &str) -> Result<(usize, PathBuf), LaunchError> {
    let Some((bank_str, path_str)) = spec.split_once('=') else {
        return Err(LaunchError::Usage(
            "--sideways expects BANK=PATH".to_owned(),
        ));
    };
    let bank: usize = bank_str
        .parse()
        .map_err(|_| LaunchError::Usage("--sideways bank must be an integer 0..=15".to_owned()))?;
    if bank > 15 {
        return Err(LaunchError::Usage(
            "--sideways bank must be 0..=15".to_owned(),
        ));
    }
    Ok((bank, PathBuf::from(path_str)))
}

impl MachineApp for Bbc {
    type Runtime = BbcMicroRuntime;
    type Query = BbcMicroSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-acorn-bbc-micro";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --mos PATH      BBC MOS ROM (16 KB); default
                    ~/.emu198x/roms/acorn-bbc-micro/os.rom (or set EMU198X_BBC_MOS)
    --sideways BANK=PATH    install a sideways ROM into bank 0..=15 (repeatable);
                    headless runs boot the bare MOS unless one is given, the
                    window also installs a staged BASIC into bank 15
    --rom ID=PATH   pin MOS or optional firmware:
                    acorn-bbc-mos, acorn-bbc-saa5050, acorn-bbc-basic
                    an explicit BASIC pin also selects bank 15 in headless modes
    --rom-dir DIR   firmware directory (or set EMU198X_BBC_ROM_DIR)";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the BBC keyboard (cursor keys are real BBC keys)
    Shift / Ctrl    the BBC SHIFT / CTRL keys; F1-F10 are the red f0-f9 keys
    Gamepad         joystick fire (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--mos" => self
                .firmware
                .pin(runtime_acorn_bbc_micro::MOS_FIRMWARE_ID, args.path(flag)?),
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    Model::BbcModelB.variant_id(),
                    &Model::BbcModelB.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--sideways" => self.sideways.push(parse_sideways(&args.value(flag)?)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        Model::BbcModelB.frame_ticks()
    }

    fn query_provider(&self) -> BbcMicroSessionQueryProvider {
        BbcMicroSessionQueryProvider
    }

    /// Headless callers explicitly select their language through a pin or bank.
    fn build_runtime(&self) -> Result<BbcMicroRuntime, LaunchError> {
        let mut runtime = if self
            .firmware
            .by_id
            .contains_key(runtime_acorn_bbc_micro::BASIC_FIRMWARE_ID)
        {
            self.build_with_basic()?
        } else {
            build_variant::<BbcMicroRuntime>(Model::BbcModelB, &self.firmware)
                .map_err(|err| LaunchError::Run(err.to_string()))?
        };
        self.insert_sideways_roms(&mut runtime)?;
        Ok(runtime)
    }

    fn build_mcp_runtime(&self) -> Result<BbcMicroRuntime, LaunchError> {
        if self
            .firmware
            .by_id
            .contains_key(runtime_acorn_bbc_micro::BASIC_FIRMWARE_ID)
        {
            return self.build_runtime();
        }
        let mut runtime =
            build_variant_or_blank(Model::BbcModelB, &self.firmware, BbcMicroRuntime::blank)
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.insert_sideways_roms(&mut runtime)?;
        Ok(runtime)
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &BbcMicroRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("mos_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert(
            "sideways_count".to_owned(),
            runtime
                .machine()
                .map_or(0, |machine| machine.sideways_rom_count())
                .into(),
        );
        report.insert("frames_run".to_owned(), frames_run.into());
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
        let Parsed::Run { app, .. } = parse::<Bbc>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.firmware.by_id.is_empty());
        assert!(app.sideways.is_empty());
    }

    #[test]
    fn parse_cli_accepts_mos_scale_video() {
        let parsed = parse::<Bbc>(&args(&[
            "--mos", "mos.rom", "--scale", "2", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_bbc_micro::MOS_FIRMWARE_ID),
            Some(&PathBuf::from("mos.rom"))
        );
        assert_eq!(common.scale, Some(2));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn sideways_is_repeatable_and_bank_checked() {
        let Parsed::Run { app, .. } = parse::<Bbc>(&args(&[
            "--sideways",
            "15=basic.rom",
            "--sideways",
            "14=dfs.rom",
        ]))
        .expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(
            app.sideways,
            vec![
                (15, PathBuf::from("basic.rom")),
                (14, PathBuf::from("dfs.rom"))
            ]
        );

        let usage = |list: &[&str]| parse::<Bbc>(&args(list)).expect_err("rejects");
        assert_eq!(
            usage(&["--sideways", "basic.rom"]),
            LaunchError::Usage("--sideways expects BANK=PATH".to_owned())
        );
        assert_eq!(
            usage(&["--sideways", "x=basic.rom"]),
            LaunchError::Usage("--sideways bank must be an integer 0..=15".to_owned())
        );
        assert_eq!(
            usage(&["--sideways", "16=basic.rom"]),
            LaunchError::Usage("--sideways bank must be 0..=15".to_owned())
        );
    }
}
