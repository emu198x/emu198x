//! Diagnostic: trace the solo-1581 serial LOAD handshake to find where it
//! deadlocks.
//!
//! Isolates the 1581 (no 1541 on the bus) so the DATA line is only the C64 +
//! 1581 — this removes the 1541-un-listen variable and targets the 1581's own
//! ATNA/DATA-acknowledge sequencing directly.
//!
//! Not asserting correctness — this is a "show me the picture" evidence pass.
//! Run with `--ignored --nocapture`.
//!
//! It boots to READY, types `LOAD"*",9,1`, then samples both CPUs and the IEC
//! line state finely through the attempt. The report answers:
//!   - does the 1581 run its ATN handler at all (page-visit set)?
//!   - does the drive ever drive DATA/CLK/ATNA (port-B output set)?
//!   - where do the C64 and the 1581 end up stuck, and what are the lines?

mod common;

use std::collections::BTreeSet;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model, type_string};

use common::local_rom_firmware_with_1581_only;

/// A blank 800 KB D81 (80 tracks × 40 sectors × 256 B). The command-transfer
/// deadlock happens during the ATN/LISTEN handshake, before any disk access,
/// so the sector contents are irrelevant here — this removes the TOSEC media
/// dependency entirely.
const D81_LEN: usize = 80 * 40 * 256;

/// Decodes the drive-side CIA Port B *input* latch into (DATA, CLK, ATN) line
/// levels (1 = released high, 0 = pulled low). PB0 DATA-in, PB2 CLK-in, PB7
/// ATN-in (see `machine-commodore-1581::apply_drive_inputs`).
fn drive_inputs(pb_in: u8) -> (u8, u8, u8) {
    (pb_in & 0x01, (pb_in >> 2) & 0x01, (pb_in >> 7) & 0x01)
}

/// Decodes the drive-side CIA Port B *output* state into (DATA-out, CLK-out,
/// ATNA) bits. PB1 DATA-out, PB3 CLK-out, PB4 ATNA.
fn drive_outputs(pb_out: u8) -> (u8, u8, u8) {
    (
        (pb_out >> 1) & 0x01,
        (pb_out >> 3) & 0x01,
        (pb_out >> 4) & 0x01,
    )
}

/// Decodes the C64's CIA2 Port A IEC *output* latch into (DATA-out, CLK-out,
/// ATN-out) bits. PA5 DATA-out, PA4 CLK-out, PA3 ATN-out. On the hardware a
/// set bit pulls the line low (inverting driver), so bit=1 means "asserting".
fn c64_outputs(pa: u8) -> (u8, u8, u8) {
    ((pa >> 5) & 0x01, (pa >> 4) & 0x01, (pa >> 3) & 0x01)
}

/// One captured handshake sample.
struct Sample {
    i: usize,
    c64_pc: u16,
    d_pc: u16,
    pb_in: u8,
    pb_out: u8,
    cia2_pa: u8,
    flag: bool,
    icr_s: u8,
    icr_m: u8,
}

