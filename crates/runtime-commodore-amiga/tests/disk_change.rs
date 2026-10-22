//! Mount policy across the runtime enum used by script, MCP and native hosts.
use emu198x_shell::{DebugPrimitives, MachineCore, MediaImage, MediaKind, MediaSet, ResetKind};
use peripheral_commodore_amiga_floppy::DD;
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};

const MODELS: [Model; 8] = [
    Model::A500OcsPal,
    Model::A500OcsNtsc,
    Model::A500PlusEcsPal,
    Model::A500PlusEcsNtsc,
    Model::A1200AgaPal,
    Model::A1200AgaNtsc,
    Model::A1000OcsPal,
    Model::A1000OcsNtsc,
];

fn mount(runtime: &mut AmigaRuntimeKind, fill: u8) {
    let bytes = vec![fill; DD.len()];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &bytes));
    runtime.load_media(&media).expect("mount DD disk");
}

fn assert_change(runtime: &AmigaRuntimeKind, pending: bool) {
    assert_eq!(
        runtime.drive().status().disk_change,
        pending,
        "drive latch on {:?}",
        runtime.model()
    );
    assert_eq!(
        runtime.cia_a().peek(0) & 4 == 0,
        pending,
        "CIA-A /DSKCHANGE pin on {:?}",
        runtime.model()
    );
}

#[test]
fn startup_and_reset_preserve_the_firmware_mount_policy() {
    for model in MODELS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        mount(&mut runtime, 0);
        assert_change(&runtime, model.is_a1000());
        assert!(runtime.dbg_step() > 0, "advance before reset on {model:?}");
        assert!(runtime.time().get() > 0);
        runtime.reset(ResetKind::Hard);
        assert_eq!(runtime.time().get(), 0);
        assert_change(&runtime, model.is_a1000());
        assert!(runtime.drive().has_disk());
    }
}

#[test]
fn live_replacement_signals_change_even_before_the_first_video_frame() {
    for model in MODELS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        mount(&mut runtime, 0);
        assert!(runtime.dbg_step() > 0, "advance live runtime on {model:?}");
        assert!(runtime.time().get() > 0);
        mount(&mut runtime, 1);
        assert_change(&runtime, true);
        assert_eq!(
            runtime.save_floppy0_image().expect("mounted disk"),
            vec![1; DD.len()]
        );
        let saved = runtime.snapshot().expect("snapshot pending change");
        runtime.restore(&saved).expect("restore pending change");
        assert_change(&runtime, true);
        assert_eq!(saved, runtime.snapshot().expect("snapshot restored change"));
    }
}

#[test]
fn live_insertion_after_eject_preserves_change() {
    for model in MODELS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        mount(&mut runtime, 0);
        assert!(runtime.dbg_step() > 0);
        runtime.eject_media("floppy-0").expect("eject DF0");
        assert_change(&runtime, true);
        mount(&mut runtime, 1);
        assert_change(&runtime, true);
    }
}

#[test]
fn restore_reattaches_media_without_inventing_a_change() {
    for model in MODELS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        mount(&mut runtime, 0);
        assert!(runtime.dbg_step() > 0);
        let pending = runtime.drive().status().disk_change;
        let saved = runtime.snapshot().expect("snapshot mounted drive");
        runtime.eject_media("floppy-0").expect("eject DF0");
        runtime.restore(&saved).expect("restore mounted drive");
        assert_change(&runtime, pending);
        assert_eq!(saved, runtime.snapshot().expect("snapshot restored drive"));
    }
}

fn mount_in(runtime: &mut AmigaRuntimeKind, slot: &'static str, fill: u8) {
    let bytes = vec![fill; DD.len()];
    let mut media = MediaSet::new();
    media.push(MediaImage::new(slot, MediaKind::Disk, &bytes));
    runtime.load_media(&media).expect("mount DD disk");
}

fn latch(runtime: &AmigaRuntimeKind, unit: usize) -> bool {
    runtime
        .drives()
        .drive(unit)
        .expect("drive fitted")
        .status()
        .disk_change
}

/// DF1 to DF3 share DF0's mount policy. Their latches are checked directly:
/// the CIA-A pin follows whichever drive is selected.
#[test]
fn df1_startup_and_live_insertion_follow_the_same_policy() {
    for model in MODELS {
        let mut runtime = AmigaRuntimeKind::blank(model);
        mount_in(&mut runtime, "floppy-1", 0);
        assert_eq!(latch(&runtime, 1), model.is_a1000(), "startup on {model:?}");
        assert!(runtime.dbg_step() > 0);
        mount_in(&mut runtime, "floppy-1", 1);
        assert!(latch(&runtime, 1), "live replacement on {model:?}");
        runtime.eject_media("floppy-1").expect("eject DF1");
        mount_in(&mut runtime, "floppy-1", 2);
        assert!(latch(&runtime, 1), "insertion after eject on {model:?}");
        runtime.reset(ResetKind::Hard);
        assert_eq!(latch(&runtime, 1), model.is_a1000(), "reset on {model:?}");
    }
}
