//! The Sega Mega Drive / Genesis as a [`MachineApp`]: its flags, runtime
//! and report fields.

use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, build_variant};
use runtime_sega_mega_drive::cartridge::{header_model, normalise};
use runtime_sega_mega_drive::{MegaDriveRuntime, MegaDriveSessionQueryProvider, Model};
use serde_json::{Map, Value};

/// The model when neither the flags nor the cartridge choose one.
const DEFAULT_MODEL: Model = Model::Genesis;

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MegaDrive {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    /// `--model ID`, when given.
    pub model: Option<Model>,
}

impl MegaDrive {
    fn cart_path(&self) -> Result<&Path, LaunchError> {
        self.cart
            .as_deref()
            .ok_or_else(|| LaunchError::Run("provide a cartridge with --cart PATH".to_owned()))
    }

    /// The model to run: the one asked for, else the one the cartridge's
    /// header names, else the Genesis. The header is read from the file
    /// here, so the launcher's frame budget and the runtime agree.
    pub fn model(&self) -> Model {
        self.model.unwrap_or_else(|| {
            self.cart
                .as_deref()
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|image| header_model(&normalise(image)))
                .unwrap_or(DEFAULT_MODEL)
        })
    }
}

impl MachineApp for MegaDrive {
    type Runtime = MegaDriveRuntime;
    type Query = MegaDriveSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sega-mega-drive";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     cartridge image, raw (.bin/.md/.gen) or .smd
                    (required; a bare PATH is accepted too)
    --model ID      mega-drive-japan | genesis | mega-drive-europe, or a
                    full profile id (alias: --variant) [default: the
                    cartridge header's region, else genesis]";
    const CONTROLS: &'static str = "    Close window    quit (or use the platform Quit shortcut)
    F12             emulator hard reset
    Arrow keys      d-pad (player 1)
    Z / X / C       A, B and C
    A / S / D       X, Y and Z (six-button pad)
    Right Shift     Mode (six-button pad)
    Enter           Start";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
            "--variant" | "--model" => {
                let id = args.value(flag)?;
                self.model = Some(Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "{flag} expects {}, got {id}",
                        Model::VARIANT_IDS.join("|")
                    ))
                })?);
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
        self.model().frame_ticks()
    }

    fn query_provider(&self) -> MegaDriveSessionQueryProvider {
        MegaDriveSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<MegaDriveRuntime, LaunchError> {
        let cart = read_rom(self.cart_path()?, "--cart")?;
        let mut runtime =
            build_variant::<MegaDriveRuntime>(self.model(), &FirmwareOverrides::none())
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime.insert_cartridge(cart);
        Ok(runtime)
    }

    /// Without a cartridge MCP starts empty and waits for load_media.
    fn build_mcp_runtime(&self) -> Result<MegaDriveRuntime, LaunchError> {
        if self.cart.is_some() {
            return self.build_runtime();
        }
        build_variant::<MegaDriveRuntime>(self.model(), &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        // build_mcp_runtime already loaded the parsed cartridge.
        self.startup_media()
    }

    fn report(&self, runtime: &MegaDriveRuntime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.vdp.frames());
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
        let Parsed::Run { app, common, .. } = parse::<MegaDrive>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.cart.is_none());
        assert_eq!(app.model, None);
        assert_eq!(app.model(), Model::Genesis);
        assert_eq!(common.frames, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<MegaDrive>(&args(&[
            "--cart",
            "/tmp/cart.md",
            "--model",
            "mega-drive-europe",
            "--frames",
            "60",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/cart.md")));
        assert_eq!(app.model, Some(Model::MegaDriveEurope));
        assert_eq!(common.frames, 60);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_positional_cart_and_variant_alias() {
        let Parsed::Run { app, mode, .. } =
            parse::<MegaDrive>(&args(&["sonic.gen", "--variant", "mega-drive-japan"]))
                .expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("sonic.gen")));
        assert_eq!(app.model, Some(Model::MegaDriveJapan));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn variant_frame_ticks_match() {
        let app = |model| MegaDrive {
            cart: None,
            model: Some(model),
        };
        assert_eq!(app(Model::Genesis).frame_ticks(), 262 * 3420);
        assert_eq!(app(Model::MegaDriveEurope).frame_ticks(), 313 * 3420);
    }

    /// The header chooses only when the flags do not.
    #[test]
    fn the_cartridge_header_picks_the_model_unless_one_is_given() {
        let path = std::env::temp_dir().join(format!(
            "emu198x-md-header-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should follow Unix epoch")
                .as_nanos()
        ));
        let mut rom = vec![0x20; 0x200];
        rom[0x1F0] = b'E';
        std::fs::write(&path, rom).expect("write cartridge");
        let mut app = MegaDrive {
            cart: Some(path.clone()),
            model: None,
        };
        assert_eq!(app.model(), Model::MegaDriveEurope);
        assert_eq!(app.frame_ticks(), Model::MegaDriveEurope.frame_ticks());
        app.model = Some(Model::MegaDriveJapan);
        assert_eq!(app.model(), Model::MegaDriveJapan);
        std::fs::remove_file(path).expect("temporary cartridge should be removable");
    }
}
