//! Capture an explicit board/RAM configuration using immutable local firmware.
use common_commodore_amiga::RamConfig;
use emu198x_peripheral_commodore_amiga_floppy::Adf;
use emu198x_shell::{HeadlessSession, MachineCore, PixelFormat};
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufWriter, Read},
    path::PathBuf,
};
fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut hex, "{byte:02x}").expect("String write");
    }
    hex
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 && args.len() != 6 {
        return Err("usage: fixed_ocs_boot_capture <owner-id> <firmware> <chip-kib> <fields> <output.png> [original-kickstart-disk.zip]".into());
    }
    let model = Model::from_variant_id(&args[0]).ok_or("unknown board")?;
    let firmware = std::fs::read(&args[1])?;
    let ram = RamConfig {
        chip_kb: args[2].parse()?,
        slow_kb: 0,
        fast_kb: 0,
    };
    let config = model.config().with_ram(ram);
    let mut runtime = AmigaRuntimeKind::with_config(config, firmware.clone())?;
    let mut disk_hash = None;
    if let Some(path) = args.get(5) {
        let mut zip = zip::ZipArchive::new(File::open(path)?)?;
        let candidates: Vec<_> = zip
            .file_names()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".adf"))
            .map(|name| name.into_owned())
            .collect();
        if candidates.len() != 1 {
            return Err("original ZIP must contain exactly one ADF".into());
        }
        let mut bytes = Vec::new();
        zip.by_name(&candidates[0])?.read_to_end(&mut bytes)?;
        disk_hash = Some(sha256(&bytes));
        runtime.insert_floppy0(Adf::from_bytes(bytes)?, false);
    }
    let mut session = HeadlessSession::new(runtime, model.frame_ticks());
    let fields: u32 = args[3].parse()?;
    let mut handoff_field = None;
    for field in 1..=fields {
        session.run_frames(1)?;
        if model.is_a1000()
            && handoff_field.is_none()
            && session
                .machine()
                .memory_diagnostic_snapshot()
                .rom
                .a1000
                .is_some_and(|r| r.wom_locked)
        {
            handoff_field = Some(field);
            session.machine_mut().eject_floppy0();
        }
    }
    if model.is_a1000() && disk_hash.is_some() && handoff_field.is_none() {
        return Err("A1000 did not complete the native WOM handoff within the field bound".into());
    }
    let frame = session.latest_frame().ok_or("no completed native frame")?;
    if frame.format != PixelFormat::Rgba8888 {
        return Err("native capture requires RGBA pixels".into());
    }
    let output = PathBuf::from(&args[4]);
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(&output)?),
        frame.width,
        frame.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&frame.pixels)?;
    writer.finish()?;
    let record = serde_json::json!({
        "owner_id": session.machine().profile().machine_id.as_str(),
        "legacy_profile_id": model.profile_id(), "chip_kib": ram.chip_kb,
        "slow_kib": ram.slow_kb, "fast_kib": ram.fast_kb,
        "firmware_sha256": sha256(&firmware), "kickstart_adf_sha256": disk_hash,
        "completed_fields": fields, "wom_handoff_field": handoff_field,
        "tick": session.machine().tick_count(), "width": frame.width, "height": frame.height,
        "rgba_sha256": sha256(&frame.pixels), "png_sha256": sha256(&std::fs::read(&output)?),
        "memory": session.machine().memory_diagnostic_snapshot(),
    });
    std::fs::write(
        output.with_extension("json"),
        serde_json::to_vec_pretty(&record)?,
    )?;
    println!("{record}");
    Ok(())
}
