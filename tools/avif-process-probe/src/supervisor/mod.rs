// Core
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};
// Services
use process::Worker;
use protocol::Event;
use staging::Staging;

mod process;
mod protocol;
mod staging;
#[cfg(test)]
mod tests;

pub const MAX_PIXELS: u64 = 32_000_000;
pub const MAX_ALLOCATION: u64 = 512 * 1024 * 1024;
pub const MAX_DEADLINE: Duration = Duration::from_secs(20);
const MAX_INPUT: u64 = 256 * 1024 * 1024;
const POLL: Duration = Duration::from_millis(2);

pub struct Options {
    pub decoder: PathBuf,
    pub input: PathBuf,
    pub output: PathBuf,
    pub deadline: Duration,
    pub cancel_after_decode: Option<Duration>,
    pub max_pixels: u64,
    pub allocation_budget: u64,
}

#[derive(Default)]
pub struct Cancellation(Mutex<Option<Instant>>);

impl Cancellation {
    pub fn request(&self) {
        self.request_at(Instant::now());
    }

    fn request_at(&self, instant: Instant) {
        let mut requested = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *requested = Some(requested.map_or(instant, |previous| previous.min(instant)));
    }

    fn requested(&self) -> Option<Instant> {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Cancelled,
    Deadline,
    Failed,
}

#[derive(Debug)]
pub struct Report {
    pub outcome: Outcome,
    pub child_id: Option<u32>,
    pub child_reaped: bool,
    pub published: bool,
    pub cleanup_ok: bool,
    pub geometry: Option<(u64, u64, u64)>,
    pub elapsed_ms: f64,
    pub cancel_reap_ms: Option<f64>,
    pub cancel_settle_ms: Option<f64>,
    pub error: String,
    pub stderr: String,
    reaped_at: Option<Instant>,
}

impl Report {
    fn new() -> Self {
        Self {
            outcome: Outcome::Failed,
            child_id: None,
            child_reaped: false,
            published: false,
            cleanup_ok: true,
            geometry: None,
            elapsed_ms: 0.0,
            cancel_reap_ms: None,
            cancel_settle_ms: None,
            error: String::new(),
            stderr: String::new(),
            reaped_at: None,
        }
    }

