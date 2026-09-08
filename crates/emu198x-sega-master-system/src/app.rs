//! The Sega Master System as a [`MachineApp`]: its flags, runtime, battery
//! save, and report fields.

use std::fs;
use std::path::{Path, PathBuf};

use emu198x_shell::HeadlessSession;
use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, build_variant};
use runtime_sega_master_system::{Model, SmsRuntime, SmsSessionQueryProvider};
use serde_json::{Map, Value};

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct MasterSystem {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    pub model: Model,
}

impl Default for MasterSystem {
    fn default() -> Self {
        Self {
            cart: None,
            model: Model::SmsNtsc,
        }
    }
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
    --model ID      canonical model id or full profile id (alias: --variant)
    --variant KIND  sms-ntsc | sms-japan-ntsc | sms-pal | sms1-ntsc | sms1-pal [default: sms-ntsc]
                    sms1 selects the early 315-5124 VDP";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      d-pad (player 1)
    Z / X           buttons 1 and 2
    Enter           Pause";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
            "--variant" | "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "{flag} expects {}, got {id}",
                        Model::VARIANT_IDS.join("|")
                    ))
                })?;
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
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> SmsSessionQueryProvider {
        SmsSessionQueryProvider
    }

    /// The cartridge, with its battery save restored from `<cart>.sav` when
    /// one is there.
    fn build_runtime(&self) -> Result<SmsRuntime, LaunchError> {
        let cart_path = self.cart_path()?;
        let cart = read_rom(cart_path, "--cart")?;
        let mut runtime = build_variant::<SmsRuntime>(self.model, &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime.insert_cartridge(cart);
        load_battery_save(&mut runtime, &default_battery_save_path(cart_path))?;
        Ok(runtime)
    }

    /// Use the same parsed cartridge and battery-save startup as normal launch.
    /// Without a cartridge MCP starts empty and waits for load_media.
    fn build_mcp_runtime(&self) -> Result<SmsRuntime, LaunchError> {
        if self.cart.is_some() {
            return self.build_runtime();
        }
        build_variant::<SmsRuntime>(self.model, &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        // build_mcp_runtime already loaded the parsed cartridge (and its SRAM).
        self.startup_media()
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
    use runtime_sega_master_system::with_cartridge;

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
        assert_eq!(app.model, Model::SmsNtsc);
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
        assert_eq!(app.model, Model::SmsPal);
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
        assert_eq!(app.model, Model::SmsPal);
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
        assert_eq!(app.model, Model::SmsNtsc);
    }

    #[test]
    fn variant_frame_ticks_match() {
        assert_eq!(Model::SmsNtsc.frame_ticks(), 228 * 262);
        assert_eq!(Model::SmsPal.frame_ticks(), 228 * 313);
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
