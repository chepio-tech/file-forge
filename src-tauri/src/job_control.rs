//! Follows and stops running compressions: one cancellation counter shared by all of them, and a per-job
//! `Control` that forwards throttled progress to the webview.

// Core
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use fileforge_core::pdf::{Control, Progress};

/// Count updates within one stage go out at most this often; stage changes and completion always go out.
const MIN_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

/// Cancels every compression started before the request and never a later one, without tracking jobs: each job
/// keeps the generation it started in, and cancelling moves the generation on.
#[derive(Debug, Default)]
pub struct Cancellation {
    generation: AtomicU64,
}

impl Cancellation {
    /// Taken when a job starts, before it waits for the work slot.
    pub fn ticket(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn cancel_started(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self, ticket: u64) -> bool {
        self.generation.load(Ordering::SeqCst) != ticket
    }
}

/// The engine's view of one compression: cancelled through [`Cancellation`], progress sent through `send`.
pub struct JobControl<'a, F> {
    cancellation: &'a Cancellation,
    ticket: u64,
    throttle: Mutex<Throttle>,
    send: F,
}

impl<'a, F: Fn(Progress) + Sync> JobControl<'a, F> {
    pub fn new(cancellation: &'a Cancellation, ticket: u64, send: F) -> Self {
        Self { cancellation, ticket, throttle: Mutex::default(), send }
    }
}

impl<F: Fn(Progress) + Sync> Control for JobControl<'_, F> {
    fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled(self.ticket)
    }

    fn report(&self, progress: Progress) {
        // Sending under the lock keeps updates in order even if reports come from several threads.
        let mut throttle = self.throttle.lock().unwrap_or_else(PoisonError::into_inner);
        if throttle.due(progress, Instant::now()) {
            (self.send)(progress);
        }
    }
}

#[derive(Debug, Default)]
struct Throttle {
    last: Option<(Progress, Instant)>,
}

impl Throttle {
    fn due(&mut self, progress: Progress, now: Instant) -> bool {
        let due = match self.last {
            None => true,
            Some((last, sent_at)) => {
                last.stage != progress.stage
                    || last.total != progress.total
                    || progress.done == progress.total
                    || now.duration_since(sent_at) >= MIN_UPDATE_INTERVAL
            }
        };
        if due {
            self.last = Some((progress, now));
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use fileforge_core::pdf::Stage;

    use super::*;

    fn at(stage: Stage, done: u32, total: u32) -> Progress {
        Progress { stage, done, total }
    }

    #[test]
    fn cancelling_stops_started_jobs_but_not_later_ones() {
        let cancellation = Cancellation::default();
        let running = cancellation.ticket();
        let waiting = cancellation.ticket();

        cancellation.cancel_started();
        let later = cancellation.ticket();

        assert!(cancellation.is_cancelled(running));
        assert!(cancellation.is_cancelled(waiting));
        assert!(!cancellation.is_cancelled(later));
    }

    #[test]
    fn a_cancel_with_nothing_running_affects_nothing() {
        let cancellation = Cancellation::default();
        cancellation.cancel_started();
        cancellation.cancel_started();

        let control = JobControl::new(&cancellation, cancellation.ticket(), |_| {});

        assert!(!control.is_cancelled());
    }

    #[test]
    fn stage_changes_and_completion_always_go_out_counts_at_most_every_interval() {
        let mut throttle = Throttle::default();
        let start = Instant::now();
        let soon = start + MIN_UPDATE_INTERVAL / 2;
        let later = start + MIN_UPDATE_INTERVAL;

        assert!(throttle.due(at(Stage::Loading, 0, 0), start));
        assert!(throttle.due(at(Stage::Images, 0, 0), start), "new stage");
        assert!(throttle.due(at(Stage::Images, 0, 40), start), "stage became counted");
        assert!(!throttle.due(at(Stage::Images, 1, 40), soon), "too soon");
        assert!(throttle.due(at(Stage::Images, 2, 40), later), "interval passed");
        assert!(!throttle.due(at(Stage::Images, 3, 40), later));
        assert!(throttle.due(at(Stage::Images, 40, 40), later), "stage finished");
        assert!(throttle.due(at(Stage::Streams, 0, 900), later), "next stage");
    }

    #[test]
    fn progress_is_forwarded_through_the_throttle() {
        let cancellation = Cancellation::default();
        let sent = Mutex::new(Vec::new());
        let control = JobControl::new(&cancellation, cancellation.ticket(), |progress| {
            sent.lock().unwrap_or_else(PoisonError::into_inner).push(progress);
        });

        for done in 0..=500 {
            control.report(at(Stage::Streams, done, 500));
        }

        let sent = sent.into_inner().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(sent.first(), Some(&at(Stage::Streams, 0, 500)));
        assert_eq!(sent.last(), Some(&at(Stage::Streams, 500, 500)));
        assert!(sent.len() < 10, "{} updates for one fast stage", sent.len());
    }
}
