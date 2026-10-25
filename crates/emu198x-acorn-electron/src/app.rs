//! The Acorn Electron, with or without the Plus 3, as a [`MachineApp`]: its
//! flags, runtime, and report fields.

#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp};
use emu198x_shell::{
    FirmwareOverrides, MediaKind, build_variant, build_variant_or_blank, read_media_asset,
};
use runtime_acorn_electron::{ElectronRuntime, ElectronSessionQueryProvider, Model};
use serde_json::{Map, Value};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Electron {
    /// `--model ID`: the plain Electron unless told otherwise.
    pub model: Model,
    pub firmware: FirmwareOverrides,
    /// `--disk PATH` and `--disk-1 PATH`: ADFS discs for the Plus 3's drives.
    pub disks: [Option<PathBuf>; 2],
}

impl MachineApp for Electron {
    type Runtime = ElectronRuntime;
    type Query = ElectronSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-acorn-electron";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --model ID      acorn-electron | acorn-electron-plus3 (the Plus 3 disc
                    expansion: a WD1770, two 3.5\" drives and ADFS)
    --os PATH       Electron OS ROM (16 KB); default
                    ~/.emu198x/roms/acorn-electron/os.rom (or set EMU198X_ELECTRON_OS)
    --basic PATH    BBC BASIC II ROM (16 KB); default
                    ~/.emu198x/roms/acorn-electron/basic.rom
                    (or set EMU198X_ELECTRON_BASIC)
    --adfs PATH     Plus 3 ADFS 1.00 ROM (16 KB, paged ROM 4); default
                    ~/.emu198x/roms/acorn-electron/adfs.rom
                    (or set EMU198X_ELECTRON_ADFS); implies the Plus 3
    --disk PATH     ADFS .adf or .adl disc for Plus 3 drive 0; implies the
                    Plus 3. Type *CAT, or hold SHIFT and press BREAK to boot it
    --disk-1 PATH   ADFS .adf or .adl disc for drive 1
    --rom ID=PATH   pin acorn-electron-os, acorn-electron-basic or
                    acorn-electron-adfs (repeatable)
    --rom-dir DIR   firmware directory (or set EMU198X_ELECTRON_ROM_DIR)";
    const CONTROLS: &'static str = "    Close window    quit (or use the platform Quit shortcut)
    F12             hard reset
    A-Z 0-9 etc.    the Electron keyboard (cursor keys are real Electron keys)
    Shift / Ctrl    the Electron SHIFT / CTRL keys; Alt = FUNC; End = COPY";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--os" | "--basic" => {
                let id = if flag == "--os" {
                    runtime_acorn_electron::OS_FIRMWARE_ID
                } else {
                    runtime_acorn_electron::BASIC_FIRMWARE_ID
                };
                self.firmware.by_id.insert(id.to_owned(), args.path(flag)?);
            }
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown Electron model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            // The ADFS ROM and the discs only exist on a Plus 3, so asking for
            // one fits it.
            "--adfs" => {
                self.model = Model::ElectronPlus3;
                self.firmware.by_id.insert(
                    runtime_acorn_electron::ADFS_FIRMWARE_ID.to_owned(),
                    args.path(flag)?,
                );
            }
            "--disk" | "--disk-1" => {
                self.model = Model::ElectronPlus3;
                let drive = usize::from(flag == "--disk-1");
                self.disks[drive] = Some(args.path(flag)?);
            }
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    Model::ElectronPlus3.variant_id(),
                    &Model::ElectronPlus3.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> ElectronSessionQueryProvider {
        ElectronSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<ElectronRuntime, LaunchError> {
        self.check_model()?;
        build_variant::<ElectronRuntime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn build_mcp_runtime(&self) -> Result<ElectronRuntime, LaunchError> {
        self.check_model()?;
        build_variant_or_blank(self.model, &self.firmware, ElectronRuntime::blank)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    /// The discs named by `--disk` and `--disk-1`.
    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let mut media = Vec::new();
        for (path, slot) in self.disks.iter().zip(runtime_acorn_electron::DISK_SLOTS) {
            let Some(path) = path else { continue };
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

    fn report(&self, runtime: &ElectronRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("model".to_owned(), runtime.model().profile_id().into());
        report.insert("roms_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        if runtime.model().has_plus3() {
            let plus3 = runtime.machine().and_then(|m| m.plus3());
            report.insert(
                "adfs_loaded".to_owned(),
                plus3.is_some_and(|p| p.has_adfs()).into(),
            );
            report.insert(
                "discs_loaded".to_owned(),
                (0..2)
                    .filter(|&d| plus3.is_some_and(|p| p.disk(d).is_some()))
                    .count()
                    .into(),
            );
        }
    }
}

impl Electron {
    /// Refuse Plus 3 firmware pinned with `--rom` on a plain Electron, where
    /// nothing would use it.
    fn check_model(&self) -> Result<(), LaunchError> {
        if !self.model.has_plus3()
            && self
                .firmware
                .by_id
                .contains_key(runtime_acorn_electron::ADFS_FIRMWARE_ID)
        {
            return Err(LaunchError::Usage(
                "acorn-electron-adfs needs the Plus 3; add --model acorn-electron-plus3".to_owned(),
            ));
        }
        Ok(())
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
        let Parsed::Run { app, .. } = parse::<Electron>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.firmware.by_id.is_empty());
        assert_eq!(app.model, Model::Electron);
    }

    #[test]
    fn adfs_and_disks_fit_the_plus3() {
        let Parsed::Run { app, .. } = parse::<Electron>(&args(&[
            "--adfs",
            "/tmp/adfs.rom",
            "--disk",
            "/tmp/a.adf",
            "--disk-1",
            "/tmp/b.adl",
        ]))
        .expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.model, Model::ElectronPlus3);
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_electron::ADFS_FIRMWARE_ID)
                .map(PathBuf::as_path),
            Some(Path::new("/tmp/adfs.rom"))
        );
        assert_eq!(app.disks[0].as_deref(), Some(Path::new("/tmp/a.adf")));
        assert_eq!(app.disks[1].as_deref(), Some(Path::new("/tmp/b.adl")));

        let Parsed::Run { app, .. } =
            parse::<Electron>(&args(&["--model", "acorn-electron-plus3"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.model, Model::ElectronPlus3);
        assert!(parse::<Electron>(&args(&["--model", "acorn-atom"])).is_err());
    }

    #[test]
    fn adfs_pinned_on_a_plain_electron_is_refused() {
        let Parsed::Run { app, .. } =
            parse::<Electron>(&args(&["--rom", "acorn-electron-adfs=/tmp/adfs.rom"]))
                .expect("parses")
        else {
            panic!("expected a run");
        };
        assert!(matches!(app.check_model(), Err(LaunchError::Usage(_))));
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<Electron>(&args(&[
            "--os",
            "/tmp/os",
            "--basic",
            "/tmp/basic",
            "--frames",
            "30",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_electron::OS_FIRMWARE_ID)
                .expect("parsed by CLI"),
            Path::new("/tmp/os")
        );
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_electron::BASIC_FIRMWARE_ID)
                .expect("parsed by CLI"),
            Path::new("/tmp/basic")
        );
        assert_eq!(common.frames, 30);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_os_basic_scale_video() {
        let parsed = parse::<Electron>(&args(&[
            "--os",
            "os.rom",
            "--basic",
            "basic.rom",
            "--scale",
            "2",
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
                .get(runtime_acorn_electron::OS_FIRMWARE_ID),
            Some(&PathBuf::from("os.rom"))
        );
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_electron::BASIC_FIRMWARE_ID),
            Some(&PathBuf::from("basic.rom"))
        );
        assert_eq!(common.scale, Some(2));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }
}
