//! The BBC Micro Model B and B+ as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{
    FirmwareOverrides, MediaKind, build_variant, build_variant_or_blank, build_variant_with,
    read_media_asset,
};
use runtime_acorn_bbc_micro::{BbcMicroRuntime, BbcMicroSessionQueryProvider, Model};
use serde_json::{Map, Value};

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bbc {
    /// `--model ID`: which BBC to build. The Model B unless told otherwise.
    pub model: Model,
    pub firmware: FirmwareOverrides,
    /// `--disk PATH` and `--disk-1 PATH`: DFS discs for drives 0 and 1 on a
    /// B+.
    pub disks: [Option<PathBuf>; 2],
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
        build_variant_with::<BbcMicroRuntime>(self.model, &self.firmware, |firmware| {
            BbcMicroRuntime::from_firmware_with_basic(self.model, firmware)
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
    const MACHINE_OPTIONS: &'static str =
        "    --model ID      acorn-bbc-micro-b | acorn-bbc-micro-b-plus; put it
                    before --mos and --rom so they name the right firmware
    --mos PATH      BBC MOS ROM (16 KB); default
                    ~/.emu198x/roms/acorn-bbc-micro/os.rom (or set EMU198X_BBC_MOS),
                    or bplus-os.rom on a B+ (or set EMU198X_BBC_BPLUS_OS)
    --sideways BANK=PATH    install a sideways ROM into bank 0..=15 (repeatable);
                    headless runs boot the bare MOS unless one is given, the
                    window also installs a staged BASIC into bank 15
    --rom ID=PATH   pin MOS or optional firmware:
                    acorn-bbc-mos, acorn-bbc-saa5050, acorn-bbc-basic; on a B+
                    acorn-bbc-bplus-mos and acorn-bbc-1770-dfs (default
                    dfs-2.26.rom, or set EMU198X_BBC_DFS), fitted in bank 14
                    an explicit BASIC pin also selects bank 15 in headless modes
    --disk PATH     .ssd or .dsd disc for drive 0 on a B+. With the DFS, type
                    *CAT, or hold SHIFT and press BREAK to boot it
    --disk-1 PATH   .ssd or .dsd disc for drive 1
    --rom-dir DIR   firmware directory (or set EMU198X_BBC_ROM_DIR)";
    const CONTROLS: &'static str = "    Close window    quit (or use the platform Quit shortcut)
    F12             hard reset
    A-Z 0-9 etc.    the BBC keyboard (cursor keys are real BBC keys)
    Shift / Ctrl    the BBC SHIFT / CTRL keys; F1-F10 are the red f0-f9 keys
    Gamepad         joystick fire (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--mos" => self
                .firmware
                .pin(self.model.mos_firmware_id(), args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown BBC model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--disk" => self.disks[0] = Some(args.path(flag)?),
            "--disk-1" => self.disks[1] = Some(args.path(flag)?),
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model.variant_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--sideways" => self.sideways.push(parse_sideways(&args.value(flag)?)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
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
            build_variant::<BbcMicroRuntime>(self.model, &self.firmware)
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
            build_variant_or_blank(self.model, &self.firmware, BbcMicroRuntime::blank)
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.insert_sideways_roms(&mut runtime)?;
        Ok(runtime)
    }

    /// The discs named by `--disk` and `--disk-1`.
    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let mut media = Vec::new();
        for (path, slot) in self.disks.iter().zip(runtime_acorn_bbc_micro::DISK_SLOTS) {
            let Some(path) = path else { continue };
            if !self.model.has_disc() {
                return Err(LaunchError::Usage(format!(
                    "{} has no disc drive; choose acorn-bbc-micro-b-plus with --model",
                    self.model.display_name()
                )));
            }
            let loaded = read_media_asset(path, MediaKind::Disk).map_err(|err| {
                LaunchError::Run(format!("failed to load disc {}: {err}", path.display()))
            })?;
            media.push((slot.to_owned(), MediaKind::Disk, loaded.bytes));
        }
        Ok(media)
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
        report.insert("model".to_owned(), runtime.model().profile_id().into());
        report.insert("mos_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert(
            "discs_loaded".to_owned(),
            (0..2)
                .filter(|&drive| runtime.machine().is_some_and(|m| m.disk(drive).is_some()))
                .count()
                .into(),
        );
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

    #[test]
    fn parse_cli_picks_the_b_plus_and_its_discs() {
        let Parsed::Run { app, .. } = parse::<Bbc>(&args(&[
            "--model",
            "acorn-bbc-micro-b-plus",
            "--mos",
            "bplus-os.rom",
            "--rom",
            "acorn-bbc-1770-dfs=dfs.rom",
            "--disk",
            "games.ssd",
            "--disk-1",
            "work.dsd",
        ]))
        .expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.model, Model::BbcBPlus);
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_bbc_micro::BPLUS_MOS_FIRMWARE_ID),
            Some(&PathBuf::from("bplus-os.rom"))
        );
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_bbc_micro::DFS_FIRMWARE_ID),
            Some(&PathBuf::from("dfs.rom"))
        );
        assert_eq!(
            app.disks,
            [
                Some(PathBuf::from("games.ssd")),
                Some(PathBuf::from("work.dsd"))
            ]
        );
    }

    #[test]
    fn an_unknown_model_is_named_and_a_model_b_disc_refused() {
        let err = parse::<Bbc>(&args(&["--model", "acorn-bbc-micro-c"]))
            .expect_err("refused")
            .to_string();
        assert!(err.contains("acorn-bbc-micro-b-plus"), "{err}");
        let app = Bbc {
            disks: [Some(PathBuf::from("a.ssd")), None],
            ..Bbc::default()
        };
        let err = app
            .startup_media()
            .expect_err("a Model B has no drive")
            .to_string();
        assert!(err.contains("no disc drive"), "{err}");
    }
}
