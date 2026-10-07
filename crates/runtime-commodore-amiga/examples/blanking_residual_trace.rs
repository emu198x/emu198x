//! Trace guest field publication and live ECS output selectors.
use emu198x_shell::HeadlessSession;
use peripheral_commodore_amiga_floppy::Adf;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaLiveAccess, AmigaRuntimeKind, Model};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: blanking_residual_trace <kickstart> <probe.adf>".into());
    }
    let mut runtime = AmigaRuntimeKind::new(Model::A500PlusEcsPal, std::fs::read(&args[0])?)?;
    runtime.insert_floppy0(Adf::from_bytes(std::fs::read(&args[1])?)?, false);
    let mut session = HeadlessSession::new(runtime, A500_PAL_FRAME_TICKS);
    for _ in 0..500 {
        let before = session.machine().tick_count();
        session.run_frames(1)?;
        let m = session.machine();
        let counter = m.read_long(0x2ff08);
        if m.read_long(0x2ff00) == 0x48424c4b {
            println!(
                "FRAME tick={} elapsed={} raw={} guest={} v={} h={} irq={:04x}",
                m.tick_count(),
                m.tick_count() - before,
                m.agnus().vbl_count,
                counter,
                m.agnus().vpos,
                m.agnus().hpos,
                m.intreq()
            );
            if counter >= 14 {
                break;
            }
        }
    }
    let m = session.machine_mut();
    assert_eq!(m.read_long(0x2ff00), 0x48424c4b);
    let mut previous = (m.read_long(0x2ff08), m.intreq() & 0x20);
    let mut changes = 0;
    for _ in 0..A500_PAL_FRAME_TICKS * 4 {
        m.tick();
        let next = (m.read_long(0x2ff08), m.intreq() & 0x20);
        if next != previous {
            println!(
                "EVENT tick={} raw={} guest={} v={} h={} irq={:04x} pc={:06x}",
                m.tick_count(),
                m.agnus().vbl_count,
                next.0,
                m.agnus().vpos,
                m.agnus().hpos,
                m.intreq(),
                m.cpu_pc()
            );
            previous = next;
            changes += 1;
        }
        if m.agnus().vpos == 128 && (140..153).contains(&m.agnus().hpos) {
            let d = m.enhanced_denise().ok_or("missing ECS state")?;
            println!(
                "SELECTOR raw={} h={} d={} bplcon0={:04x} bplcon3={:04x} visible={} ext={} pending={:?} blank={}",
                m.agnus().vbl_count,
                m.agnus().hpos,
                m.denise_board_pipeline_diagnostic_snapshot()
                    .horizontal_counter
                    .position(),
                m.bplcon0(),
                d.bplcon3,
                d.output_ecsena_enabled,
                d.output_extblken_enabled,
                d.output_selector_pipeline,
                d.programmed_hblank_active
            );
        }
    }
    assert!(
        changes >= 8,
        "trace must observe guest and interrupt changes"
    );
    Ok(())
}
