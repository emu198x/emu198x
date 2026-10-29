//! Sound cartridges through the runtime: the FM-PAC found by its signature,
//! and an SCC note, played by the CPU's own bus cycles, reaching the host's
//! audio packets.

use emu198x_shell::{
    AudioPacket, AudioSink, HostIo, MachineCore, MachineError, MachineTime, MediaImage, MediaKind,
    MediaSet, NullFrameSink, NullTraceSink,
};
use runtime_msx::{MapperType, Model, MsxRuntime};

/// Collects every audio packet's samples.
#[derive(Default)]
struct Recorder(Vec<f32>);

impl AudioSink for Recorder {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        self.0.extend_from_slice(packet.samples);
        Ok(())
    }
}

/// `DI; HALT` — the machine runs but does nothing.
fn bios() -> Vec<u8> {
    let mut rom = vec![0; 32 * 1024];
    rom[..2].copy_from_slice(&[0xF3, 0x76]);
    rom
}

#[test]
fn the_media_path_detects_an_fm_pac() {
    let mut runtime = MsxRuntime::new(Model::PhilipsVg8235, bios()).expect("runtime");
    runtime.set_sub_rom(vec![0; 16 * 1024]).expect("sub-rom");
    let mut rom = vec![0; 0x10000];
    rom[0x18..0x20].copy_from_slice(b"PAC2OPLL");
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-2", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("cartridge should load");
    let machine = runtime.machine().expect("machine");
    assert_eq!(
        machine.cartridge(2).map(|(_, mapper)| mapper),
        Some(MapperType::FmPac)
    );
    assert!(machine.cartridge_ym2413(2).is_some());
    assert_eq!(machine.fm_pac_sram(2).map(<[u8]>::len), Some(0x1FFE));
}

/// A BIOS that plays A440 on the SCC through the CPU's own bus cycles:
/// set the PPI, put page 2 in cartridge slot 1, open the SCC, write a
/// square wave, period 253, full volume, enable channel 1, halt.
fn scc_bios() -> Vec<u8> {
    let program = [
        0xF3, // DI
        0x3E, 0x82, 0xD3, 0xAB, // PPI mode
        0x3E, 0x10, 0xD3, 0xA8, // page 2 -> slot 1
        0x3E, 0x3F, 0x32, 0x00, 0x90, // open the SCC
        0x21, 0x00, 0x98, // LD HL,$9800
        0x06, 0x10, 0x3E, 0x7F, 0x77, 0x23, 0x10, 0xFC, // 16 x $7F
        0x06, 0x10, 0x3E, 0x80, 0x77, 0x23, 0x10, 0xFC, // 16 x $80
        0x3E, 0xFD, 0x32, 0x80, 0x98, // period low 253
        0xAF, 0x32, 0x81, 0x98, // period high 0
        0x3E, 0x0F, 0x32, 0x8A, 0x98, // volume 15
        0x3E, 0x01, 0x32, 0x8F, 0x98, // enable channel 1
        0x76, // HALT
    ];
    let mut rom = vec![0; 32 * 1024];
    rom[..program.len()].copy_from_slice(&program);
    rom
}

#[test]
fn an_scc_note_reaches_the_host() {
    let mut runtime = MsxRuntime::new(Model::SonyHb10, scc_bios()).expect("runtime");
    runtime.insert_cartridge1(vec![0; 128 * 1024], MapperType::KonamiScc);
    let mut audio = Recorder::default();
    runtime
        .run_until(
            MachineTime::new(Model::SonyHb10.frame_ticks() * 60),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut audio,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("run");

    // Each packet is a whole 1024-sample buffer; keep each frame's own 802.
    let samples: Vec<f32> = audio
        .0
        .chunks(1024)
        .flat_map(|chunk| &chunk[..802])
        .copied()
        .collect();
    let tail = &samples[samples.len() / 2..];
    let high = tail.iter().copied().fold(f32::MIN, f32::max);
    let low = tail.iter().copied().fold(f32::MAX, f32::min);
    assert!(high - low > 0.25, "the note swings {}", high - low);
    let rising = tail
        .windows(2)
        .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
        .count();
    let hz = rising as f64 * 48_000.0 / tail.len() as f64;
    assert!((hz - 440.4).abs() < 5.0, "the SCC played {hz:.1} Hz");
    let scc = runtime
        .machine()
        .and_then(|m| m.cartridge_scc(1))
        .expect("an SCC");
    assert_eq!(scc.period(0), 253);
}
