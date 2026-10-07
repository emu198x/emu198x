//! Observe counter, serial-data and stored-pixel phase on a settled SPHX guest.
use emu198x_shell::HeadlessSession;
use peripheral_commodore_amiga_floppy::Adf;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaLiveAccess, AmigaRuntimeKind, Model};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: ecs_output_phase_trace <ocs|ecs|aga> <kickstart> <probe.adf>".into());
    }
    let model = match args[0].as_str() {
        "ocs" => Model::A500OcsPal,
        "ecs" => Model::A500PlusEcsPal,
        "aga" => Model::A1200AgaPal,
        _ => return Err("model must be ocs, ecs or aga".into()),
    };
    let mut runtime = AmigaRuntimeKind::new(model, std::fs::read(&args[1])?)?;
    runtime.insert_floppy0(Adf::from_bytes(std::fs::read(&args[2])?)?, false);
    let mut session = HeadlessSession::new(runtime, A500_PAL_FRAME_TICKS);
    session.run_frames(360)?;
    let machine = session.machine_mut();
    assert_eq!(machine.read_long(0x2FF00), 0x5350_4858, "SPHX readiness");
    assert!(machine.read_long(0x2FF08) >= 9, "settled guest fields");
    let end = machine.tick_count() + A500_PAL_FRAME_TICKS;
    println!("tick,vpos,hpos,next_counter,window,shift16,shift32,holding,copy,scroll_cursor");
    let mut rows = 0;
    while machine.tick_count() < end {
        machine.tick();
        let a = machine.agnus();
        if a.vpos == 132 {
            let d = machine.denise_board_pipeline_diagnostic_snapshot();
            let b = machine.denise_diagnostic_snapshot().bitplanes;
            println!(
                "{},{},{},{},{},{:04x},{:08x},{:04x},{},{}",
                machine.tick_count(),
                a.vpos,
                a.hpos,
                d.horizontal_counter.position(),
                d.horizontal_diw_active,
                b.shift_data[0],
                b.shift_data_32[0],
                b.holding_data[0],
                b.pending_copy_odd_planes,
                b.serial_scroll_cursor,
            );
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
    assert!(row.contains(&0xFFFF_FFFF), "foreground must be present");
    Ok(())
}
