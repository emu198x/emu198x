//! Trace real guest display-register writes across a scroll wrap.
use emu198x_shell::{HeadlessSession, InputEvent, MachineCore};
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaOcsRuntime, Model};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: scroll_trace <kickstart> <snapshot>".into());
    }
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPalA501, std::fs::read(&args[0])?)?;
    runtime.restore(&std::fs::read(&args[1])?)?;
    let mut session = HeadlessSession::new(runtime, A500_PAL_FRAME_TICKS);
    session.run_frames(1)?;
    session.queue_input(InputEvent::Button {
        port: 2,
        name: "right".into(),
        pressed: true,
    });
    session.run_frames(90)?;
    for frame in 91..120 {
        let m = session.machine_mut().machine_mut();
        m.debug_copper_move_log.clear();
        m.debug_custom_write_log.clear();
        session.run_frames(1)?;
        let m = session.machine().machine();
        println!("FRAME {frame} DDF={:04x}", m.agnus().ddfstrt);
        for &(cck, v, h, reg, val) in &m.debug_copper_move_log {
            if (0xe0..=0xf6).contains(&reg) || reg == 0x102 || reg == 0x92 {
                println!("COP {cck} {v:03x}:{h:02x} {reg:03x} {val:04x}");
            }
        }
        for &(cck, pc, _, reg, val, _) in &m.debug_custom_write_log {
            if (0xe0..=0xf6).contains(&reg) || reg == 0x102 || reg == 0x92 {
                println!("CPU {cck} PC={pc:06x} {reg:03x} {val:04x}");
            }
        }
    }
    Ok(())
}
