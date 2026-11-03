//! CPU bus stores must agree with the logical RAM exposed to tools and saves.
use emu198x_shell::{
    FamilyRuntime, FirmwareImage, FirmwareSet, HostIo, MachineCore, NullAudioSink, NullFrameSink,
    NullTraceSink,
};
use runtime_sinclair_zx_spectrum::{Model, SpectrumLiveAccess, SpectrumRuntimeKind};

fn machine(model: Model, code: &[u8]) -> SpectrumRuntimeKind {
    let mut rom = [0_u8; 16384];
    rom[..code.len()].copy_from_slice(code);
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("sinclair-zx-spectrum-48k-rom", &rom));
    SpectrumRuntimeKind::from_firmware(model, &firmware).expect("synthetic CPU ROM")
}

fn finish(machine: &mut SpectrumRuntimeKind) {
    let mut frames = NullFrameSink;
    let mut audio = NullAudioSink;
    let mut trace = NullTraceSink;
    for _ in 0..64 {
        if machine.z80_halted() {
            return;
        }
        machine
            .run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut frames,
                audio_sink: &mut audio,
                trace_sink: &mut trace,
            })
            .expect("native boundary");
    }
    assert!(
        machine.z80_halted(),
        "bounded CPU RAM sweep must reach HALT"
    );
}

#[test]
fn zx8401_cpu_store_reaches_lower_display_ram() {
    // DI; LD A,$A5; LD ($4000),A; HALT. This uses the CPU bus, not a host poke.
    let mut m = machine(
        Model::SpectrumIssue5Ula6c001e6Ram16kPal,
        &[0xf3, 0x3e, 0xa5, 0x32, 0, 0x40, 0x76],
    );
    finish(&mut m);
    assert_eq!(m.read_byte(0x4000), 0xa5);
    assert_eq!(m.read_byte(0xc000), 0xff);
}

#[test]
fn cpu_address_sweeps_reach_every_fitted_cell_on_every_board() {
    let mut checked = 0;
    for model in Model::ALL
        .into_iter()
        .filter(|m| m.physical_board().is_some())
    {
        for high_byte in [false, true] {
            // DI; LD HL,$4000; loop: LD (HL),L/H; INC HL; LD A,H;
            // OR L; JR NZ,loop; HALT. Two patterns identify every address bit.
            let mut m = machine(
                model,
                &[
                    0xf3,
                    0x21,
                    0,
                    0x40,
                    if high_byte { 0x74 } else { 0x75 },
                    0x23,
                    0x7c,
                    0xb5,
                    0x20,
                    0xfa,
                    0x76,
                ],
            );
            finish(&mut m);
            for address in 0x4000..=u16::MAX {
                let expected = if model.ram_kib() == Some(16) && address >= 0x8000 {
                    0xff
                } else if high_byte {
                    (address >> 8) as u8
                } else {
                    address as u8
                };
                assert_eq!(
                    m.read_byte(address),
                    expected,
                    "{}: address {address:#06x}, high-byte pass {high_byte}",
                    model.profile_id()
                );
            }
            assert_eq!(m.read_byte(0), 0xf3, "ROM remains immutable");
            checked += 1;
        }
    }
    assert_eq!(checked, 36);
}
