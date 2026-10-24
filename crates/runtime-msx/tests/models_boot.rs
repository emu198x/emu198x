//! Every model in the catalogue boots real firmware on its own slot layout.
//!
//! The BIOS is [C-BIOS](https://github.com/cbios/cbios), the clean-room MSX
//! BIOS. It finds its RAM by probing every slot and subslot, so a model whose
//! layout the machine decodes wrongly never reaches its screen. One BIOS
//! serves every model here: the claim is about the layout, not about each
//! machine's own firmware.
//!
//! Provisioned from the corpora store; `EMU198X_ROMS_ROOT` points at the
//! firmware root, and this joins `microsoft-msx/` onto it.

use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_msx::{Model, MsxRuntime};

/// TMS9918 entry 4 — the blue C-BIOS boots to.
const MSX_BLUE: u32 = 0xFF54_55ED;

fn bios() -> Option<PathBuf> {
    let root = std::env::var_os("EMU198X_ROMS_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms")))?;
    Some(root.join("microsoft-msx/cbios_main_msx1.rom"))
}

#[test]
#[ignore = "FIXTURE: needs C-BIOS at <EMU198X_ROMS_ROOT>/microsoft-msx/cbios_main_msx1.rom"]
fn every_msx1_model_boots_cbios_to_its_screen() {
    let Some(path) = bios() else {
        emu198x_test_skip::skip!("neither EMU198X_ROMS_ROOT nor HOME is set");
    };
    if !path.exists() {
        emu198x_test_skip::skip!("C-BIOS not staged at {}", path.display());
    }
    let rom = std::fs::read(&path).expect("C-BIOS should read");

    for model in Model::ALL.into_iter().filter(|model| !model.is_msx2()) {
        let mut runtime = MsxRuntime::new(model, rom.clone()).expect("a 32 KB BIOS");
        runtime
            .run_until(
                MachineTime::new(model.frame_ticks() * 200),
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut NullFrameSink,
                    audio_sink: &mut NullAudioSink,
                    trace_sink: &mut NullTraceSink,
                },
            )
            .expect("the runtime should run");
        let framebuffer = runtime.machine().expect("a machine").framebuffer();
        let blue = framebuffer
            .iter()
            .filter(|&&pixel| pixel == MSX_BLUE)
            .count();
        let drawn = framebuffer.len() - blue;
        assert!(
            blue * 100 / framebuffer.len() > 80 && drawn > 500,
            "{} should reach the C-BIOS screen; {blue} blue and {drawn} other pixels",
            model.model_id()
        );
    }
}

/// Every MSX2 model boots C-BIOS's MSX2 build, without its logo ROM, to
/// the text screen that says there is no cartridge. C-BIOS probes for the
/// SUB-ROM and for mapped or plain RAM, so this covers the three layouts.
#[test]
#[ignore = "FIXTURE: needs C-BIOS at <EMU198X_ROMS_ROOT>/microsoft-msx/cbios_{main_msx2,main_msx2_eu,sub}.rom"]
fn every_msx2_model_boots_cbios_to_its_screen() {
    let Some(dir) = bios().and_then(|path| path.parent().map(PathBuf::from)) else {
        emu198x_test_skip::skip!("neither EMU198X_ROMS_ROOT nor HOME is set");
    };
    let read = |name: &str| std::fs::read(dir.join(name));
    let (Ok(ntsc), Ok(pal), Ok(sub)) = (
        read("cbios_main_msx2.rom"),
        read("cbios_main_msx2_eu.rom"),
        read("cbios_sub.rom"),
    ) else {
        emu198x_test_skip::skip!("C-BIOS MSX2 not staged in {}", dir.display());
    };

    for model in Model::ALL.into_iter().filter(|model| model.is_msx2()) {
        let bios = match model.region() {
            emu198x_shell::Region::Pal => pal.clone(),
            _ => ntsc.clone(),
        };
        let mut runtime = MsxRuntime::new(model, bios).expect("a 32 KB BIOS");
        assert!(runtime.machine().is_none(), "an MSX2 waits for its SUB-ROM");
        runtime.set_sub_rom(sub.clone()).expect("a 16 KB SUB-ROM");
        runtime
            .run_until(
                MachineTime::new(model.frame_ticks() * 200),
                &mut HostIo {
                    input_events: &[],
                    frame_sink: &mut NullFrameSink,
                    audio_sink: &mut NullAudioSink,
                    trace_sink: &mut NullTraceSink,
                },
            )
            .expect("the runtime should run");
        let machine = runtime.machine().expect("a machine");
        let mut pixels = machine.framebuffer().to_vec();
        pixels.sort_unstable();
        let runs: Vec<usize> = pixels.chunk_by(|a, b| a == b).map(<[u32]>::len).collect();
        let ground = runs.iter().copied().max().unwrap_or(0) * 100 / pixels.len();
        assert!(
            runs.len() == 2 && (80..99).contains(&ground),
            "{} should reach the C-BIOS text screen; {} colours, ground {ground}%",
            model.model_id(),
            runs.len()
        );
        assert!(machine.v9938().is_some());
    }
}
