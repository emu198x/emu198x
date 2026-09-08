//! The Jupiter Ace as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::MediaKind;
use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, FirmwareResolveError, build_variant};
use runtime_jupiter_ace::{JupiterAceRuntime, JupiterAceSessionQueryProvider, Model};
use serde_json::{Map, Value};

/// Z80 @ 3.25 MHz PAL: 312 lines x 208 T-states = 64,896 T-states/frame.
/// Derived from the machine so it cannot drift: a budget longer than
/// run_frame() makes the harness run two machine frames per displayed frame
/// (~2x too fast). See docs/status/ui-boot-verification.
#[cfg(test)]
pub const FRAME_TICKS: u64 = Model::Ace3k.frame_ticks();

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct JupiterAce {
    pub firmware: FirmwareOverrides,
    pub model: Model,
    /// `--ace PATH`: an ACE32 snapshot restored before a script runs.
    pub ace: Option<PathBuf>,
}

impl Default for JupiterAce {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            model: Model::Ace3k,
            ace: None,
        }
    }
}

impl MachineApp for JupiterAce {
    type Runtime = JupiterAceRuntime;
    type Query = JupiterAceSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-jupiter-ace";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH|ID=PATH Jupiter Ace Forth ROM (8 KB); default
                    ~/.emu198x/roms/jupiter-ace/ace.rom
                    (or set EMU198X_JUPITER_ACE_ROM)
    --rom-dir DIR   firmware directory (or set EMU198X_JUPITER_ACE_ROM_DIR)
                    firmware ID: jupiter-ace-rom
    --model ID      jupiter-ace-3k | jupiter-ace-16k | jupiter-ace-48k
    --ram-kb N      legacy preset selection: <16 stock, 16..47 16 KiB expansion,
                    ≥48 48 KiB expansion [default: stock]; last selector wins
    --ace PATH      restore an ACE32 .ace snapshot before running";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 Space   the Ace keyboard
    Shift           CAPS SHIFT (hold with another key)
    Ctrl            SYMBOL SHIFT (the red symbol layer)
    Enter           ENTER";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model.variant_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown jupiter-ace model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--ram-kb" => {
                self.model = Model::from_ram_kb(args.parse(flag, "a non-negative integer")?)
            }
            "--ace" => self.ace = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> JupiterAceSessionQueryProvider {
        JupiterAceSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<JupiterAceRuntime, LaunchError> {
        build_variant::<JupiterAceRuntime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    /// Conventional firmware is loaded when available. Missing conventional
    /// firmware permits blank startup; invalid images and explicit paths fail.
    fn build_mcp_runtime(&self) -> Result<JupiterAceRuntime, LaunchError> {
        match build_variant::<JupiterAceRuntime>(self.model, &self.firmware) {
            Ok(runtime) => Ok(runtime),
            Err(
                err @ (FirmwareResolveError::HomeUnset
                | FirmwareResolveError::NoRomDir { .. }
                | FirmwareResolveError::Missing { .. }),
            ) if self.firmware.dir.is_none()
                && self.firmware.by_id.is_empty()
                && std::env::var_os("EMU198X_JUPITER_ACE_ROM_DIR").is_none() =>
            {
                eprintln!("{} mcp: {err} — starting blank", Self::BIN_NAME);
                Ok(JupiterAceRuntime::blank(self.model))
            }
            Err(err) => Err(LaunchError::Run(err.to_string())),
        }
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.ace else {
            return Ok(Vec::new());
        };
        let bytes = read_rom(path, "--ace")?;
        Ok(vec![("snapshot-1".to_owned(), MediaKind::Snapshot, bytes)])
    }

    fn report(&self, runtime: &JupiterAceRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("ram_kb".to_owned(), runtime.model().ram_kb().into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::HeadlessSession;
    use emu198x_shell::launch::{Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn flags_set_rom_and_ram() {
        let parsed =
            parse::<JupiterAce>(&args(&["--rom", "ace.rom", "--ram-kb", "16"])).expect("parses");
        let Parsed::Run { app, .. } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_jupiter_ace::BIOS_FIRMWARE_ID),
            Some(&PathBuf::from("ace.rom"))
        );
        assert_eq!(app.model, Model::Ace16k);
    }

    #[test]
    fn model_selects_by_ram() {
        assert_eq!(Model::from_ram_kb(3), Model::Ace3k);
        assert_eq!(Model::from_ram_kb(16), Model::Ace16k);
        assert_eq!(Model::from_ram_kb(48), Model::Ace48k);
    }

    #[test]
    fn native_budget_runs_exact_requested_frame_count() {
        let runtime =
            JupiterAceRuntime::new(Model::Ace3k, vec![0; 8 * 1024]).expect("valid test ROM");
        let mut session = HeadlessSession::new(runtime, FRAME_TICKS);

        session.run_frames(1).expect("first frame");
        assert_eq!(
            session.machine().machine().expect("machine").frame_count(),
            1
        );

        session.run_frames(3).expect("three more frames");
        assert_eq!(
            session.machine().machine().expect("machine").frame_count(),
            4
        );
    }
}
