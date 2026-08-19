//! Throwaway probe.
use machine_sinclair_zx80::Zx80;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
#[ignore]
fn probe() {
    let path =
        PathBuf::from(std::env::var("HOME").unwrap()).join(".emu198x/roms/sinclair-zx80/zx80.rom");
    let rom = std::fs::read(&path).expect("real ZX80 ROM");
    let mut m = Zx80::new(rom, 16 * 1024).expect("init");
    let frames: usize = std::env::var("FRAMES")
        .ok()
        .and_then(|f| f.parse().ok())
        .unwrap_or(100);
    for _ in 0..frames {
        m.run_frame();
    }
    let ev: Vec<String> = m
        .video_events()
        .iter()
        .map(|(k, l)| format!("{k}{l}"))
        .collect();
    println!("events (kind+line): {}", ev.join(" "));
    let fb = m.framebuffer();
    let mut h: HashMap<u32, usize> = HashMap::new();
    for &p in fb {
        *h.entry(p).or_default() += 1;
    }
    let mut v: Vec<_> = h.into_iter().collect();
    v.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    let tr = m.trace();
    println!("--- last frame trace ({} entries) ---", tr.len());
    let mut addrs: HashMap<u16, usize> = HashMap::new();
    for &(k, _, a) in tr {
        if k == 'F' {
            *addrs.entry(a).or_default() += 1;
        }
    }
    let mut av: Vec<_> = addrs.into_iter().collect();
    av.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    println!("distinct display-fetch addrs: {}", av.len());
    for (a, n) in av.iter().take(6) {
        println!("   {a:#06X} x{n}");
    }
    let seg: Vec<String> = tr
        .iter()
        .filter(|&&(_, l, _)| (228..=238).contains(&l))
        .map(|&(k, l, a)| format!("{k}[{l} @ {a:#06X}]"))
        .collect();
    println!("lines 228-238: {}", seg.join(" "));
    let last = tr.last().copied();
    println!("last trace entry: {last:?}");
    let (ov, vst, vsp, pc2, fo) = m.video_counts();
    println!("overflow={ov} vsync_start={vst} vsync_stop={vsp} paint_calls={pc2} forced={fo}");
    let (f, r, pc, px) = m.video_debug();
    println!("paint lines {f}..{r}   paint x {pc}..{px}");
    println!("frames={frames} pixels={} distinct={}", fb.len(), v.len());
    for (col, n) in v.iter().take(3) {
        println!(
            "  {:#010X}  {n}  ({:.2}%)",
            col,
            100.0 * *n as f64 / fb.len() as f64
        );
    }
    let w = 320usize;
    let rows = fb
        .chunks(w)
        .enumerate()
        .filter(|(_, r)| r.iter().any(|&p| p == 0xFF000000))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    println!(
        "ink rows: {:?}..{:?} ({} rows)",
        rows.first(),
        rows.last(),
        rows.len()
    );
}