#[test]
#[ignore = "FIXTURE: requires local C64 ROMs + 1581 DOS ROM + Batman D81 — run with --ignored --nocapture"]
fn trace_solo_1581_load_handshake() {
    let firmware = local_rom_firmware_with_1581_only();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("local ROMs (incl. 1581) should construct a C64 runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );

    // Disk selection:
    //   EMU1581_D81=<path> — mount a real .d81 (the true repro; exercises the
    //                        FDC directory read + the IEC transfer)
    //   EMU1581_NO_DISK=1  — no disk (isolates the IEC turnaround, no FDC)
    //   (default)          — a blank 800 KB image (has no valid directory)
    if let Some(path) = std::env::var_os("EMU1581_D81") {
        let bytes = std::fs::read(&path).expect("EMU1581_D81 file should be readable");
        println!("(mounted real D81: {} bytes)", bytes.len());
        let mut media = MediaSet::new();
        media.push(MediaImage::new("drive-9", MediaKind::Disk, &bytes));
        session
            .load_media(&media)
            .expect("real D81 should mount into drive-9");
    } else if std::env::var_os("EMU1581_NO_DISK").is_some() {
        println!("(running with NO disk mounted)");
    } else {
        let blank_d81 = vec![0u8; D81_LEN];
        let mut media = MediaSet::new();
        media.push(MediaImage::new("drive-9", MediaKind::Disk, &blank_d81));
        session
            .load_media(&media)
            .expect("blank D81 should mount into drive-9");
    }

    assert!(
        session.machine().drive_1581().is_some(),
        "1581 should be present"
    );
    assert!(
        session.machine().drive8().is_none(),
        "no 1541 in the solo case"
    );

    // ── Boot trace: find whether/where the drive ever enables interrupts ──
    // A healthy 1581 must reach an interrupts-enabled idle so it can service the
    // ATN FLAG IRQ. Trace the drive's I-flag through boot to find where it goes
    // to I=1 and never returns. Sample finely (run_until +16) and note the last
    // PC seen with I=0 and where BUSY first sticks.
    use emu198x_shell::SessionQueryProvider;
    let provider = C64SessionQueryProvider;
    {
        let mut ever_i0 = false;
        let mut last_i0_pc = 0u16;
        let mut last_i0_step = 0usize;
        let mut first_stuck_busy_pc = 0u16;
        let mut busy_edges = 0u32; // BUSY 0->1 transitions = distinct FDC commands
        let mut prev_busy = false;
        let mut atn_low_events = 0u32; // ATN-in 1->0 edges the drive sees
        let mut first_atn_low_pc = 0u16;
        let mut first_atn_low_step = 0usize;
        let mut prev_atn_low = false;
        // Drive PC-page transition log (boot phases), to diff against VICE's
        // sequence $AF -> $CB -> $95 -> $CB -> $A9 -> $B1(idle).
        let mut phase_log: Vec<(usize, u16)> = Vec::new();
        let mut prev_page = 0xFFFFu16;
        let mut reached_b105 = false; // did the drive reach the correct idle loop?
        let mut flag_irq_count = 0u32; // drive CIA FLAG (ATN) interrupt rising edges
        let mut prev_flag_irq = 0u8;
        let mut flag_irq_first_step = 0usize;
        let mut reached_ready = false;
        // Ring buffer of recent drive PCs; snapshot the path INTO $AE58 the
        // first time the drive gets stuck there.
        let mut ring: Vec<u16> = Vec::new();
        let mut path_into_ae58: Option<Vec<u16>> = None;
        let mut cia2_pa_at_ae58 = 0u8; // C64 CIA2 PA when the drive first stalls
        let mut clk_asserted = false; // has the C64 driven CLK low (PA4) yet?
        let mut clk_assert_c64_pc = 0u16;
        let mut clk_assert_step = 0usize;
        // Boot-onset probe: the drive sees a spurious ATN-in 1->0 edge very
        // early (step 3). Step finely from reset and log the C64 CIA2 output +
        // the drive's ATN-in to find the exact write that asserts ATN.
        println!("  -- boot-onset ATN probe (C64 drives ATN-in; VICE sees 0 edges) --");
        {
            let q = |m: &_, k: &str| -> u64 {
                provider
                    .query(m, k)
                    .ok()
                    .flatten()
                    .and_then(|r| r.value.as_u64())
                    .unwrap_or(0xFFFF)
            };
            let mut prev_atn_in = true;
            for p in 0..80usize {
                let target = session.time().saturating_add(4);
                session.run_until(target).expect("onset probe");
                let m = session.machine();
                let pa = q(m, "cia2.pa") as u8;
                let ddra = q(m, "cia2.ddra") as u8;
                let pads = q(m, "cia2.port_a_drive_state") as u8;
                let drive = m.drive_1581().expect("present");
                let atn_in = drive.cia().pb_in & 0x80 != 0;
                let c64pc = drive.cpu().regs.pc; // (drive pc; C64 pc below)
                if p < 6 || prev_atn_in != atn_in {
                    println!(
                        "    onset p={p:2}: C64 cia2 pa=${pa:02X} ddra=${ddra:02X} pa_drive=${pads:02X} | drive ATN-in={} dpc=${c64pc:04X}",
                        u8::from(atn_in)
                    );
                }
                prev_atn_in = atn_in;
            }
        }
        // Track $76 (the DOS serial/ATN state byte) through boot. bit1 = serial-
        // idle flag; at the boot tail `$AFE3 JSR $AD15` needs bit1 SET so $AD15
        // RTSs (VICE) instead of `JMP $FF30` into the serial handler (ours).
        let mut prev_76 = 0xFFu8;
        let mut z76_log: Vec<(usize, u16, u8)> = Vec::new();
        // (step, pc, ring) snapshot when $76 bit1 first sets — the divergence.
        let mut path_into_ab: Option<(usize, u16, Vec<u16>)> = None;
        let steps = 400_000usize; // ~6.4M C64 cycles — long enough to reach $AE58
        for s in 0..steps {
            let target = session.time().saturating_add(16);
            session.run_until(target).expect("boot advance");
            let drive = session.machine().drive_1581().expect("present");
            let d_pc = drive.cpu().regs.pc;
            let z76_now = drive.peek(0x76);
            // Exclude the RAM-test page ($AFxx) — it sweeps incrementing values
            // through all of zero-page including $76, which is just noise.
            if z76_now != prev_76 {
                if (d_pc >> 8) != 0xAF && z76_log.len() < 300 {
                    z76_log.push((s, d_pc, z76_now));
                }
                prev_76 = z76_now;
            }
            // Snapshot the exact ring the first time $76 bit1 gets set (the
            // divergence toward the serial-idle/$AE58 path).
            if path_into_ab.is_none() && z76_now & 0x02 != 0 && (d_pc >> 8) != 0xAF {
                path_into_ab = Some((s, d_pc, ring.clone()));
            }
            ring.push(d_pc);
            if ring.len() > 48 {
                ring.remove(0);
            }
            if path_into_ae58.is_none() && (0xAE58..=0xAE5D).contains(&d_pc) {
                path_into_ae58 = Some(ring.clone());
                cia2_pa_at_ae58 = provider
                    .query(session.machine(), "cia2.pa")
                    .ok()
                    .flatten()
                    .and_then(|r| r.value.as_u64())
                    .unwrap_or(0) as u8;
            }
            let i_set = (drive.cpu().regs.p >> 2) & 1 == 1;
            if !i_set {
                ever_i0 = true;
                last_i0_pc = drive.cpu().regs.pc;
                last_i0_step = s;
            }
            let busy = drive.fdc_status() & 0x01 != 0;
            if busy && !prev_busy {
                first_stuck_busy_pc = drive.cpu().regs.pc;
                busy_edges += 1;
            }
            prev_busy = busy;
            let atn_low = drive.cia().pb_in & 0x80 == 0; // PB7 ATN-in low
            if atn_low && !prev_atn_low {
                if atn_low_events == 0 {
                    first_atn_low_pc = drive.cpu().regs.pc;
                    first_atn_low_step = s;
                }
                atn_low_events += 1;
            }
            prev_atn_low = atn_low;
            let page = d_pc >> 8;
            if page != prev_page {
                if phase_log.len() < 400 {
                    phase_log.push((s, d_pc));
                }
                prev_page = page;
            }
            if (0xB100..=0xB137).contains(&d_pc) {
                reached_b105 = true;
            }
            let flag_irq = (drive.cia().icr_status() >> 4) & 1;
            if flag_irq == 1 && prev_flag_irq == 0 {
                if flag_irq_count == 0 {
                    flag_irq_first_step = s;
                }
                flag_irq_count += 1;
            }
            prev_flag_irq = flag_irq;
            // Watch the C64's CLK-out (CIA2 PA bit4); record where it first
            // drives CLK low.
            if !clk_asserted && s % 40 == 0 {
                let pa = provider
                    .query(session.machine(), "cia2.pa")
                    .ok()
                    .flatten()
                    .and_then(|r| r.value.as_u64())
                    .unwrap_or(0) as u8;
                if (pa >> 4) & 1 == 1 {
                    clk_asserted = true;
                    clk_assert_c64_pc = session.machine().machine().cpu().regs.pc;
                    clk_assert_step = s;
                }
            }
            // Note READY but keep tracing — the drive settles into $AE58 only
            // AFTER the C64 gives up, so breaking here would miss the divergence.
            if !reached_ready
                && s % 2_000 == 0
                && let Ok(Some(r)) = provider.query(session.machine(), "screen.text.lines")
                && format!("{:?}", r.value).contains("READY.")
            {
                reached_ready = true;
            }
            // Stop once the drive has both diverged ($76 bit1) and stalled.
            if path_into_ab.is_some() && path_into_ae58.is_some() {
                break;
            }
        }
        println!("\n=== 1581 boot trace ===");
        println!("  C64 reached READY: {reached_ready}");
        println!("  drive EVER had I=0 during boot: {ever_i0}");
        println!("  last step with I=0: {last_i0_step}, PC there=${last_i0_pc:04X}");
        println!("  PC where FDC BUSY last (re)asserted: ${first_stuck_busy_pc:04X}");
        println!("  FDC BUSY 0->1 edges (distinct commands issued): {busy_edges}");
        println!(
            "  ATN-in 1->0 edges the drive saw during boot: {atn_low_events}; first at step {first_atn_low_step}, drive PC=${first_atn_low_pc:04X}"
        );
        println!(
            "  C64 first drove CLK LOW at step {clk_assert_step}, C64 PC=${clk_assert_c64_pc:04X} (asserted={clk_asserted})"
        );
        let d_pc_now = session
            .machine()
            .drive_1581()
            .expect("present")
            .cpu()
            .regs
            .pc;
        println!("  drive reached correct idle $B105-$B137: {reached_b105}");
        println!(
            "  drive FLAG (ATN) IRQ rising edges during boot: {flag_irq_count} (first at step {flag_irq_first_step})"
        );
        println!("  drive PC at end of boot trace: ${d_pc_now:04X}");
        println!("  drive boot phase sequence (step: PC on each page change):");
        for (st, pc) in phase_log.iter().take(80) {
            print!("  {st}:${pc:04X}");
        }
        println!();
        println!("  $76 changes during boot (step: PC -> $76; bit1=serial-idle):");
        for (st, pc, v) in &z76_log {
            print!("  {st}:${pc:04X}=${v:02X}");
        }
        println!();
        match &path_into_ab {
            Some((st, pc, pcs)) => {
                println!("  $76 bit1 first SET at step {st}, PC=${pc:04X}; ring into it:");
                print!("   ");
                for p in pcs {
                    print!(" ${p:04X}");
                }
                println!();
            }
            None => println!("  $76 bit1 never set during boot"),
        }
        match &path_into_ae58 {
            Some(pcs) => {
                println!(
                    "  C64 CIA2.PA at first $AE58 stall: ${cia2_pa_at_ae58:02X} (CLK-out bit4={}, so CLK line {})",
                    (cia2_pa_at_ae58 >> 4) & 1,
                    if (cia2_pa_at_ae58 >> 4) & 1 == 1 {
                        "LOW/asserted"
                    } else {
                        "HIGH/released"
                    }
                );
                println!(
                    "  path INTO $AE58 (last {} drive PCs before first entry):",
                    pcs.len()
                );
                print!("   ");
                for pc in pcs {
                    print!(" ${pc:04X}");
                }
                println!();
            }
            None => println!("  drive never entered $AE58 during boot"),
        }
    }

    // ── Probe the drive's TRUE post-boot idle loop, before any LOAD ──
    // A healthy 1581 idles polling ATN (its ATN handler runs on the FLAG IRQ).
    // Histogram the drive PC to see where it actually sits at rest.
    {
        use std::collections::BTreeMap;
        let mut hist: BTreeMap<u16, u32> = BTreeMap::new();
        let mut ever_i_clear = false; // did the drive EVER enable interrupts?
        let mut fdc_busy_seen = 0u32;
        // Run a long idle so the boot-time directory reads definitely finish,
        // and only histogram the SETTLED tail (last 8000 of 40000 samples).
        const IDLE_TOTAL: usize = 40_000;
        const HIST_TAIL: usize = 8_000;
        for i in 0..IDLE_TOTAL {
            let target = session.time().saturating_add(8);
            session.run_until(target).expect("advance");
            let drive = session.machine().drive_1581().expect("1581 present");
            let d_pc = drive.cpu().regs.pc;
            if i >= IDLE_TOTAL - HIST_TAIL {
                if (drive.cpu().regs.p >> 2) & 1 == 0 {
                    ever_i_clear = true;
                }
                if drive.fdc_status() & 0x01 != 0 {
                    fdc_busy_seen += 1;
                }
                *hist.entry(d_pc).or_default() += 1;
            }
        }
        let drive = session.machine().drive_1581().expect("1581 present");
        let cia = drive.cia();
        let p = drive.cpu().regs.p;
        println!("\n  drive EVER had interrupts enabled (I=0) post-boot: {ever_i_clear}");
        println!(
            "  WD177x BUSY set in {fdc_busy_seen}/8000 idle samples; final fdc_status=${:02X}",
            drive.fdc_status()
        );
        println!("\n=== 1581 post-boot idle probe (no LOAD yet) ===");
        println!(
            "  CPU: P=${p:02X} (I-flag={}), CIA: flag_pin={} icr_mask=${:02X} icr_status=${:02X} irq={} pb_in=${:02X}",
            (p >> 2) & 1,
            u8::from(cia.flag),
            cia.icr_mask(),
            cia.icr_status(),
            u8::from(cia.irq),
            cia.pb_in
        );
        let mut top: Vec<(u16, u32)> = hist.into_iter().collect();
        top.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        println!("  top idle PCs (pc: samples):");
        for (pc, n) in top.iter().take(12) {
            println!("    ${pc:04X}: {n}");
        }
        // Drive zero-page state, comparable to VICE's read. $76 = ATN-pending
        // flag (bit0), $87 = job pending. VICE at idle: $76=$08 (bit0=0).
        let z76 = drive.peek(0x76);
        let z87 = drive.peek(0x87);
        print!("  drive ZP $70-$90:");
        for a in 0x70u16..=0x90 {
            print!(" {:02X}", drive.peek(a));
        }
        println!();
        // Main-loop dispatch vectors: boot ends `JMP $FF00` = `JMP ($0190)`,
        // and `$FF30` (ATN handler) = `JMP ($01B0)`. Correct idle is $B105.
        let v190 = u16::from(drive.peek(0x0190)) | (u16::from(drive.peek(0x0191)) << 8);
        let v1b0 = u16::from(drive.peek(0x01B0)) | (u16::from(drive.peek(0x01B1)) << 8);
        println!(
            "  RAM vectors: ($0190)=${v190:04X} (idle; want $B105)  ($01B0)=${v1b0:04X} (ATN)"
        );
        println!(
            "  $76=${z76:02X} (ATN-pending bit0={})  $87=${z87:02X} (job)",
            z76 & 1
        );
    }

    // Type the command WITHOUT the newline, so the LOAD does not execute yet;
    // we press RETURN inside the sampling loop to catch the live handshake.
    type_string(&mut session, "LOAD\"*\",9,1", 3, 2).expect("type the LOAD command");
    session.queue_input(InputEvent::Key {
        name: "return".into(),
        pressed: true,
    });

    // ── Fine sampling of the handshake window ──
    const STEP_TICKS: u64 = 8;
    const MAX_SAMPLES: usize = 400_000;

    let mut c64_pages: BTreeSet<u16> = BTreeSet::new();
    let mut drive_pages: BTreeSet<u16> = BTreeSet::new();
    let mut drive_out_states: BTreeSet<u8> = BTreeSet::new();
    let mut drive_in_states: BTreeSet<u8> = BTreeSet::new();
    let mut flag_asserts = 0u32; // ICR-status FLAG bit (0x10) seen set
    let mut prev_flag_bit = 0u8;

    // Transition log: record whenever any IEC line (drive in/out OR the C64's
    // CIA2 PA drive latch) changes, capped so the report stays readable.
    let mut transitions: Vec<Sample> = Vec::new();
    let mut prev_pb_in = 0xFFu8;
    let mut prev_pb_out = 0xFFu8;
    let mut prev_cia2_pa = 0xFFu8;

    // Detect the end of the attempt: the LOAD has to actually start (C64 leaves
    // the READY loop into the LOAD/serial region) before we treat a return to
    // READY as the deadlock/failure point.
    let mut load_started = false;
    let mut ready_streak = 0u32;
    let mut deadlock_at: Option<usize> = None;

    for i in 0..MAX_SAMPLES {
        if i == 3_000 {
            // Release RETURN after ~one keyboard-scan frame so it registers as
            // a clean keypress rather than a stuck key.
            session.queue_input(InputEvent::Key {
                name: "return".into(),
                pressed: false,
            });
        }
        let target = session.time().saturating_add(STEP_TICKS);
        session.run_until(target).expect("advance the machine");

        let c64_pc = session.machine().machine().cpu().regs.pc;
        let drive = session.machine().drive_1581().expect("1581 stays present");
        let d_pc = drive.cpu().regs.pc;
        let cia = drive.cia();
        let pb_in = cia.pb_in;
        let pb_out = cia.port_b_drive_state();
        let flag_pin = cia.flag;
        let icr_status = cia.icr_status();
        let icr_mask = cia.icr_mask();
        let cia2_pa = provider
            .query(session.machine(), "cia2.pa")
            .ok()
            .flatten()
            .and_then(|r| r.value.as_u64())
            .unwrap_or(0xFF) as u8;

        // Sample the C64 KERNAL load pointer ($AE/$AF advances as bytes arrive)
        // to prove data is actually transferring, not hung at "LOADING".
        if matches!(i, 20_000 | 100_000 | 200_000 | 399_000) {
            let lo = provider
                .query(session.machine(), "memory.ram.00ae")
                .ok()
                .flatten()
                .and_then(|r| r.value.as_u64())
                .unwrap_or(0);
            let hi = provider
                .query(session.machine(), "memory.ram.00af")
                .ok()
                .flatten()
                .and_then(|r| r.value.as_u64())
                .unwrap_or(0);
            println!("  load-ptr @sample {i}: ${:04X}", (hi << 8) | lo);
        }

        c64_pages.insert(c64_pc >> 8);
        drive_pages.insert(d_pc >> 8);
        drive_out_states.insert(pb_out);
        drive_in_states.insert(pb_in);

        let flag_bit = (icr_status >> 4) & 0x01;
        if flag_bit == 1 && prev_flag_bit == 0 {
            flag_asserts += 1;
        }
        prev_flag_bit = flag_bit;

        if (pb_in != prev_pb_in || pb_out != prev_pb_out || cia2_pa != prev_cia2_pa)
            && transitions.len() < 800
        {
            transitions.push(Sample {
                i,
                c64_pc,
                d_pc,
                pb_in,
                pb_out,
                cia2_pa,
                flag: flag_pin,
                icr_s: icr_status,
                icr_m: icr_mask,
            });
            prev_pb_in = pb_in;
            prev_pb_out = pb_out;
            prev_cia2_pa = cia2_pa;
        }

        // The LOAD/serial path lives in $F4xx (LOAD), $ED/$EE (IEC), $E1
        // (LOAD vector). Once we see it, arm the end detector.
        let page = c64_pc >> 8;
        if matches!(page, 0xF4 | 0xF5 | 0xED | 0xEE | 0xE1) {
            load_started = true;
        }
        // End of the attempt: back in the BASIC READY loop ($E5xx) for a while
        // after the LOAD ran.
        if load_started && page == 0xE5 {
            ready_streak += 1;
            if ready_streak > 1_500 {
                deadlock_at = Some(i);
                break;
            }
        } else {
            ready_streak = 0;
        }
    }

    // ── Final state ──
    let c64_pc = session.machine().machine().cpu().regs.pc;
    let drive = session.machine().drive_1581().expect("1581 present");
    let d_pc = drive.cpu().regs.pc;
    let cia = drive.cia();
    let (din_d, din_c, din_a) = drive_inputs(cia.pb_in);
    let (dout_d, dout_c, dout_a) = drive_outputs(cia.port_b_drive_state());

    println!("\n=== solo 1581 LOAD handshake trace ===");
    println!("deadlock detected at sample: {deadlock_at:?}");
    println!("FLAG (ATN) interrupt asserts during window: {flag_asserts}");

    println!("\n-- C64 CPU PC pages visited --");
    print!("  ");
    for p in &c64_pages {
        print!("${p:02X} ");
    }
    println!();

    println!("-- 1581 CPU PC pages visited --");
    print!("  ");
    for p in &drive_pages {
        print!("${p:02X} ");
    }
    println!();
    // $AE is the wait loop itself; the real ATN handler / IRQ dispatch lives in
    // $AC/$AD/$DA/$FF. Exclude $AE so this isn't a false positive.
    let ran_atn = drive_pages
        .iter()
        .any(|&p| p == 0xAC || p == 0xAD || p == 0xDA || p == 0xDB || p == 0xFF);
    println!("  1581 ran its ATN/command-handler region ($ACxx/$ADxx/$DAxx/$FFxx): {ran_atn}");

    println!("\n-- 1581 IEC OUTPUT states seen (PB1 DATA-out, PB3 CLK-out, PB4 ATNA) --");
    for s in &drive_out_states {
        let (d, c, a) = drive_outputs(*s);
        println!("  ${s:02X}: DATAo={d} CLKo={c} ATNA={a}");
    }

    println!("\n-- 1581 IEC INPUT states seen (PB0 DATA-in, PB2 CLK-in, PB7 ATN-in) --");
    for s in &drive_in_states {
        let (d, c, a) = drive_inputs(*s);
        println!("  ${s:02X}: DATAi={d} CLKi={c} ATNi={a}");
    }

    println!(
        "\n-- IEC line transitions ({} captured) --",
        transitions.len()
    );
    println!(
        "   #     c64pc d_pc   drive: DATAi/CLKi/ATNi  DATAo/CLKo/ATNA   C64pa: DATAo/CLKo/ATNo  flag icrS icrM"
    );
    for s in transitions.iter().take(200) {
        let (di, ci, ai) = drive_inputs(s.pb_in);
        let (dobit, cobit, aobit) = drive_outputs(s.pb_out);
        let (cdo, cco, cao) = c64_outputs(s.cia2_pa);
        println!(
            "  {:>5}  ${:04X} ${:04X}         {di}/{ci}/{ai}            {dobit}/{cobit}/{aobit}      ${:02X}   {cdo}/{cco}/{cao}         {}   ${:02X}  ${:02X}",
            s.i,
            s.c64_pc,
            s.d_pc,
            s.cia2_pa,
            u8::from(s.flag),
            s.icr_s,
            s.icr_m
        );
    }

    println!("\n-- final stuck state --");
    println!("  C64  PC=${c64_pc:04X}");
    println!(
        "  1581 PC=${d_pc:04X} P=${:02X} (I-flag={})",
        drive.cpu().regs.p,
        (drive.cpu().regs.p >> 2) & 1
    );
    println!(
        "  1581 CIA: flag_pin={} icr_mask=${:02X} icr_status=${:02X} irq={}",
        u8::from(cia.flag),
        cia.icr_mask(),
        cia.icr_status(),
        u8::from(cia.irq)
    );
    println!("  1581 DATA-in={din_d} CLK-in={din_c} ATN-in={din_a}");
    println!("  1581 DATA-out={dout_d} CLK-out={dout_c} ATNA={dout_a}");
    println!("  screen:");
    if let Ok(Some(r)) = provider.query(session.machine(), "screen.text.lines") {
        println!("  {:?}", r.value);
    }
    if let Ok(Some(r)) = provider.query(session.machine(), "cia2.pa") {
        println!("  C64 CIA2.PA = {:?}", r.value);
    }
}
