use emu_68000::M68000;

fn main() {
    let mut cpu = M68000::new();
    let mut accepted = 0u32;
    let mut total = 0u32;

    for op in 0u32..=0xFFFF {
        total += 1;
        if cpu.recipe_accepts_opcode(op as u16) {
            accepted += 1;
        }
    }

    let rejected = total - accepted;
    let pct = (accepted as f64) * 100.0 / (total as f64);
    println!(
        "recipe-accepted: {accepted} / {total} ({pct:.2}%), legacy: {rejected}"
    );
}
