//! The ZX81 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp};
use emu198x_shell::{FirmwareOverrides, build_variant};
use runtime_sinclair_zx81::{Model, ROM_FIRMWARE_ID, Zx81Runtime, Zx81SessionQueryProvider};
use serde_json::{Map, Value};

/// Frame budget for the board the runtime is configured as. The 60 Hz strap
/// lays out a much shorter field, so this cannot be one shared constant.
pub fn frame_ticks(model: Model) -> u64 {
    u64::from(model.television_standard().slow_mode_frame_tstates())
}

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Zx81 {
    pub rom: Option<PathBuf>,
    pub ram_bytes: Option<usize>,
    pub model: Model,
}

impl Default for Zx81 {
    fn default() -> Self {
        Self {
            rom: None,
            ram_bytes: None,
            model: Model::Zx81,
        }
    }
}

impl Zx81 {
    fn firmware_overrides(&self) -> FirmwareOverrides {
        let mut overrides = FirmwareOverrides::none();
        if let Some(path) = &self.rom {
            overrides.pin(ROM_FIRMWARE_ID, path);
        }
        overrides
    }

    fn apply_ram_override(&self, runtime: &mut Zx81Runtime) -> Result<(), LaunchError> {
        if let Some(bytes) = self.ram_bytes {
            if !bytes.is_power_of_two() || bytes > 16_384 {
                return Err(LaunchError::Usage(
                    "--ram-bytes must be a power of two between 1 and 16384".to_owned(),
                ));
            }
            runtime
                .set_ram_bytes(bytes)
                .map_err(|err| LaunchError::Run(format!("invalid --ram-bytes: {err}")))?;
        }
        Ok(())
    }

    /// The model selected at launch.
    pub const fn model(&self) -> Model {
        self.model
    }
}

impl MachineApp for Zx81 {
    type Runtime = Zx81Runtime;
    type Query = Zx81SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sinclair-zx81";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --rom PATH      ZX81 monitor ROM (8 KB); default
                    ~/.emu198x/roms/sinclair-zx81/zx81.rom (or set EMU198X_ZX81_ROM)
    --model ID      sinclair-zx81, sinclair-zx81-16k, or timex-ts1000
                    [default: sinclair-zx81]
    --ram-bytes N   RAM size (power-of-two ≤ 16384) [default: model RAM]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 . Space the ZX81 membrane keyboard
    Shift           SHIFT (the function/symbol layer — hold with another key)
    Enter           NEWLINE";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self.rom = Some(args.path(flag)?),
            "--ram-bytes" => self.ram_bytes = Some(args.parse(flag, "a positive integer")?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown ZX81 model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        frame_ticks(self.model())
    }

    fn query_provider(&self) -> Zx81SessionQueryProvider {
        Zx81SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Zx81Runtime, LaunchError> {
        let mut runtime = build_variant::<Zx81Runtime>(self.model(), &self.firmware_overrides())
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.apply_ram_override(&mut runtime)?;
        Ok(runtime)
    }

    /// MCP can start without firmware, allowing a client to load it later.
    fn build_mcp_runtime(&self) -> Result<Zx81Runtime, LaunchError> {
        let mut runtime =
            match build_variant::<Zx81Runtime>(self.model(), &self.firmware_overrides()) {
                Ok(runtime) => runtime,
                Err(err) => {
                    eprintln!("{} mcp: {err} — starting blank", Self::BIN_NAME);
                    Zx81Runtime::blank(self.model())
                }
            };
        self.apply_ram_override(&mut runtime)?;
        Ok(runtime)
    }

    fn report(&self, runtime: &Zx81Runtime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("ram_bytes".to_owned(), runtime.ram_bytes().into());
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
        let Parsed::Run { app, .. } = parse::<Zx81>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.ram_bytes, None);
        assert_eq!(app.model(), Model::Zx81);
    }

    #[test]
    fn parse_cli_accepts_rom_scale_video() {
        let parsed = parse::<Zx81>(&args(&[
            "--rom", "zx81.rom", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.rom, Some(PathBuf::from("zx81.rom")));
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }
}
