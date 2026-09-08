//! The ZX80 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, FirmwareResolveError, MediaKind, build_variant};
use runtime_sinclair_zx80::{Model, Zx80Runtime, Zx80SessionQueryProvider};
use serde_json::{Map, Value};

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Zx80 {
    pub firmware: FirmwareOverrides,
    pub model: Model,
    pub ram_bytes: Option<usize>,
    /// `--tape PATH`: a .o/.80 cassette put in the deck before a script runs.
    pub tape: Option<PathBuf>,
}

impl Default for Zx80 {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            model: Model::Zx80,
            ram_bytes: None,
            tape: None,
        }
    }
}

impl Zx80 {
    fn apply_ram_override(&self, runtime: &mut Zx80Runtime) -> Result<(), LaunchError> {
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
}

impl MachineApp for Zx80 {
    type Runtime = Zx80Runtime;
    type Query = Zx80SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sinclair-zx80";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --rom PATH|ID=PATH   pin the ZX80 monitor ROM (4 KB)
                         firmware ID: sinclair-zx80-rom
    --rom-dir DIR        firmware directory (or set EMU198X_ZX80_ROM_DIR)
                         default directory: ~/.emu198x/roms/sinclair-zx80
                         filename: zx80.rom
                         EMU198X_ZX80_ROM still names one ROM file.
    --model ID           sinclair-zx80, sinclair-zx80-usa, sinclair-zx80-16k
                         [default: sinclair-zx80]
    --ram-bytes N        RAM override (power-of-two ≤ 16384) [default: model RAM]
    --tape PATH          put a .o/.80 cassette in the deck. This does not press
                         play: type LOAD (W), then use `media_transport` start
                         on slot `tape-1`. Switching machines ejects the tape.";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 . Space the ZX80 membrane keyboard
    Shift           SHIFT (the function/symbol layer — hold with another key)
    Enter           NEWLINE";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model.profile_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown ZX80 model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--ram-bytes" => self.ram_bytes = Some(args.parse(flag, "a positive integer")?),
            "--tape" => self.tape = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> Zx80SessionQueryProvider {
        Zx80SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Zx80Runtime, LaunchError> {
        let mut runtime = build_variant::<Zx80Runtime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.apply_ram_override(&mut runtime)?;
        Ok(runtime)
    }

    /// Missing conventional firmware permits blank MCP startup; invalid images
    /// and explicitly requested paths remain errors.
    fn build_mcp_runtime(&self) -> Result<Zx80Runtime, LaunchError> {
        let mut runtime = match build_variant::<Zx80Runtime>(self.model, &self.firmware) {
            Ok(runtime) => runtime,
            Err(
                err @ (FirmwareResolveError::HomeUnset
                | FirmwareResolveError::NoRomDir { .. }
                | FirmwareResolveError::Missing { .. }),
            ) if self.firmware.dir.is_none()
                && self.firmware.by_id.is_empty()
                && std::env::var_os("EMU198X_ZX80_ROM_DIR").is_none() =>
            {
                eprintln!("{} mcp: {err} — starting blank", Self::BIN_NAME);
                Zx80Runtime::blank(self.model)
            }
            Err(err) => return Err(LaunchError::Run(err.to_string())),
        };
        self.apply_ram_override(&mut runtime)?;
        Ok(runtime)
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.tape else {
            return Ok(Vec::new());
        };
        let bytes = read_rom(path, "--tape")?;
        Ok(vec![("tape-1".to_owned(), MediaKind::Tape, bytes)])
    }

    fn report(&self, runtime: &Zx80Runtime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("ram_bytes".to_owned(), runtime.ram_bytes().into());
        report.insert("tape_loaded".to_owned(), runtime.tape_loaded().into());
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
        let Parsed::Run { app, .. } = parse::<Zx80>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.ram_bytes, None);
        assert_eq!(app.model, Model::Zx80);
    }

    #[test]
    fn parse_cli_accepts_rom_scale_video() {
        let parsed = parse::<Zx80>(&args(&[
            "--rom", "zx80.rom", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_sinclair_zx80::ROM_FIRMWARE_ID),
            Some(&PathBuf::from("zx80.rom"))
        );
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_set_ram_bytes_and_tape() {
        let parsed =
            parse::<Zx80>(&args(&["--ram-bytes", "16384", "--tape", "game.o"])).expect("parses");
        let Parsed::Run { app, .. } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.ram_bytes, Some(16384));
        assert_eq!(app.tape, Some(PathBuf::from("game.o")));
    }
}
