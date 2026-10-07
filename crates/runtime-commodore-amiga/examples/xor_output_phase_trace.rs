//! Trace a settled AGA SPHX guest’s raw BPLCON4 write and XOR sample history.
use emu198x_shell::HeadlessSession;
use peripheral_commodore_amiga_floppy::Adf;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaLiveAccess, AmigaRuntimeKind, Model};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: xor_output_phase_trace <aga> <kickstart> <probe.adf>".into());
    }
    let model = match args[0].as_str() {
        "aga" => Model::A1200AgaPal,
        _ => return Err("XOR phase tracing requires aga".into()),
    };
    let mut runtime = AmigaRuntimeKind::new(model, std::fs::read(&args[1])?)?;
    runtime.insert_floppy0(Adf::from_bytes(std::fs::read(&args[2])?)?, false);
    let mut session = HeadlessSession::new(runtime, A500_PAL_FRAME_TICKS);
    session.run_frames(360)?;
    let machine = session.machine_mut();
    assert_eq!(machine.read_long(0x2FF00), 0x5350_4858, "SPHX readiness");
    assert!(machine.read_long(0x2FF08) >= 9, "settled guest fields");
    let end = machine.tick_count() + A500_PAL_FRAME_TICKS;
    println!("tick,vpos,hpos,next_counter,bplcon4,xor_pipeline");
    let mut rows = 0;
    while machine.tick_count() < end {
        machine.tick();
        let a = machine.agnus();
        if a.vpos == 132 {
            let d = machine.denise_board_pipeline_diagnostic_snapshot();
            let lisa = machine
                .aga_denise_diagnostic_snapshot()
                .ok_or("requires AGA")?;
            if (120..145).contains(&a.hpos) {
                println!(
                    "{},{},{},{},{:04x},{:?}",
                    machine.tick_count(),
                    a.vpos,
                    a.hpos,
                    d.horizontal_counter.position(),
                    lisa.bplcon4,
                    lisa.playfield_xor_pipeline
                );
            }
            rows += 1;
        }
    }
    assert!(rows >= 450, "a complete line must be observed");
    // Stored row 214 is beam line 132, independently of the trace's next-counter column.
    let (width, _) = machine.framebuffer_dims();
    let row = &machine.framebuffer()[214 * width as usize..215 * width as usize];
    for (x, &pixel) in row.iter().enumerate() {
        if x == 0 || pixel != row[x - 1] {
            eprintln!("PIXEL_EDGE x={x} argb={pixel:08x}");
        }
    }

    Ok(())
}
