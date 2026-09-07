//! The Commodore VIC-20 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::fs;
use std::path::{Path, PathBuf};

use emu198x_shell::launch::{
    Args, LaunchError, MachineApp, conventional_rom_path, read_rom, read_rom_exact,
};
use emu198x_shell::{HeadlessSession, MediaKind};
use runtime_commodore_vic_20::{Model, Vic20RamExpansion, Vic20Runtime, Vic20SessionQueryProvider};
use serde_json::{Map, Value};

/// VIC cycles per frame — `cols × lines`.
const FRAME_TICKS_PAL: u64 = 71 * 312;
const FRAME_TICKS_NTSC: u64 = 65 * 261;

/// Display region — selects the model, frame tick budget, and refresh rate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Region {
    Ntsc,
    #[default]
    Pal,
}

impl Region {
    pub const fn model(self) -> Model {
        match self {
            Self::Ntsc => Model::Vic20Ntsc,
            Self::Pal => Model::Vic20Pal,
        }
    }

    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::Ntsc => FRAME_TICKS_NTSC,
            Self::Pal => FRAME_TICKS_PAL,
        }
    }

    #[cfg(feature = "ui")]
    pub fn frame_hz(self) -> f64 {
        match self {
            Self::Ntsc => 60.0,
            Self::Pal => 50.0,
        }
    }
}

/// One of the three VIC-20 ROM images: the label used in errors, its flag,
/// the `EMU198X_VIC20_<kind>` variable, its file under `~/.emu198x/roms/`,
/// and the size the machine requires.
struct Rom {
    label: &'static str,
    flag: &'static str,
    env: &'static str,
    relative: &'static str,
    size: usize,
}

impl Rom {
    /// `explicit` from the command line, else the conventional location.
    fn path(&self, explicit: Option<&Path>) -> Option<PathBuf> {
        explicit
            .map(Path::to_path_buf)
            .or_else(|| conventional_rom_path(self.env, self.relative))
    }

    /// The image, which must be exactly `size` bytes.
    fn read(&self, explicit: Option<&Path>) -> Result<Vec<u8>, LaunchError> {
        let path = self.path(explicit).ok_or_else(|| {
            LaunchError::Run(format!(
                "no {} ROM: pass {} or set {}",
                self.label, self.flag, self.env
            ))
        })?;
        read_rom_exact(&path, &format!("{} ROM", self.label), self.size)
    }
}

const KERNAL: Rom = Rom {
    label: "KERNAL",
    flag: "--kernal",
    env: "EMU198X_VIC20_KERNAL",
    relative: "commodore-vic-20/kernal.rom",
    size: 8 * 1024,
};
const BASIC: Rom = Rom {
    label: "BASIC",
    flag: "--basic",
    env: "EMU198X_VIC20_BASIC",
    relative: "commodore-vic-20/basic.rom",
    size: 8 * 1024,
};
const CHAR: Rom = Rom {
    label: "character",
    flag: "--char",
    env: "EMU198X_VIC20_CHAR",
    relative: "commodore-vic-20/chargen.rom",
    size: 4 * 1024,
};

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Vic20 {
    pub kernal: Option<PathBuf>,
    pub basic: Option<PathBuf>,
    pub char_rom: Option<PathBuf>,
    pub region: Region,
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
            kernal: None,
            basic: None,
            char_rom: None,
            region: Region::Pal,
            ram_expansion: Vic20RamExpansion::NONE,
            prg: None,
            prg_sys: false,
            esp_at_tcp: false,
        }
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
            "--kernal" => self.kernal = Some(args.path(flag)?),
            "--basic" => self.basic = Some(args.path(flag)?),
            "--char" => self.char_rom = Some(args.path(flag)?),
            "--region" => {
                self.region = match args.value(flag)?.as_str() {
                    "ntsc" => Region::Ntsc,
                    "pal" => Region::Pal,
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
        self.region.frame_ticks()
    }

    fn query_provider(&self) -> Vic20SessionQueryProvider {
        Vic20SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Vic20Runtime, LaunchError> {
        let kernal = KERNAL.read(self.kernal.as_deref())?;
        let basic = BASIC.read(self.basic.as_deref())?;
        let char_rom = CHAR.read(self.char_rom.as_deref())?;
        let mut runtime = Vic20Runtime::new(self.region.model(), kernal, basic, char_rom)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))?;
        runtime.set_ram_expansion(self.ram_expansion);
        if self.esp_at_tcp {
            // The external modem keeps real baud time while the VIC-I CPU clock
            // differs by region: ~115 PAL cycles or ~107 NTSC cycles at 9600.
            let cycles_per_bit = match self.region {
                Region::Pal => 115,
                Region::Ntsc => 107,
            };
            runtime.attach_esp_at_tcp_bridge(cycles_per_bit, 64);
        }
        Ok(runtime)
    }

    /// MCP starts blank and takes all three ROMs from their conventional
    /// locations when every one is there and the right size; otherwise a
    /// client hands it firmware later.
    fn build_mcp_runtime(&self) -> Result<Vic20Runtime, LaunchError> {
        let mut runtime = Vic20Runtime::blank(self.region.model());
        let read = |rom: &Rom, explicit: Option<&Path>| fs::read(rom.path(explicit)?).ok();
        let (Some(kernal), Some(basic), Some(char_rom)) = (
            read(&KERNAL, self.kernal.as_deref()),
            read(&BASIC, self.basic.as_deref()),
            read(&CHAR, self.char_rom.as_deref()),
        ) else {
            return Ok(runtime);
        };
        if kernal.len() == KERNAL.size && basic.len() == BASIC.size && char_rom.len() == CHAR.size {
            runtime
                .set_roms(kernal, basic, char_rom)
                .map_err(|err| LaunchError::Run(format!("ROMs invalid: {err}")))?;
            eprintln!("{} mcp: loaded all 3 ROMs", Self::BIN_NAME);
        } else {
            eprintln!("{} mcp: ROM sizes wrong; starting blank", Self::BIN_NAME);
        }
        Ok(runtime)
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

    /// `--prg-sys` remains the explicit machine-code side channel: boot to
    /// READY, then inject and SYS the program.
    fn after_prepare(
        &self,
        session: &mut HeadlessSession<Vic20Runtime, Vic20SessionQueryProvider>,
    ) -> Result<(), LaunchError> {
        let Some(path) = self.prg.as_deref().filter(|_| self.prg_sys) else {
            return Ok(());
        };
        let bytes = read_rom(path, "--prg")?;
        session
            .run_frames(150)
            .map_err(|err| LaunchError::Run(format!("boot-to-READY run failed: {err}")))?;
        session
            .machine_mut()
            .autoload_prg(&bytes, true)
            .map_err(|err| LaunchError::Run(format!("PRG autoload failed: {err}")))
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
        assert_eq!(app.region, Region::Pal);
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
        assert_eq!(app.kernal, Some(PathBuf::from("k.rom")));
        assert_eq!(app.region, Region::Ntsc);
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
        assert_eq!(Region::Pal.frame_ticks(), 71 * 312);
        assert_eq!(Region::Ntsc.frame_ticks(), 65 * 261);
    }
}