    fn fail(&mut self, error: impl Into<String>) {
        self.outcome = Outcome::Failed;
        self.error = error.into();
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn validate(options: &Options) -> Result<(), String> {
    if options.deadline.is_zero()
        || options.deadline > MAX_DEADLINE
        || options.max_pixels == 0
        || options.max_pixels > MAX_PIXELS
        || options.allocation_budget == 0
        || options.allocation_budget > MAX_ALLOCATION
        || options.cancel_after_decode.is_some_and(|delay| delay > MAX_DEADLINE)
    {
        return Err("invalid supervisor resource bounds".into());
    }
    let metadata = fs::metadata(&options.input).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT {
        return Err("input must be a regular file of at most 256 MiB".into());
    }
    Ok(())
}

pub fn run(options: &Options, cancellation: &Cancellation) -> Report {
    run_with(options, cancellation, |staged| {
        let mut command = Command::new(&options.decoder);
        command
            .arg(&options.input)
            .arg(staged)
            .arg("--max-pixels")
            .arg(options.max_pixels.to_string())
            .arg("--allocation-budget")
            .arg(options.allocation_budget.to_string());
        command
    })
}

fn run_with(options: &Options, cancellation: &Cancellation, command: impl FnOnce(&Path) -> Command) -> Report {
    let start = Instant::now();
    let mut report = Report::new();
    if cancellation.requested().is_some() {
        report.outcome = Outcome::Cancelled;
        report.elapsed_ms = millis(start.elapsed());
        return report;
    }
    let result = (|| {
        validate(options)?;
        let staging = Staging::new(&options.output)?;
        supervise(command(&staging.output), options, cancellation, &staging, start, &mut report);
        // supervise has reaped the worker and joined the pipe readers before publication/cleanup.
        if report.outcome == Outcome::Completed {
            // Publication and accepting cancellation have a single ordering point.
            let requested = cancellation.0.lock().unwrap_or_else(PoisonError::into_inner);
            if requested.is_some() {
                report.outcome = Outcome::Cancelled;
            } else if start.elapsed() >= options.deadline {
                report.outcome = Outcome::Deadline;
            } else if let Err(error) = staging.publish() {
                report.fail(error);
            } else {
                report.published = true;
            }
        }
        if let Err(error) = staging.cleanup() {
            report.cleanup_ok = false;
            report.fail(format!("{}; staging cleanup failed: {error}", report.error));
        }
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        report.fail(error);
    }
    if report.outcome == Outcome::Cancelled {
        report.cancel_reap_ms = cancellation
            .requested()
            .zip(report.reaped_at)
            .map(|(requested, reaped)| millis(reaped.saturating_duration_since(requested)));
        report.cancel_settle_ms = cancellation.requested().map(|instant| millis(instant.elapsed()));
    }
    report.elapsed_ms = millis(start.elapsed());
    report
}

fn supervise(
    command: Command,
    options: &Options,
    cancellation: &Cancellation,
    staging: &Staging,
    start: Instant,
    report: &mut Report,
) {
    let (sender, events) = mpsc::sync_channel(4);
    let mut worker = match Worker::spawn(command, sender, report) {
        Ok(worker) => worker,
        Err(error) => {
            report.fail(error);
            return;
        }
    };
    let mut decoded_at: Option<Instant> = None;
    let mut natural_exit = None;
    loop {
        if let (Some(at), Some(delay)) = (decoded_at, options.cancel_after_decode) {
            let due = at + delay;
            if Instant::now() >= due {
                cancellation.request_at(due);
            }
        }
        if cancellation.requested().is_some() {
            report.outcome = Outcome::Cancelled;
            break;
        }
        if start.elapsed() >= options.deadline {
            report.outcome = Outcome::Deadline;
            break;
        }
        if let Ok(metadata) = fs::symlink_metadata(&staging.output)
            && (!metadata.file_type().is_file() || metadata.len() > options.max_pixels * 4)
        {
            report.fail("worker staging output exceeds bounds or is not a regular file");
            break;
        }
        match worker.poll() {
            Ok(Some(status)) => {
                natural_exit = Some(status);
                break;
            }
            Ok(None) => {}
            Err(error) => {
                report.fail(error);
                break;
            }
        }
        match events.recv_timeout(POLL) {
            Ok(Event::Started(at)) => decoded_at = Some(at),
            Ok(Event::Fault) => {
                report.fail("decoder protocol/capture failed");
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(POLL),
        }
    }
    match worker.stop() {
        Ok(_) => {
            report.child_reaped = true;
            report.reaped_at = Some(Instant::now());
        }
        Err(error) => report.fail(format!("could not reap decoder: {error}")),
    }
    let (protocol, stderr) = worker.readers();
    if !report.child_reaped {
        return;
    }
    if let Ok(stderr) = &stderr {
        report.stderr.clone_from(stderr);
    }
    let Some(status) = natural_exit else { return };
    // A fast child can exit before the main loop consumes its stage event.
    if let (Ok(protocol), Some(delay)) = (&protocol, options.cancel_after_decode)
        && let Some(due) = protocol.started.map(|at| at + delay)
        && Instant::now() >= due
    {
        cancellation.request_at(due);
    }
    if cancellation.requested().is_some() {
        report.outcome = Outcome::Cancelled;
        return;
    }
    if let Err(error) = stderr {
        report.fail(error);
        return;
    }
    if !status.success() {
        report.fail(format!("decoder exited unsuccessfully: {status}"));
        return;
    }
    let result = protocol.and_then(|protocol| protocol.geometry(options.max_pixels)).and_then(|geometry| {
        staging.validate(geometry.2)?;
        Ok(geometry)
    });
    match result {
        Ok(geometry) => {
            report.geometry = Some(geometry);
            report.outcome = Outcome::Completed;
        }
        Err(error) => report.fail(error),
    }
}
