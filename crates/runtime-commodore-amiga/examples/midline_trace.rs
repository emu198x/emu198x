//! Trace physical bitplane transfers around the mid-line register probes.
use emu198x_shell::HeadlessSession;
use peripheral_commodore_amiga_floppy::Adf;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaA1200Runtime, Model};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        return Err("usage: midline_trace <kickstart> <probe.adf> [line]".into());
    }
    let line: u16 = args.get(2).map_or(Ok(132), |value| value.parse())?;
    let mut runtime = AmigaA1200Runtime::new(Model::A1200AgaPal, std::fs::read(&args[0])?)?;
    runtime
        .machine_mut()
        .insert_adf(Adf::from_bytes(std::fs::read(&args[1])?)?);
    let mut session = HeadlessSession::new(runtime, A500_PAL_FRAME_TICKS);
    session.run_frames(180)?;
    let machine = session.machine_mut().machine_mut();
    let mut previous = (
        machine.agnus().bpl_pt[0],
        machine.agnus().bplcon0,
        machine.agnus().fmode,
        machine.denise_aga().as_inner().as_inner().bplcon1,
    );
    let end = machine.tick_count() + A500_PAL_FRAME_TICKS;
    while machine.tick_count() < end {
        machine.tick();
        let agnus = machine.agnus();
        let current = (
            agnus.bpl_pt[0],
            agnus.bplcon0,
            agnus.fmode,
            machine.denise_aga().as_inner().as_inner().bplcon1,
        );
        let in_window = args.len() == 3 || (0x7C..=0x90).contains(&agnus.hpos);
        if agnus.vpos == line && in_window && current != previous {
            println!(
                "h={} pt={:08x}->{:08x} fmode={} con0={:04x} dma={:04x} lisa={:04x} con1={:04x}",
                agnus.hpos,
                previous.0,
                current.0,
                current.2,
                current.1,
                agnus.dma_bplcon0(),
                machine.denise_aga().as_inner().as_inner().bplcon0,
                current.3
            );
        }
        previous = current;
    }
    Ok(())
}
