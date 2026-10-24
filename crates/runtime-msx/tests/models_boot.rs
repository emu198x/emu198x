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
fn every_model_boots_cbios_to_its_screen() {
    let Some(path) = bios() else {
        emu198x_test_skip::skip!("neither EMU198X_ROMS_ROOT nor HOME is set");
    };
    if !path.exists() {
        emu198x_test_skip::skip!("C-BIOS not staged at {}", path.display());
    }
    let rom = std::fs::read(&path).expect("C-BIOS should read");

    for model in Model::ALL {
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
