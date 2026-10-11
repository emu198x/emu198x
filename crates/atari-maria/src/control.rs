//! CPU-written register delivery on the native clock. CTRL retains a holding
//! value until the CPU write selection ends; consecutive CTRL writes can
//! replace that value before it reaches DMA or playback.

use serde::{Deserialize, Serialize};

use super::Maria;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Control {
    pub write_strobe: bool,
    pub ctrl_selected: bool,
    pub pending_ctrl: Option<u8>,
    pub wsync_strobe: bool,
    pub wsync_wait: bool,
    pub wsync_held: bool,
}

impl Maria {
    pub(super) fn tick_registers(&mut self, phase2: bool, drive: bool) {
        let selected = !drive && self.address_in & 0xfce0 == 0x0020 && self.write_in;
        // READY has its own phase-1-transparent output latch. The WSYNC
        // selection persists until another phase-2 access replaces it, so a
        // write spanning line reset can keep the internal wait asserted.
        if !phase2 {
            self.control.wsync_held = self.control.wsync_wait;
        }
        if self.control.wsync_strobe {
            self.control.wsync_wait = true;
        } else if matches!(self.native_cycle, 824 | 825) {
            self.control.wsync_wait = false;
        }
        if phase2 {
            self.control.wsync_strobe = selected && self.address_in & 0x1f == 4;
        }
        self.wsync = if self.clock.phase2 {
            self.control.wsync_held
        } else {
            self.control.wsync_wait && !matches!(self.native_cycle, 823 | 824)
        };
        let old_ctrl_selected = self.control.ctrl_selected;
        if self.native_cycle % 2 == 1 {
            self.control.ctrl_selected = selected && self.address_in & 0x1f == 0x1c;
        }
        if self.control.write_strobe && selected {
            let register = (self.address_in & 0x1f) as u8;
            if register == 0x1c {
                self.control.pending_ctrl = Some(self.write_data_in);
            } else if register != 4 {
                self.write(register, self.write_data_in);
            }
        }
        if !phase2
            && self.native_cycle.is_multiple_of(2)
            && !old_ctrl_selected
            && let Some(value) = self.control.pending_ctrl.take()
        {
            self.ctrl = value;
        }
        self.control.write_strobe = self.phi2;
    }
}
