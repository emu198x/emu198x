//! Messages to the machine binary, written on standard output.

use std::cell::RefCell;
use std::io::{self, BufWriter, Stdout};
use std::rc::Rc;

use emu198x_ui::window_protocol::{self, AppCommand, FromWindow};

/// The window's end of the pipe. Clones share it; everything that writes
/// runs on GTK's main thread.
#[derive(Clone)]
pub(crate) struct Outbox(Rc<RefCell<BufWriter<Stdout>>>);

impl Outbox {
    pub(crate) fn new() -> Self {
        Self(Rc::new(RefCell::new(BufWriter::new(io::stdout()))))
    }

    /// Send `message`. A machine binary that has gone is noticed by the
    /// reader, which closes the window, so a failed write is dropped here.
    pub(crate) fn send(&self, message: &FromWindow) {
        if let Ok(mut out) = self.0.try_borrow_mut() {
            let _ = window_protocol::write_message(&mut *out, message);
        }
    }

    /// Send a menu command.
    pub(crate) fn command(&self, command: AppCommand) {
        self.send(&FromWindow::Command(command));
    }
}
