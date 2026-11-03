//! Bounded CLI/MCP children, following Spectrum's startup-test polling pattern.
use std::{
    io::{self, Write},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

struct OwnedChild(Option<Child>);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn wait(child: &mut Child, timeout: Duration, case: &str) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            let pid = child.id();
            child.kill()?;
            let status = child.wait()?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "{case}: deadline {timeout:?} exceeded; child {pid} killed and reaped ({status})"
                ),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

pub fn output(command: &mut Command, input: Option<&[u8]>, case: &str) -> io::Result<Output> {
    let mut child = OwnedChild(Some(
        command
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    ));
    let process = child.0.as_mut().expect("owned child");
    if let Some(bytes) = input {
        process
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(bytes)?;
    }
    wait(process, Duration::from_secs(15), case)?;
    child.0.take().expect("finished child").wait_with_output()
}
