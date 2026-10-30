//! Optional end-to-end boot through each fixed physical owner, without bundling ROMs.
use emu198x_shell::{HostIo, MachineCore, NullAudioSink, NullFrameSink, NullTraceSink};
use runtime_atari_800xl::{Atari800xlRuntime, Model};

#[test]
#[ignore = "FIXTURE: needs local OS and BASIC ROMs in ~/.emu198x/roms/atari-800xl"]
fn all_four_boards_boot_local_firmware_to_ready() {
    let root = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
        .join(".emu198x/roms/atari-800xl");
    let os = std::fs::read(root.join("atarixl.rom")).expect("local OS");
    let basic = std::fs::read(root.join("ataribas.rom")).expect("local BASIC");
    assert_eq!(os.len(), 16384);
    assert_eq!(basic.len(), 8192);
    for model in Model::ALL {
        let mut r =
            Atari800xlRuntime::new(model, Some(os.clone()), Some(basic.clone()), None, true)
                .expect("firmware");
        for _ in 0..300 {
            r.run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            })
            .expect("native frame");
            assert!(
                !r.machine().expect("machine").cpu().halted,
                "{model:?}: illegal opcode during boot"
            );
        }
        let m = r.machine().expect("machine");
        assert_ne!(m.antic().dmactl_value() & 0x20, 0, "{model:?}: display DMA");
        assert_ne!(m.antic().dlist_value(), 0, "{model:?}: display list");
        let colours: std::collections::HashSet<_> = m.framebuffer().iter().collect();
        assert!(colours.len() >= 3, "{model:?}: border, paper and cursor");
        let screen = u16::from_le_bytes([m.peek(0x58), m.peek(0x59)]);
        let text: String = (0u16..960)
            .map(|i| match m.peek(screen.wrapping_add(i)) & 0x7F {
                c @ 0..=0x3F => (c + 0x20) as char,
                c @ 0x60..=0x7F => c as char,
                _ => ' ',
            })
            .collect();
        assert!(
            text.contains("READY"),
            "{model:?}: missing BASIC READY prompt: {text:?}"
        );
        assert_eq!(r.profile().machine_id.as_str(), m.board_id());
    }
}
