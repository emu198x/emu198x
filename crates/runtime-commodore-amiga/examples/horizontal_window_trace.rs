//! Observe DIW register delivery and Denise's gate without modifying execution.
use emu198x_shell::HeadlessSession;
use peripheral_commodore_amiga_floppy::Adf;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaA1200Runtime, AmigaLiveAccess, Model};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: horizontal_window_trace <kickstart> <probe.adf>".into());
    }
    let mut runtime = AmigaA1200Runtime::new(Model::A1200AgaPal, std::fs::read(&args[0])?)?;
    runtime
        .machine_mut()
        .insert_adf(Adf::from_bytes(std::fs::read(&args[1])?)?);
    let mut session = HeadlessSession::new(runtime, A500_PAL_FRAME_TICKS);
    session.run_frames(360)?;
    let machine = session.machine_mut().machine_mut();
    assert_eq!(
        machine.read_long(0x2FF00),
        0x5350_4858,
        "SPHX guest must be ready"
    );
    assert!(machine.read_long(0x2FF08) >= 9, "guest must have settled");
    let end = machine.tick_count() + A500_PAL_FRAME_TICKS;
    println!("ticks,vpos,hpos,next_denise_counter,diwstrt,diwstop,horizontal_active");
    while machine.tick_count() < end {
        machine.tick();
        let a = machine.agnus();
        if (134..=137).contains(&a.vpos) && (0x5A..=0x90).contains(&a.hpos) {
            let d = machine.denise_board_pipeline_diagnostic_snapshot();
            println!(
                "{},{},{},{},{:04x},{:04x},{}",
                machine.tick_count(),
                a.vpos,
                a.hpos,
                d.horizontal_counter.position(),
                a.diwstrt,
                a.diwstop,
                d.horizontal_diw_active,
            );
        }
    }
    Ok(())
}
