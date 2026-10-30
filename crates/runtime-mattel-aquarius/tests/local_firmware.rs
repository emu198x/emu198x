//! Optional boot evidence through both fixed owners; no ROMs are bundled.
use emu198x_shell::{HostIo, MachineCore, NullAudioSink, NullFrameSink, NullTraceSink};
use runtime_mattel_aquarius::{AquariusRuntime, Model};
#[test]
#[ignore = "FIXTURE: needs local Aquarius BIOS and character ROM in ~/.emu198x/roms/mattel-aquarius"]
fn both_boards_boot_local_basic_firmware() {
    let root = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
        .join(".emu198x/roms/mattel-aquarius");
    let bios = std::fs::read(root.join("aquarius.rom")).expect("BIOS");
    let chars = std::fs::read(root.join("aquarius-char.rom")).expect("characters");
    assert_eq!(bios.len(), 8192);
    assert_eq!(chars.len(), 2048);
    for model in Model::ALL {
        let mut r = AquariusRuntime::new(model, bios.clone()).expect("runtime");
        r.set_char_rom(chars.clone()).expect("characters");
        let mut saw_basic = false;
        let mut saw_prompt = false;
        let mut last = String::new();
        for _ in 0..200 {
            r.run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            })
            .expect("frame");
            let m = r.machine().expect("machine");
            last = (0x3000..0x33e8)
                .map(|a| char::from(m.peek(a) & 0x7f))
                .collect();
            saw_basic |= last.contains("BASIC");
            saw_prompt |= last.to_ascii_uppercase().contains("RETURN");
        }
        assert!(
            saw_basic && saw_prompt,
            "{model:?}: missing BASIC / RETURN screen: {last:?}"
        );
        assert_eq!(
            r.profile().machine_id.as_str(),
            r.machine().expect("machine").board_id()
        );
    }
}
