// Run from the baseline runtime examples directory at source 49956980d.
use emu198x_shell::{HostIo,MachineCore,MachineTime,MediaImage,MediaKind,MediaSet};
use runtime_nintendo_game_boy::{GameBoyRuntime,Model};
#[path="../tests/common/mod.rs"] mod common;
fn main() {
    let output=std::path::PathBuf::from(std::env::args().nth(1).expect("fixture directory"));
    std::fs::create_dir_all(&output).expect("fixture directory");
    for model in Model::ALL {
        let mut runtime=GameBoyRuntime::blank(model);
        let bytes=common::loop_rom();
        let mut media=MediaSet::new();media.push(MediaImage::new("cartridge",MediaKind::Cartridge,&bytes));
        runtime.load_media(&media).expect("original synthetic cartridge");
        let(mut frame_sink,mut audio_sink,mut trace_sink)=common::null_host_buffers();
        let mut host=HostIo{input_events:&[],frame_sink:&mut frame_sink,audio_sink:&mut audio_sink,trace_sink:&mut trace_sink};
        runtime.run_until(MachineTime::new(2048),&mut host).expect("run baseline");
        let saved=runtime.snapshot().expect("v3 snapshot");assert_eq!(saved[0],3);
        std::fs::write(output.join(format!("{}.state",model.variant_id())),&saved).expect("write v3 fixture");
        println!("{} {}",model.variant_id(),saved.len());

    }
}
