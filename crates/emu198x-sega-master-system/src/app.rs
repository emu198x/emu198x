//! The Sega Master System as a [`MachineApp`]: its flags, runtime, battery
//! save, and report fields.

use std::fs;
use std::path::{Path, PathBuf};

use emu198x_shell::HeadlessSession;
use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::register_base_tools;
use runtime_sega_master_system::{
    Model, SmsRuntime, SmsSessionQueryProvider, blank, with_cartridge,
};
use serde_json::{Map, Value};

/// CPU clocks per frame — `228 × lines`.
const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;
#[cfg(feature = "ui")]
const NTSC_FRAME_HZ: f64 = 60.0;
#[cfg(feature = "ui")]
const PAL_FRAME_HZ: f64 = 50.0;

/// Console variant — selects the model, frame tick budget and refresh rate.
/// The Game Gear ships from `emu198x-sega-game-gear` (#998).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Variant {
    #[default]
    SmsNtsc,
    SmsPal,
    Sms1Ntsc,
    Sms1Pal,
}

impl Variant {
    pub const fn model(self) -> Model {
        match self {
            Self::SmsNtsc => Model::SmsNtsc,
            Self::SmsPal => Model::SmsPal,
            Self::Sms1Ntsc => Model::Sms1Ntsc,
            Self::Sms1Pal => Model::Sms1Pal,
        }
    }

    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::SmsPal | Self::Sms1Pal => FRAME_TICKS_PAL,
            Self::SmsNtsc | Self::Sms1Ntsc => FRAME_TICKS_NTSC,
        }
    }

    #[cfg(feature = "ui")]
    pub const fn frame_hz(self) -> f64 {
        match self {
            Self::SmsPal | Self::Sms1Pal => PAL_FRAME_HZ,
            Self::SmsNtsc | Self::Sms1Ntsc => NTSC_FRAME_HZ,
        }
    }
}

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MasterSystem {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    pub variant: Variant,
}

impl MasterSystem {
    fn cart_path(&self) -> Result<&Path, LaunchError> {
        self.cart
            .as_deref()
            .ok_or_else(|| LaunchError::Run("provide a cartridge with --cart PATH".to_owned()))
    }
}

