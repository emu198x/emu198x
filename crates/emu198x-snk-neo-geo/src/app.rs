//! The SNK Neo Geo AES as a [`MachineApp`]: its flags, runtime and report
//! fields.

use std::io::Write;
use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant, build_variant_or_blank};
use runtime_snk_neo_geo::{
    BIOS_FIRMWARE_ID, Model, NeoGeoRuntime, NeoGeoSessionQueryProvider, ZOOM_FIRMWARE_ID,
};
use serde_json::{Map, Value};

/// Parsed launch options; the runtime owns the model and firmware.
#[derive(Debug, PartialEq, Eq)]
pub struct NeoGeo {
    pub firmware: FirmwareOverrides,
    /// `--cart PATH`, or the one positional argument: a zip of a ROM set, a
    /// directory of its files, or a bare P-ROM.
    pub cart: Option<PathBuf>,
}

impl Default for NeoGeo {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            cart: None,
        }
    }
}

/// The cartridge at `path` as the runtime's cartridge slot takes it. A
/// directory's files are packed into a zip in memory, so a directory and a
/// zip of it load the same way.
///
/// # Errors
///
/// Returns a message when the path or a file in it cannot be read.
pub fn read_cartridge(path: &Path) -> Result<Vec<u8>, LaunchError> {
    if !path.is_dir() {
        return read_rom(path, "--cart");
    }
    let failed = |err: &dyn std::fmt::Display| {
        LaunchError::Run(format!(
            "failed to read cartridge directory {}: {err}",
            path.display()
        ))
    };
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .map_err(|err| failed(&err))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let mut zip = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut zip);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        for entry in entries {
            let bytes = read_rom(&entry.path(), "--cart")?;
            writer
                .start_file(entry.file_name().to_string_lossy(), options)
                .map_err(|err| failed(&err))?;
            writer.write_all(&bytes).map_err(|err| failed(&err))?;
        }
        writer.finish().map_err(|err| failed(&err))?;
    }
    Ok(zip.into_inner())
}

impl MachineApp for NeoGeo {
    type Runtime = NeoGeoRuntime;
    type Query = NeoGeoSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-snk-neo-geo";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --bios PATH     AES BIOS ROM (128 KB, as dumped); default
                    ~/.emu198x/roms/snk-neo-geo/neo-epo.bin or neo-po.bin
                    (or set EMU198X_NEOGEO_BIOS)
    --l0 PATH       L0 shrink-table ROM, 000-lo.lo (optional; without it
                    vertical shrink is approximated)
    --rom PATH|ID=PATH firmware pin (IDs: neo-geo-aes-bios, neo-geo-l0)
    --rom-dir DIR   firmware directory (or set EMU198X_NEOGEO_ROM_DIR)
    --cart PATH     cartridge: a zip of a MAME-named ROM set, a directory
                    of its files, or a bare P-ROM (a bare PATH is
                    accepted too)";
    const CONTROLS: &'static str = "    Close window    quit (or use the platform Quit shortcut)
    Cmd/Ctrl+Shift+R  emulator hard reset
    Cmd/Ctrl+P      pause / resume
    Arrow keys      joystick (player 1)
    Z / X / C / V   A, B, C and D
    Enter           Start
    Right Shift     Select";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--bios" => {
                self.firmware
                    .by_id
                    .insert(BIOS_FIRMWARE_ID.to_owned(), args.path(flag)?);
            }
            "--l0" => {
                self.firmware
                    .by_id
                    .insert(ZOOM_FIRMWARE_ID.to_owned(), args.path(flag)?);
            }
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    Model::Aes.variant_id(),
                    &Model::Aes.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--cart" => self.cart = Some(args.path(flag)?),
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
        Model::Aes.frame_ticks()
    }

    fn query_provider(&self) -> NeoGeoSessionQueryProvider {
        NeoGeoSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<NeoGeoRuntime, LaunchError> {
        build_variant::<NeoGeoRuntime>(Model::Aes, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    /// Without a BIOS, MCP starts blank and waits for one.
    fn build_mcp_runtime(&self) -> Result<NeoGeoRuntime, LaunchError> {
        build_variant_or_blank(Model::Aes, &self.firmware, NeoGeoRuntime::blank)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.cart else {
            return Ok(Vec::new());
        };
        Ok(vec![(
            "cartridge-1".to_owned(),
            MediaKind::Cartridge,
            read_cartridge(path)?,
        )])
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &NeoGeoRuntime, report: &mut Map<String, Value>) {
        report.insert("bios_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("cart_loaded".to_owned(), runtime.cartridge_loaded().into());
        report.insert("frames_run".to_owned(), runtime.frames().into());
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
    fn parse_cli_defaults_and_flags() {
        let Parsed::Run { app, common, .. } = parse::<NeoGeo>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.cart.is_none());
        assert_eq!(common.frames, 0);

        let Parsed::Run { app, common, mode } = parse::<NeoGeo>(&args(&[
            "--bios",
            "/tmp/neo-epo.bin",
            "--l0",
            "/tmp/000-lo.lo",
            "--cart",
            "/tmp/game.zip",
            "--frames",
            "60",
        ]))
        .expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/game.zip")));
        assert_eq!(
            app.firmware
                .by_id
                .get(BIOS_FIRMWARE_ID)
                .map(PathBuf::as_path),
            Some(Path::new("/tmp/neo-epo.bin"))
        );
        assert!(app.firmware.by_id.contains_key(ZOOM_FIRMWARE_ID));
        assert_eq!(common.frames, 60);
        assert_eq!(mode, Mode::Script);
        assert_eq!(app.frame_ticks(), 264 * 384 * 4);
    }

    #[test]
    fn a_positional_cart_is_accepted_once() {
        let Parsed::Run { app, mode, .. } = parse::<NeoGeo>(&args(&["game.zip"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.zip")));
        assert_eq!(mode, Mode::Ui);
        assert!(parse::<NeoGeo>(&args(&["a.zip", "b.zip"])).is_err());
    }

    #[test]
    fn a_directory_packs_into_a_zip_the_runtime_reads() {
        let dir = std::env::temp_dir().join(format!("emu198x-neogeo-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("t-p1.p1"), [0x34, 0x12]).expect("p1");
        std::fs::write(dir.join("t-s1.s1"), [9]).expect("s1");
        let bytes = read_cartridge(&dir).expect("packs");
        let set = format_snk_neo_geo_romset::from_zip(&bytes).expect("reads");
        assert_eq!(set.p, [0x12, 0x34]);
        assert_eq!(set.s, [9]);
        let app = NeoGeo {
            cart: Some(dir.clone()),
            ..NeoGeo::default()
        };
        assert_eq!(app.startup_media().expect("media").len(), 1);
        std::fs::remove_dir_all(dir).expect("cleanup");
        assert!(read_cartridge(Path::new("/nonexistent/emu198x/neo")).is_err());
        assert!(NeoGeo::default().startup_media().expect("none").is_empty());
    }
}
