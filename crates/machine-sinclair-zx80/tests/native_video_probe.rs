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
    let fb = m.framebuffer();
    let mut h: HashMap<u32, usize> = HashMap::new();
    for &p in fb {
        *h.entry(p).or_default() += 1;
    }
    let mut v: Vec<_> = h.into_iter().collect();
    v.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
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
