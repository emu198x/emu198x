//! The Commodore VIC-20 as a [`MachineApp`]: its flags, runtime, and report fields.

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{
    FirmwareOverrides, HeadlessSession, MediaKind, build_variant, build_variant_or_blank,
};
use runtime_commodore_vic_20::{
    BASIC_FIRMWARE_ID, CHAR_FIRMWARE_ID, KERNAL_FIRMWARE_ID, Model, Vic20RamExpansion,
    Vic20Runtime, Vic20SessionQueryProvider,
};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Vic20 {
    pub firmware: FirmwareOverrides,
    pub model: Model,
    pub ram_expansion: Vic20RamExpansion,
    /// `--prg PATH`: a program injected after boot and auto-RUN.
    pub prg: Option<PathBuf>,
    /// `--prg-sys`: launch the program with SYS (machine code) instead of RUN.
    pub prg_sys: bool,
    /// `--esp-at-tcp`: attach an ESP-AT modem with a real TCP bridge.
    pub esp_at_tcp: bool,
}

impl Default for Vic20 {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            model: Model::Vic20Pal,
            ram_expansion: Vic20RamExpansion::NONE,
            prg: None,
            prg_sys: false,
            esp_at_tcp: false,
        }
    }
}

impl Vic20 {
    fn configure_runtime(&self, mut runtime: Vic20Runtime) -> Result<Vic20Runtime, LaunchError> {
        runtime.set_ram_expansion(self.ram_expansion);
        if self.esp_at_tcp {
            let cycles_per_bit = match self.model {
                Model::Vic20Pal => 115,
                Model::Vic20Ntsc => 107,
            };
            runtime.attach_esp_at_tcp_bridge(cycles_per_bit, 64);
        }
        if let Some(path) = self.prg.as_deref().filter(|_| self.prg_sys) {
            let bytes = read_rom(path, "--prg")?;
            let mut session = HeadlessSession::new_with_query_provider(
                runtime,
                self.frame_ticks(),
                Vic20SessionQueryProvider,
            );
            session
                .run_frames(150)
                .map_err(|err| LaunchError::Run(format!("boot-to-READY run failed: {err}")))?;
            session
                .machine_mut()
                .autoload_prg(&bytes, true)
                .map_err(|err| LaunchError::Run(format!("PRG autoload failed: {err}")))?;
            return Ok(session.into_machine());
        }
        Ok(runtime)
    }
}