impl MachineApp for MasterSystem {
    type Runtime = SmsRuntime;
    type Query = SmsSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sega-master-system";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     cartridge ROM (required; a bare PATH is accepted too)
    --variant KIND  sms-ntsc | sms-pal | sms1-ntsc | sms1-pal [default: sms-ntsc]
                    sms1 selects the early 315-5124 VDP";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      d-pad (player 1)
    Z / X           buttons 1 and 2
    Enter           Pause";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
            "--variant" => {
                self.variant = match args.value(flag)?.as_str() {
                    "sms-ntsc" | "sms" => Variant::SmsNtsc,
                    "sms-pal" => Variant::SmsPal,
                    "sms1-ntsc" | "sms1" => Variant::Sms1Ntsc,
                    "sms1-pal" => Variant::Sms1Pal,
                    other => {
                        return Err(LaunchError::Usage(format!(
                            "--variant expects sms-ntsc|sms-pal|sms1-ntsc|sms1-pal, got {other}"
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
        self.variant.frame_ticks()
    }

    fn query_provider(&self) -> SmsSessionQueryProvider {
        SmsSessionQueryProvider
    }

    /// The cartridge, with its battery save restored from `<cart>.sav` when
    /// one is there.
    fn build_runtime(&self) -> Result<SmsRuntime, LaunchError> {
        let cart_path = self.cart_path()?;
        let cart = read_rom(cart_path, "--cart")?;
        let mut runtime = with_cartridge(self.variant.model(), cart);
        load_battery_save(&mut runtime, &default_battery_save_path(cart_path))?;
        Ok(runtime)
    }

    /// MCP starts blank; the cartridge arrives via load_media.
    fn build_mcp_runtime(&self) -> Result<SmsRuntime, LaunchError> {
        Ok(blank(self.variant.model()))
    }

    /// Write the battery save back once the script and frames have run.
    /// This sits before the captures where the old runner wrote it after
    /// them; the ordering does not matter, as the save image comes from the
    /// runtime and the captures from the session's frame and audio buffers.
    fn after_run(
        &self,
        session: &mut HeadlessSession<SmsRuntime, SmsSessionQueryProvider>,
    ) -> Result<(), LaunchError> {
        let save_path = default_battery_save_path(self.cart_path()?);
        write_battery_save(session.machine(), &save_path)
    }

    fn report(&self, runtime: &SmsRuntime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("cart_loaded".to_owned(), cart_loaded.into());
        report.insert("frames_run".to_owned(), frame_count.into());
    }

    /// The Master System has no keyboard: the base tools only.
    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<SmsRuntime, SmsSessionQueryProvider>>,
    ) {
        register_base_tools(registry);
    }
}

/// The battery save lives beside the cartridge as `<cart>.sav`.
pub fn default_battery_save_path(cart_path: &Path) -> PathBuf {
    let mut path = cart_path.to_path_buf();
    path.set_extension("sav");
    path
}

fn load_battery_save(runtime: &mut SmsRuntime, path: &Path) -> Result<(), LaunchError> {
    match fs::read(path) {
        Ok(bytes) => runtime.restore_cartridge_save_image(&bytes).map_err(|err| {
            LaunchError::Run(format!(
                "failed to restore battery save {}: {err}",
                path.display()
            ))
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(LaunchError::Run(format!(
            "failed to read battery save {}: {err}",
            path.display()
        ))),
    }
}

fn write_battery_save(runtime: &SmsRuntime, path: &Path) -> Result<(), LaunchError> {
    let Some(image) = runtime.cartridge_save_image() else {
        return Ok(());
    };
    fs::write(path, image).map_err(|err| {
        LaunchError::Run(format!(
            "failed to write battery save {}: {err}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn temporary_save_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "emu198x-sms-save-{}-{}.sav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should follow Unix epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, common, .. } = parse::<MasterSystem>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.cart.is_none());
        assert_eq!(app.variant, Variant::SmsNtsc);
        assert_eq!(common.frames, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<MasterSystem>(&args(&[
            "--cart",
            "/tmp/cart",
            "--variant",
            "sms-pal",
            "--frames",
            "60",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/cart")));
        assert_eq!(app.variant, Variant::SmsPal);
        assert_eq!(common.frames, 60);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_cart_variant_scale_video() {
        let parsed = parse::<MasterSystem>(&args(&[
            "--cart",
            "sonic.sms",
            "--variant",
            "sms-pal",
            "--scale",
            "4",
            "--video",
            "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("sonic.sms")));
        assert_eq!(app.variant, Variant::SmsPal);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let Parsed::Run { app, .. } = parse::<MasterSystem>(&args(&["sonic.sms"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("sonic.sms")));
        assert_eq!(app.variant, Variant::SmsNtsc);
    }

    #[test]
    fn variant_frame_ticks_match() {
        assert_eq!(Variant::SmsNtsc.frame_ticks(), 228 * 262);
        assert_eq!(Variant::SmsPal.frame_ticks(), 228 * 313);
    }

    #[test]
    fn battery_save_is_created_only_after_sram_changes_and_loads_cleanly() {
        let path = temporary_save_path();
        let mut runtime = with_cartridge(Model::SmsNtsc, vec![0; 0x10000]);

        write_battery_save(&runtime, &path).expect("clean cartridge should be skipped");
        assert!(!path.exists());

        let machine = runtime.machine_mut().expect("cartridge should be loaded");
        machine.poke(0xFFFC, 0x08);
        machine.poke(0x8123, 0x5A);
        write_battery_save(&runtime, &path).expect("changed SRAM should save");
        assert_eq!(fs::metadata(&path).expect("save should exist").len(), 32768);

        let mut restored = with_cartridge(Model::SmsNtsc, vec![0; 0x10000]);
        load_battery_save(&mut restored, &path).expect("save should load");
        let machine = restored.machine_mut().expect("cartridge should be loaded");
        machine.poke(0xFFFC, 0x08);
        assert_eq!(machine.peek(0x8123), 0x5A);
        assert!(restored.cartridge_save_image().is_none());

        fs::remove_file(path).expect("temporary save should be removable");
    }
}
