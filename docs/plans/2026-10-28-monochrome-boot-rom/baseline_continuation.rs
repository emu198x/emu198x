use emu198x_shell::{HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};
#[path="../tests/common/mod.rs"] mod common;
fn main() {
    let output=std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));
    for model in [Model::Dmg,Model::Cgb] {
        let old=std::fs::read(output.join(format!("{}.state",model.variant_id()))).expect("baseline fixture");
        let mut runtime=GameBoyRuntime::blank(model);runtime.restore(&old).expect("baseline v3");
        let before=runtime.time().get();
        let target=before+model.frame_ticks();
        let(mut frame_sink,mut audio_sink,mut trace_sink)=common::null_host_buffers();
        let mut host=HostIo{input_events:&[],frame_sink:&mut frame_sink,audio_sink:&mut audio_sink,trace_sink:&mut trace_sink};
        runtime.run_until(MachineTime::new(target),&mut host).expect("baseline continuation");
        assert!(runtime.time().get()>before);
        let saved=runtime.snapshot().expect("v3");assert_eq!(saved[0],3);
        std::fs::write(output.join(format!("{}-next.state",model.variant_id())),saved).expect("write continuation");
        println!("{} {} {}",model.variant_id(),before,runtime.time().get());
    }
}
