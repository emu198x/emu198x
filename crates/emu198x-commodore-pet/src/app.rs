//! The Commodore PET as a [`MachineApp`]: its flags, runtime, and report fields.

use std::fs;
use std::path::{Path, PathBuf};

use emu198x_shell::MediaKind;
use emu198x_shell::launch::{
    Args, LaunchError, MachineApp, conventional_rom_path, read_rom, read_rom_exact,
};
use runtime_commodore_pet::{Model, PetRuntime, PetSessionQueryProvider};
use serde_json::{Map, Value};

/// PET 40-col PAL: 6502 @ 1 MHz, 50 Hz → 20,000 cycles/frame.
pub const FRAME_TICKS: u64 = 20_000;

/// One of the four PET ROM images: the label used in errors, its flag, the
/// `EMU198X_PET_<kind>` variable, its file under `~/.emu198x/roms/`, and
/// the size the machine requires.
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
    env: "EMU198X_PET_KERNAL",
    relative: "commodore-pet/kernal.rom",
    size: 4096,
};
const BASIC: Rom = Rom {
    label: "BASIC",
    flag: "--basic",
    env: "EMU198X_PET_BASIC",
    relative: "commodore-pet/basic.rom",
    size: 8192,
};
const EDITOR: Rom = Rom {
    label: "editor",
    flag: "--editor",
    env: "EMU198X_PET_EDITOR",
    relative: "commodore-pet/editor.rom",
    size: 2048,
};
const CHAR: Rom = Rom {
    label: "character",
    flag: "--char",
    env: "EMU198X_PET_CHAR",
    relative: "commodore-pet/chargen.rom",
    size: 4096,
};

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct CommodorePet {
    pub kernal: Option<PathBuf>,
    pub basic: Option<PathBuf>,
    pub editor: Option<PathBuf>,
    pub char_rom: Option<PathBuf>,
    pub columns: u32,
    /// `--prg PATH`: a program loaded after boot and auto-RUN.
    pub prg: Option<PathBuf>,
}

impl Default for CommodorePet {
    fn default() -> Self {
        Self {
            kernal: None,
            basic: None,
            editor: None,
            char_rom: None,
            columns: 40,
            prg: None,
        }
    }
}

pub fn model_for(columns: u32) -> Model {
    match columns {
        80 => Model::Pet80Col,
        _ => Model::Pet40Col,
    }
}

impl MachineApp for CommodorePet {
    type Runtime = PetRuntime;
    type Query = PetSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-commodore-pet";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --kernal PATH   KERNAL ROM (4 KB)
    --basic PATH    BASIC ROM (8 KB)
    --editor PATH   editor ROM (2 KB)
    --char PATH     character ROM (4 KB)
                    ROM defaults: $EMU198X_PET_{KERNAL,BASIC,EDITOR,CHAR}, then
                    ~/.emu198x/roms/commodore-pet/{kernal,basic,editor,chargen}.rom
    --columns N     40 or 80 [default: 40]
    --prg PATH      load a .prg after boot and auto-RUN it";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the PET keyboard
    Enter           RETURN";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--kernal" => self.kernal = Some(args.path(flag)?),
            "--basic" => self.basic = Some(args.path(flag)?),
            "--editor" => self.editor = Some(args.path(flag)?),
            "--char" => self.char_rom = Some(args.path(flag)?),
            "--columns" => self.columns = args.parse(flag, "40 or 80")?,
            "--prg" => self.prg = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS
    }

    fn query_provider(&self) -> PetSessionQueryProvider {
        PetSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<PetRuntime, LaunchError> {
        let kernal = KERNAL.read(self.kernal.as_deref())?;
        let basic = BASIC.read(self.basic.as_deref())?;
        let editor = EDITOR.read(self.editor.as_deref())?;
        let char_rom = CHAR.read(self.char_rom.as_deref())?;
        PetRuntime::new(model_for(self.columns), kernal, basic, editor, char_rom)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank and takes all four ROMs from their conventional
    /// locations when every one is there and the right size; otherwise a
    /// client hands it firmware later.
    fn build_mcp_runtime(&self) -> Result<PetRuntime, LaunchError> {
        let mut runtime = PetRuntime::blank(model_for(self.columns));
        let read = |rom: &Rom, explicit: Option<&Path>| fs::read(rom.path(explicit)?).ok();
        let (Some(kernal), Some(basic), Some(editor), Some(char_rom)) = (
            read(&KERNAL, self.kernal.as_deref()),
            read(&BASIC, self.basic.as_deref()),
            read(&EDITOR, self.editor.as_deref()),
            read(&CHAR, self.char_rom.as_deref()),
        ) else {
            return Ok(runtime);
        };
        if kernal.len() == KERNAL.size
            && basic.len() == BASIC.size
            && editor.len() == EDITOR.size
            && char_rom.len() == CHAR.size
        {
            runtime
                .set_roms(kernal, basic, editor, char_rom)
                .map_err(|err| LaunchError::Run(format!("ROMs invalid: {err}")))?;
            eprintln!("{} mcp: loaded all 4 ROMs", Self::BIN_NAME);
        } else {
            eprintln!("{} mcp: ROM sizes wrong; starting blank", Self::BIN_NAME);
        }
        Ok(runtime)
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.prg else {
            return Ok(Vec::new());
        };
        let bytes = read_rom(path, "--prg")?;
        Ok(vec![("program-1".to_owned(), MediaKind::Program, bytes)])
    }

    fn report(&self, runtime: &PetRuntime, report: &mut Map<String, Value>) {
        let roms_loaded = runtime.machine().is_some();
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("roms_loaded".to_owned(), roms_loaded.into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert(
            "columns".to_owned(),
            model_for(self.columns).screen_chars().into(),
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

    #[test]
    fn defaults_to_forty_columns() {
        let parsed = parse::<CommodorePet>(&[]).expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert!(app.kernal.is_none());
        assert_eq!(app.columns, 40);
        assert_eq!(common.frames, 0);
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_set_roms_columns_scale_video() {
        let parsed = parse::<CommodorePet>(&args(&[
            "--kernal",
            "k.rom",
            "--columns",
            "80",
            "--scale",
            "2",
            "--video",
            "crt",
            "--prg",
            "hello.prg",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.kernal, Some(PathBuf::from("k.rom")));
        assert_eq!(app.columns, 80);
        assert_eq!(app.prg, Some(PathBuf::from("hello.prg")));
        assert_eq!(common.scale, Some(2));
        assert_eq!(common.video.as_deref(), Some("crt"));
        // A bare `--columns` is shared with the UI, so it opens the window.
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_column_count_is_a_usage_error() {
        let err = parse::<CommodorePet>(&args(&["--columns", "forty"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--columns expects 40 or 80, got forty".to_owned())
        );
    }

    #[test]
    fn model_selects_by_columns() {
        assert_eq!(model_for(40), Model::Pet40Col);
        assert_eq!(model_for(80), Model::Pet80Col);
    }
}