impl MachineApp for Vic20 {
    type Runtime = Vic20Runtime;
    type Query = Vic20SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-commodore-vic-20";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --kernal PATH   KERNAL ROM (8 KB)
    --basic PATH    BASIC ROM (8 KB)
    --char PATH     character ROM (4 KB)
                    ROM defaults: $EMU198X_VIC20_{KERNAL,BASIC,CHAR}, then
                    ~/.emu198x/roms/commodore-vic-20/{kernal,basic,chargen}.rom
    --rom ID=PATH   pin one of the three catalogue ROM images
    --rom-dir DIR   firmware directory (or EMU198X_VIC20_ROM_DIR)
    --model ID      commodore-vic-20-ntsc | commodore-vic-20-pal
    --region MODE   ntsc | pal [default: pal]
    --ram-expansion SPEC
                    RAM expansion cartridges [default: none]
                    none | 3k | 8k | 16k | 24k, or a DIP-switched
                    8k@2000 | 8k@4000 | 8k@6000 | 8k@a000, joined
                    with + (3k+8k, 8k@2000+8k@6000)
    --prg PATH      inject a .PRG after boot and auto-RUN it. A
                    canonical BASIC load address fits what it needs
                    ($0401 -> 3k, $1201 -> BLK1); cartridges already
                    named by --ram-expansion are kept
    --prg-sys       launch the --prg with SYS <load-addr> (machine
                    code) instead of RUN (BASIC)
    --esp-at-tcp    attach an ESP-AT modem (9600 baud initially,
                    with AT+UART_CUR support); CIPSTART opens a real
                    TCP connection (64-byte frame reassembly)";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the VIC-20 keyboard
    Right / Down    the two cursor keys; Tab = RUN/STOP, Alt = Commodore
    Gamepad         joystick (single control port)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--kernal" => self.firmware.pin(KERNAL_FIRMWARE_ID, args.path(flag)?),
            "--basic" => self.firmware.pin(BASIC_FIRMWARE_ID, args.path(flag)?),
            "--char" => self.firmware.pin(CHAR_FIRMWARE_ID, args.path(flag)?),
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model.variant_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown VIC-20 model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--region" => {
                self.model = match args.value(flag)?.as_str() {
                    "ntsc" => Model::Vic20Ntsc,
                    "pal" => Model::Vic20Pal,
                    other => {
                        return Err(LaunchError::Usage(format!(
                            "--region expects ntsc|pal, got {other}"
                        )));
                    }
                };
            }
            "--ram-expansion" => {
                let spec = args.value(flag)?;
                self.ram_expansion = Vic20RamExpansion::parse(&spec).map_err(|reason| {
                    LaunchError::Usage(format!("--ram-expansion {spec}: {reason}"))
                })?;
            }
            // Removed in #1363. The old value folded the 3K expander and the
            // block cartridges into one number, so silently reinterpreting it
            // would change which RAM a script gets.
            "--ram-expansion-kb" => {
                return Err(LaunchError::Usage(
                    "--ram-expansion-kb was removed because it could not express a plain 8K \
                     cartridge; use --ram-expansion (none|3k|8k|16k|24k, e.g. 3k+8k for what \
                     --ram-expansion-kb 11 used to mean)"
                        .to_owned(),
                ));
            }
            "--prg" => self.prg = Some(args.path(flag)?),
            "--prg-sys" => self.prg_sys = true,
            "--esp-at-tcp" => self.esp_at_tcp = true,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> Vic20SessionQueryProvider {
        Vic20SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Vic20Runtime, LaunchError> {
        let runtime = build_variant::<Vic20Runtime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.configure_runtime(runtime)
    }

    fn build_mcp_runtime(&self) -> Result<Vic20Runtime, LaunchError> {
        let runtime =
            build_variant_or_blank::<Vic20Runtime>(self.model, &self.firmware, Vic20Runtime::blank)
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.configure_runtime(runtime)
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    /// Ordinary BASIC PRGs use the standard media path; the runtime delays
    /// their injection until the KERNAL has reached READY.
    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = self.prg.as_deref().filter(|_| !self.prg_sys) else {
            return Ok(Vec::new());
        };
        let bytes = read_rom(path, "--prg")?;
        Ok(vec![("program-1".to_owned(), MediaKind::Program, bytes)])
    }

    fn report(&self, runtime: &Vic20Runtime, report: &mut Map<String, Value>) {
        let roms_loaded = runtime.machine().is_some();
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        let bridge = runtime.esp_at_tcp_bridge();
        report.insert("roms_loaded".to_owned(), roms_loaded.into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert(
            "ram_expansion".to_owned(),
            runtime.ram_expansion().to_string().into(),
        );
        report.insert(
            "ram_expansion_kb".to_owned(),
            runtime.ram_expansion_kb().into(),
        );
        report.insert(
            "esp_at_tcp_error".to_owned(),
            bridge.and_then(|bridge| bridge.last_error()).into(),
        );
        report.insert(
            "esp_at_received_hex".to_owned(),
            bridge
                .map(|bridge| {
                    bridge
                        .diagnostic_received()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .into(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn app(list: &[&str]) -> Vic20 {
        match parse::<Vic20>(&args(list)).expect("parses") {
            Parsed::Run { app, .. } => app,
            Parsed::Help => panic!("expected a run"),
        }
    }

    #[test]
    fn defaults_to_pal_with_no_expansion() {
        let Parsed::Run { app, mode, .. } = parse::<Vic20>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.model, Model::Vic20Pal);
        assert_eq!(app.ram_expansion, Vic20RamExpansion::NONE);
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_set_roms_region_scale_video() {
        let parsed = parse::<Vic20>(&args(&[
            "--kernal",
            "k.rom",
            "--region",
            "ntsc",
            "--scale",
            "2",
            "--video",
            "crt",
            "--prg",
            "game.prg",
            "--prg-sys",
            "--esp-at-tcp",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware.by_id.get(KERNAL_FIRMWARE_ID),
            Some(&PathBuf::from("k.rom"))
        );
        assert_eq!(app.model, Model::Vic20Ntsc);
        assert_eq!(app.prg, Some(PathBuf::from("game.prg")));
        assert!(app.prg_sys);
        assert!(app.esp_at_tcp);
        assert_eq!(common.scale, Some(2));
        assert_eq!(common.video.as_deref(), Some("crt"));
        // A bare `--region` is shared with the UI, so it opens the window.
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn ram_expansion_flag_accepts_hardware_shaped_specs() {
        for (spec, expected) in [
            ("none", Vic20RamExpansion::NONE),
            ("3k", Vic20RamExpansion::EXP_3K),
            ("8k", Vic20RamExpansion::EXP_8K),
            ("16k", Vic20RamExpansion::EXP_16K),
            ("24k", Vic20RamExpansion::EXP_24K),
        ] {
            assert_eq!(
                app(&["--ram-expansion", spec]).ram_expansion,
                expected,
                "{spec}"
            );
        }

        let module = app(&["--ram-expansion", "3k+8k"]);
        assert!(module.ram_expansion.exp_3k && module.ram_expansion.blk1);

        let dipped = app(&["--ram-expansion", "8k@4000"]);
        assert!(dipped.ram_expansion.blk2 && !dipped.ram_expansion.blk1);
    }

    #[test]
    fn removed_ram_expansion_kb_flag_is_a_usage_error() {
        let err = parse::<Vic20>(&args(&["--ram-expansion-kb", "11"])).expect_err("rejects");
        assert!(matches!(err, LaunchError::Usage(message) if message.contains("--ram-expansion")));
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::Vic20Pal.frame_ticks(), 71 * 312);
        assert_eq!(Model::Vic20Ntsc.frame_ticks(), 65 * 261);
    }
}
