//! Observe Lisa's first visible line without changing chip state or timing.
use emu198x_shell::HeadlessSession;
use peripheral_commodore_amiga_floppy::Adf;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaLiveAccess, AmigaRuntimeKind, Model};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: top_field_blanking_trace <kickstart> <probe.adf>".into());
    }
    let mut runtime = AmigaRuntimeKind::new(Model::A1200AgaPal, std::fs::read(&args[0])?)?;
    runtime.insert_floppy0(Adf::from_bytes(std::fs::read(&args[1])?)?, false);
    let mut session = HeadlessSession::new(runtime, A500_PAL_FRAME_TICKS);
    for _ in 0..500 {
        session.run_frames(1)?;
        let m = session.machine();
        if m.read_long(0x2ff00) == 0x5350_4858 && m.read_long(0x2ff08) >= 9 {
            break;
        }
    }
    let m = session.machine_mut();
    assert_eq!(m.read_long(0x2ff00), 0x5350_4858, "guest ready magic");
    assert!(m.read_long(0x2ff08) >= 9, "guest must settle");
    let mut previous = (u16::MAX, u16::MAX);
    let mut rows = 0;
    for _ in 0..A500_PAL_FRAME_TICKS * 2 {
        m.tick();
        let position = (m.agnus().vpos, m.agnus().hpos);
        if position == previous {
            continue;
        }
        previous = position;
        let (v, h) = position;
        if (24..=27).contains(&v) && (h < 8 || (120..=164).contains(&h)) {
            let board = m.denise_board_pipeline_diagnostic_snapshot();
            let lisa = m.aga_denise_diagnostic_snapshot().ok_or("missing Lisa")?;
            println!(
                "TOP_FIELD tick={} field={} guest={} v={v} h={h} counter={:?} phblank={}",
                m.tick_count(),
                m.agnus().vbl_count,
                m.read_long(0x2ff08),
                board.horizontal_counter,
                lisa.programmed_hblank_active,
            );
            rows += 1;
        }
    }
    assert!(rows >= 400, "trace must span both boundary crossings");
    assert_eq!(m.framebuffer_dims(), (1536, 576));
    for y in 0..8 {
        println!(
            "PIXELS y={y} x16={:08x} x667={:08x} x668={:08x} x923={:08x} x924={:08x}",
            m.framebuffer()[y * 1536 + 16],
            m.framebuffer()[y * 1536 + 667],
            m.framebuffer()[y * 1536 + 668],
            m.framebuffer()[y * 1536 + 923],
            m.framebuffer()[y * 1536 + 924]
        );
    }
    Ok(())
}
