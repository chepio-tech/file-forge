// Core
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};
// Domain
use super::Report;
use super::protocol::{self, Event, Protocol};

pub(super) struct Worker {
    child: Child,
    stdout: Option<JoinHandle<Result<Protocol, String>>>,
    stderr: Option<JoinHandle<Result<String, String>>>,
    reaped: bool,
}

impl Worker {
    pub fn spawn(mut command: Command, events: SyncSender<Event>, report: &mut Report) -> Result<Self, String> {
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        report.child_id = Some(child.id());
        let mut owner = Self { child, stdout: None, stderr: None, reaped: false };
        let setup = (|| {
            let stdout = owner.child.stdout.take().ok_or("missing stdout pipe")?;
            let stderr = owner.child.stderr.take().ok_or("missing stderr pipe")?;
            let stdout_events = events.clone();
            owner.stdout = Some(
                thread::Builder::new()
                    .name("avif-stdout".into())
                    .spawn(move || protocol::stdout(stdout, stdout_events))
                    .map_err(|error| error.to_string())?,
            );
            owner.stderr = Some(
                thread::Builder::new()
                    .name("avif-stderr".into())
                    .spawn(move || protocol::stderr(stderr, events))
                    .map_err(|error| error.to_string())?,
            );
            Ok::<_, String>(())
        })();
        if let Err(error) = setup {
            report.child_reaped = owner.stop().is_ok();
            return Err(error);
        }
        Ok(owner)
    }

    pub fn poll(&mut self) -> Result<Option<ExitStatus>, String> {
        let status = self.child.try_wait().map_err(|error| error.to_string())?;
        self.reaped |= status.is_some();
        Ok(status)
    }

    pub fn stop(&mut self) -> Result<ExitStatus, String> {
        if !self.reaped {
            // A concurrent exit can make kill redundant. wait remains necessary to reap ownership.
            let _ = self.child.kill();
        }
        let status = self.child.wait().map_err(|error| error.to_string())?;
        self.reaped = true;
        Ok(status)
    }

    pub fn readers(&mut self) -> (Result<Protocol, String>, Result<String, String>) {
        let stdout = self
            .stdout
            .take()
            .ok_or("missing stdout reader".to_owned())
            .and_then(|reader| reader.join().map_err(|_| "stdout reader panicked".to_owned()))
            .and_then(|value| value);
        let stderr = self
            .stderr
            .take()
            .ok_or("missing stderr reader".to_owned())
            .and_then(|reader| reader.join().map_err(|_| "stderr reader panicked".to_owned()))
            .and_then(|value| value);
        (stdout, stderr)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.stop();
        }
        // The trusted decoder has no descendants that can retain these pipe handles.
        let _ = self.readers();
    }
}
