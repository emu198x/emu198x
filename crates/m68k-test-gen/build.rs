use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let musashi_dir = Path::new("musashi");

    // Step 1: Compile m68kmake from source
    let m68kmake_src = musashi_dir.join("m68kmake.c");
    let m68kmake_bin = out_dir.join("m68kmake");

    let status = Command::new("cc")
        .args([
            m68kmake_src.to_str().expect("valid path"),
            "-o",
            m68kmake_bin.to_str().expect("valid path"),
        ])
        .status()
        .expect("failed to compile m68kmake");
    assert!(status.success(), "m68kmake compilation failed");

    // Step 2: Run m68kmake to generate m68kops.c and m68kops.h
    //   m68kmake <output_path> <input_file>
    let m68k_in = musashi_dir.join("m68k_in.c");
    let status = Command::new(&m68kmake_bin)
        .args([
            out_dir.to_str().expect("valid path"),
            m68k_in.to_str().expect("valid path"),
        ])
        .status()
        .expect("failed to run m68kmake");
    assert!(status.success(), "m68kmake code generation failed");

    // Step 3: Compile Musashi via the cc crate
    cc::Build::new()
        .file(musashi_dir.join("m68kcpu.c"))
        .file(musashi_dir.join("m68kdasm.c"))
        .file(musashi_dir.join("m68kfpu.c"))
        .file(musashi_dir.join("softfloat/softfloat.c"))
        .file(out_dir.join("m68kops.c"))
        // Use our custom m68kconf.h from the musashi/ directory
        .include(musashi_dir)
        // Generated m68kops.h lives in OUT_DIR
        .include(&out_dir)
        // Suppress warnings from Musashi's C89 code
        .warnings(false)
        .compile("musashi");

    // Tell cargo to re-run if sources change
    println!("cargo:rerun-if-changed=musashi/");
    println!("cargo:rerun-if-changed=build.rs");
}
