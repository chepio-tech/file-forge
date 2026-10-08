//! How a caller follows and stops a running compression, shared by every engine.

// Core
use std::sync::{Mutex, PoisonError};

use serde::Serialize;

/// Pipeline stages, reported in this order. PDFs: `Loading`, `Structure`, `Images` (lossy presets only), `Streams`,
/// `Saving`, `Verifying`. Images: `Loading`, `Encoding`, `Verifying`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Loading,
    Structure,
    Images,
    Streams,
    Encoding,
    Saving,
    Verifying,
}

/// Where a compression is. `total == 0` means the stage is not counted; otherwise `done` grows from 0 to `total`
/// and never goes back within a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    pub done: u32,
    pub total: u32,
}

/// Hooks called while a compression runs, also from worker threads: keep both cheap.
pub trait Control: Sync {
    /// Checked between stages and before each unit of work (image, stream, encoding pass). Once it returns `true` it
    /// must keep doing so; the compression then ends with the engine's `Cancelled` error.
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report(&self, _progress: Progress) {}
}

/// Never cancelled, reports nothing.
impl Control for () {}

/// Counts finished items of one stage and reports them in order, whichever thread finishes them.
pub(crate) struct Counter<'a> {
    control: &'a dyn Control,
    stage: Stage,
    total: u32,
    done: Mutex<u32>,
}

impl<'a> Counter<'a> {
    /// Reports the stage start (`done = 0`).
    pub fn start(control: &'a dyn Control, stage: Stage, total: usize) -> Self {
        let total = u32::try_from(total).unwrap_or(u32::MAX);
        control.report(Progress { stage, done: 0, total });
        Self { control, stage, total, done: Mutex::new(0) }
    }

    pub fn advance(&self) {
        // Reporting under the lock keeps the reported counts increasing across threads.
        let mut done = self.done.lock().unwrap_or_else(PoisonError::into_inner);
        *done = done.saturating_add(1).min(self.total);
        self.control.report(Progress { stage: self.stage, done: *done, total: self.total });
    }
}
