//! Background-removal IPC: registry ids, one work slot, one content-keyed embedding, bounded previews.
// Core
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::PoisonError;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
// Domain
use fileforge_core::raster::background::{self, Options, Segmenter};
// Types
use crate::error::AppError;
use crate::file_registry::{FileId, FileRegistry, FileScope};
use crate::job_control::Cancellation;
use crate::models::{CachedEmbedding, DownloadProgress, MODEL_FILES, ModelStatus, Models};
use crate::results::ResultStore;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundReport {
    original_size: u64,
    output_size: u64,
    width: u32,
    height: u32,
    original_preview: Vec<u8>,
    preview: Vec<u8>,
}
fn background_error(error: fileforge_core::raster::RasterError) -> AppError {
    use fileforge_core::raster::RasterError;
    match error {
        RasterError::TooLarge { .. } | RasterError::TooManyPixels { .. } => AppError::BackgroundTooLarge,
        RasterError::Unsupported(detail) => AppError::BackgroundUnsupported(detail),
        error => error.into(),
    }
}

fn preview(pixels: &image::RgbaImage) -> Result<Vec<u8>, AppError> {
    let scale = (512.0 / pixels.width().max(pixels.height()) as f64).min(1.0);
    let small = image::imageops::resize(
        pixels,
        (pixels.width() as f64 * scale).round().max(1.0) as u32,
        (pixels.height() as f64 * scale).round().max(1.0) as u32,
        image::imageops::FilterType::Triangle,
    );
    Ok(background::png_bytes(&small, None)?)
}
#[tauri::command]
pub async fn background_preview(app: AppHandle, id: FileId) -> Result<Vec<u8>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let results = app.state::<ResultStore>();
        let _slot = results.work_slot();
        let file = app.state::<FileRegistry>().get(id)?;
        if file.scope != FileScope::Background {
            return Err(AppError::InvalidOptions("file belongs to another tool".into()));
        }
        let input = crate::commands::read_input(&file.path, background::MAX_INPUT_BYTES, |limit| {
            let _ = limit;
            AppError::BackgroundTooLarge
        })?;
        preview(&background::decode(&input).map_err(background_error)?.pixels)
    })
    .await?
}

#[tauri::command]
pub async fn background_model_status(app: AppHandle) -> Result<ModelStatus, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let results = app.state::<ResultStore>();
        let _slot = results.work_slot();
        Ok(app.state::<Models>().status())
    })
    .await?
}
#[tauri::command]
pub async fn download_background_model(
    app: AppHandle,
    on_progress: Channel<DownloadProgress>,
) -> Result<ModelStatus, AppError> {
    let ticket = app.state::<Cancellation>().ticket();
    tauri::async_runtime::spawn_blocking(move || {
        let results = app.state::<ResultStore>();
        let _slot = results.work_slot();
        let models = app.state::<Models>();
        let cancellation = app.state::<Cancellation>();
        models.download(|| cancellation.is_cancelled(ticket), |p| drop(on_progress.send(p)))?;
        Ok(models.status())
    })
    .await?
}
#[tauri::command]
pub async fn remove_background_model(app: AppHandle) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let results = app.state::<ResultStore>();
        let _slot = results.work_slot();
        app.state::<Models>().remove()
    })
    .await?
}
#[tauri::command]
pub async fn release_background_model(app: AppHandle) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let results = app.state::<ResultStore>();
        let _slot = results.work_slot();
        app.state::<Models>().unload();
        Ok(())
    })
    .await?
}
#[tauri::command]
pub async fn remove_background(app: AppHandle, id: FileId, options: Options) -> Result<BackgroundReport, AppError> {
    options.validate()?;
    let ticket = app.state::<Cancellation>().ticket();
    tauri::async_runtime::spawn_blocking(move || {
        let results = app.state::<ResultStore>();
        let _slot = results.work_slot();
        let cancellation = app.state::<Cancellation>();
        let control = crate::job_control::JobControl::new(&cancellation, ticket, |_| {});
        background::checkpoint(&control)?;
        let file = app.state::<FileRegistry>().get(id)?;
        if file.scope != FileScope::Background {
            return Err(AppError::InvalidOptions("file belongs to another tool".into()));
        }
        let input = crate::commands::read_input(&file.path, background::MAX_INPUT_BYTES, |limit| {
            let _ = limit;
            AppError::BackgroundTooLarge
        })?;
        let hash: [u8; 32] = Sha256::digest(&input).into();
        let image = background::decode(&input).map_err(background_error)?;
        background::checkpoint(&control)?;
        let models = app.state::<Models>();
        let mut engine = models.engine.lock().unwrap_or_else(PoisonError::into_inner);
        if engine.is_none() {
            if !models.status().installed {
                return Err(AppError::ModelMissing);
            }
            *engine = Some(fileforge_segment::Sam2::load(
                &models.dir.join(MODEL_FILES[0].name),
                &models.dir.join(MODEL_FILES[1].name),
            )?);
        }
        let engine = engine.as_ref().ok_or(AppError::ModelMissing)?;
        let mut cache = models.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.as_ref().is_none_or(|c| c.hash != hash) {
            *cache = None;
            let embedding = engine.embed(&image.pixels)?;
            background::checkpoint(&control)?;
            *cache = Some(CachedEmbedding { hash, embedding });
        }
        let embedding = &cache.as_ref().ok_or(AppError::ModelMissing)?.embedding;
        let mask = engine.mask(embedding, options.point.unwrap_or([0.5, 0.5]), options.point.is_none())?;
        background::checkpoint(&control)?;
        let pixels = background::render(&image, &mask, &options, &control)?;
        let bytes = background::encode(&pixels, image.icc.as_deref(), options.format, &control)?;
        let report = BackgroundReport {
            original_size: input.len() as u64,
            output_size: bytes.len() as u64,
            width: pixels.width(),
            height: pixels.height(),
            original_preview: preview(&image.pixels)?,
            preview: preview(&pixels)?,
        };
        background::checkpoint(&control)?;
        results.put_named(id, &bytes, options.format.extension(), "cutout")?;
        Ok(report)
    })
    .await?
}
