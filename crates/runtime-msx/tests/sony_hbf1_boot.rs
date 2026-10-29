//! The identified Sony HB-F1 ROM set reaches BASIC 2.0 on its concrete owner.
//!
//! MAME `src/mame/msx/msx2.cpp`: f1bios.rom SHA-1
//! 0081ea0d25bc5cd8d70b60ad8cfdc7307812c0fd and f1ext.rom SHA-1
//! b8e30d604d319d511cbfbc61e5d8c38fbb9c5a33. No firmware is committed.
use common_msx::{MsxHardware, V9938Mode};
use emu198x_shell::{HostIo, MachineCore, NullAudioSink, NullFrameSink, NullTraceSink};
use runtime_msx::{Model, MsxRuntime};
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
fn text(runtime: &MsxRuntime) -> Option<String> {
    let v = runtime.machine()?.v9938()?;
    // The original Japanese ROM boots in G1 (32-column character names);
    // T1 uses 40 columns. Both use R#2 bits 0-6 as A10-A16, matching the
    // existing renderer's name-table view. The ROM supplies the expected text.
    let columns = match v.mode() {
        V9938Mode::G1 => 32,
        V9938Mode::T1 => 40,
        _ => return None,
    };
    let base = usize::from(v.register(2) & 0x7F) << 10;
    Some(
        v.vram()
            .get(base..base + columns * 24)?
            .chunks(columns)
            .map(|row| {
                row.iter()
                    .map(|&b| {
                        if b.is_ascii_graphic() || b == b' ' {
                            char::from(b)
                        } else {
                            ' '
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}
#[test]
#[ignore = "FIXTURE: needs identified f1bios.rom and f1ext.rom in EMU198X_MSX_HBF1_ROM_DIR"]
fn original_sony_hbf1_reaches_msx_basic_2_and_ok() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("EMU198X_MSX_HBF1_ROM_DIR").expect("explicit model-matched ROM directory"),
    );
    let bios = std::fs::read(dir.join("f1bios.rom")).expect("original main ROM");
    let sub = std::fs::read(dir.join("f1ext.rom")).expect("original SUB-ROM");
    assert_eq!(bios.len(), 32768);
    assert_eq!(sub.len(), 16384);
    assert_eq!(crc32(&bios), 0x9B3E_7B97, "MAME HB-F1 main ROM identity");
    assert_eq!(crc32(&sub), 0x4A48_779C, "MAME HB-F1 SUB-ROM identity");
    let mut r = MsxRuntime::new(Model::SonyHbF1, bios).expect("main ROM");
    r.set_sub_rom(sub).expect("SUB-ROM");
    assert_eq!(
        r.machine().expect("board").hardware(),
        MsxHardware::SonyHbF1
    );
    for frame in 1..=400 {
        r.run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut NullFrameSink,
            audio_sink: &mut NullAudioSink,
            trace_sink: &mut NullTraceSink,
        })
        .expect("native frame");
        if let Some(screen) = text(&r) {
            let upper = screen.to_ascii_uppercase();
            if upper.contains("MSX BASIC VERSION 2.0")
                && upper.lines().any(|row| row.trim() == "OK")
            {
                println!("Sony HB-F1 reached BASIC 2.0 and Ok at native frame {frame}\n{screen}");
                return;
            }
        }
    }
    panic!("Sony HB-F1 did not reach BASIC 2.0/Ok; text={:?}", text(&r));
}
