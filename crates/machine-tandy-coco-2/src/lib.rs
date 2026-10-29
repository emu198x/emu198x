//! Fixed CoCo2 Ntsc board over shared Dragon/CoCo wiring.
pub use common_dragon::{
    AddressRange, CpuInterruptAcceptTrace, CpuInterruptKind, CpuInterruptLineTrace,
    CpuRegisterTrace, DRAGON_AUDIO_SAMPLE_RATE, DRAGON_CPU_HZ, DRAGON_FRAME_CYCLES,
    DRAGON_JOYSTICK_CENTER, DRAGON_JOYSTICK_MAX, DRAGON_JOYSTICK_MIN, DRAGON_MASTER_HZ,
    DeviceAccess, DeviceRegion, DragonCartridgeKind, DragonDiskError, DragonDiskSummary,
    DragonHardwareModel, DragonJoystickAxis, DragonJoystickError, DragonKey, DragonKeyboard,
    DragonProgramLoadError, DragonSnapshotPeripherals, DragonSnapshotRegisters, DragonVideoPhase,
    FetchTrace, KeyboardMatrixError, MatrixKey, MemoryEvent, MemoryWriteTrace, NTSC_FRAME_CYCLES,
    PiaSignalTrace, RAM_SIZE, ROM_SIZE, ReadonlyWrite, RunOptions, RunReport, StopReason,
    VdgModeWriteTrace, VdgSampleTrace, VideoStandard, WatchedFetchTrace, disk_serde,
};
use common_dragon::{DragonBoard, DragonCore, DragonHardware};
/// Static physical board policy.
#[derive(Debug, Clone)]
pub struct Board;
impl DragonBoard for Board {
    const HARDWARE: DragonHardware = DragonHardware::CoCo2;
    const COLD_BOOT_MODEL: DragonHardwareModel = DragonHardwareModel::CoCo64k;
    const VIDEO: VideoStandard = VideoStandard::Ntsc;
}
/// This fixed physical machine.
pub type Dragon32 = DragonCore<Board>;
