//! In-app updates (ADR-0013). A check reads the release manifest configured under `plugins.updater` in
//! `tauri.conf.json`; installing downloads the update found by the last check, verifies its signature, installs it
//! and restarts. The webview only asks to check or to install: it never supplies a version, URL or file.

// Core
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
// Types
use crate::error::AppError;
use crate::results::ResultStore;

/// The manifest is a few kilobytes: a check that takes longer is treated as offline.
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
/// Installers are tens of megabytes; a stalled download must end with an error instead of spinning forever.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    /// The newer version the release manifest offers; `None` when this one is current.
    pub available_version: Option<String>,
}

/// The update found by the last check, waiting for the user to install it.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

impl PendingUpdate {
    /// Plain data without cross-field invariants: recovering from a poisoned lock is safe.
    fn lock(&self) -> MutexGuard<'_, Option<Update>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub async fn check(app: &AppHandle) -> Result<UpdateStatus, AppError> {
    let handle = app.clone();
    let update = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        // Windows runs the installer and exits the process directly, without the app's exit event.
        .on_before_exit(move || handle.state::<ResultStore>().clear())
        .build()?
        .check()
        .await?;
    let available_version = update.as_ref().map(|update| update.version.clone());
    *app.state::<PendingUpdate>().lock() = update;
    Ok(UpdateStatus { current_version: app.package_info().version.to_string(), available_version })
}

/// Downloads, verifies and installs the pending update, then restarts the app. Returns only on failure, which leaves
/// the installed app and the session's results untouched.
pub async fn install(app: AppHandle, discard_unsaved: bool) -> Result<(), AppError> {
    let mut update = app
        .state::<PendingUpdate>()
        .lock()
        .clone()
        .ok_or_else(|| AppError::Update("no update was found by the last check".into()))?;
    // Refuse before downloading tens of megabytes; checked again under the work slot before installing.
    drop(claim(&app.state::<ResultStore>(), discard_unsaved)?);
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    // Checks the signature against `plugins.updater.pubkey` and the signed version against the manifest's.
    let bytes = update.download(|_, _| {}, || {}).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let results = app.state::<ResultStore>();
        // Held until the process exits: no compression or save can start between installing and restarting.
        let _slot = claim(&results, discard_unsaved)?;
        update.install(bytes)?;
        // Called off the main thread, `restart` goes through the exit event, which clears temp results (`lib.rs`).
        app.restart()
    })
    .await?
}

/// The work slot, if the app may restart now: nothing is running, and no result is unsaved unless the user agreed
/// to discard unsaved results.
fn claim(results: &ResultStore, discard_unsaved: bool) -> Result<MutexGuard<'_, ()>, AppError> {
    let slot = results.try_work_slot().ok_or(AppError::Busy)?;
    if !discard_unsaved && results.unsaved_count() > 0 {
        return Err(AppError::UnsavedResults);
    }
    Ok(slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ResultStore) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = ResultStore::open(dir.path().join("results")).expect("opens");
        (dir, store)
    }

    #[test]
    fn an_idle_session_without_results_may_restart() {
        let (_dir, results) = store();
        assert!(claim(&results, false).is_ok());
    }

    #[test]
    fn running_work_blocks_the_restart_even_when_discarding() {
        let (_dir, results) = store();
        let _running = results.work_slot();
        assert!(matches!(claim(&results, false), Err(AppError::Busy)));
        assert!(matches!(claim(&results, true), Err(AppError::Busy)));
    }

    #[test]
    fn unsaved_results_need_the_users_consent() {
        let (dir, results) = store();
        results.put(1, b"%PDF-1", "pdf").expect("stores");
        assert!(matches!(claim(&results, false), Err(AppError::UnsavedResults)));
        assert!(claim(&results, true).is_ok());

        results.mark_saved(1, dir.path().join("1.pdf"));
        assert!(claim(&results, false).is_ok());
    }

    #[test]
    fn the_claimed_slot_keeps_other_work_out() {
        let (_dir, results) = store();
        let slot = claim(&results, false).expect("free");
        assert!(results.try_work_slot().is_none());
        drop(slot);
        assert!(results.try_work_slot().is_some());
    }

    #[test]
    fn status_serializes_in_camel_case() {
        let status = UpdateStatus { current_version: "0.1.0".into(), available_version: Some("0.2.0".into()) };
        let json = serde_json::to_value(status).ok();
        assert_eq!(json, Some(serde_json::json!({ "currentVersion": "0.1.0", "availableVersion": "0.2.0" })));
    }
}
