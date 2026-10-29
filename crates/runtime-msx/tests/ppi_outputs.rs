//! An original CPU guest drives PPI outputs and produces host audio.
use emu198x_shell::{
    AudioPacket, AudioSink, HostIo, MachineCore, MachineError, NullFrameSink, NullTraceSink,
    SessionQueryProvider,
};
use runtime_msx::{Model, MsxRuntime, MsxSessionQueryProvider};
#[derive(Default)]
struct Audio {
    nonzero: usize,
    samples: usize,
}
impl AudioSink for Audio {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        assert_eq!(packet.channels, 1);
        assert!(packet.samples.iter().all(|s| s.is_finite()));
        self.samples += packet.samples.len();
        self.nonzero += packet
            .samples
            .iter()
            .filter(|s| s.abs() > 0.000_001)
            .count();
        Ok(())
    }
}
fn guest() -> Vec<u8> {
    let mut program = vec![0x3E, 0x82, 0xD3, 0xAB, 0x3E, 0xC0, 0xD3, 0xA8];
    for _ in 0..4 {
        for control in [0x0F, 0x0E] {
            program.extend([0x3E, control, 0xD3, 0xAB, 0x06, 0x50, 0x10, 0xFE]);
        }
    }
    // Click high, CAPS asserted, motor asserted, write high, keyboard row 10.
    program.extend([0x3E, 0xAA, 0xD3, 0xAA, 0x3E, 0x5A, 0x32, 0x00, 0xC1, 0x76]);
    let mut bios = vec![0; 32768];
    bios[..program.len()].copy_from_slice(&program);
    bios
}
#[test]
fn guest_ppi_controls_reach_queries_and_the_host_audio_packet() {
    let mut runtime = MsxRuntime::new(Model::SonyHb10p, guest()).expect("synthetic BIOS");
    let mut audio = Audio::default();
    runtime
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut NullFrameSink,
            audio_sink: &mut audio,
            trace_sink: &mut NullTraceSink,
        })
        .expect("native field");
    assert_eq!(
        runtime.machine().expect("loaded").peek(0xC100),
        0x5A,
        "guest must finish the port writes"
    );
    let provider = MsxSessionQueryProvider;
    let ppi = provider
        .query(&runtime, "ppi")
        .expect("query")
        .expect("group")
        .value;
    assert_eq!(ppi["keyboard_row"], 10);
    for name in [
        "caps_led_on",
        "cassette_motor_on",
        "cassette_output",
        "key_click_output",
    ] {
        let leaf = provider
            .query(&runtime, &format!("ppi.{name}"))
            .expect("query")
            .expect("live output leaf")
            .value;
        assert_eq!(leaf, serde_json::json!(true), "{name}");
        assert_eq!(ppi[name], leaf, "group and leaf must agree");
    }
    assert!(audio.samples > 0);
    assert!(
        audio.nonzero > 0,
        "CPU PPI writes must be audible in the host packet"
    );
}
