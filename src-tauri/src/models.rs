//! Model delivery is explicit, bounded, hash pinned and atomic. No user image enters a request.
// Core
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;
// Types
use crate::error::AppError;

pub const MODEL_TAG: &str = "models/sam2.1-hiera-tiny-v1";
pub struct ModelFile {
    pub name: &'static str,
    pub size: u64,
    pub hash: &'static str,
}
pub const MODEL_FILES: [ModelFile; 2] = [
    ModelFile {
        name: "sam2.1-hiera-tiny-encoder.onnx",
        size: 67624839,
        hash: "6c6ef6bcf30cbc4e481d55d97756222847d7aa265a5159a0c551f8290c046de3",
    },
    ModelFile {
        name: "sam2.1-hiera-tiny-decoder.onnx",
        size: 14912939,
        hash: "e53f68b0ff065fa617ff2717dffc49cefe40143ccf465b19070184f7adb5aff1",
    },
];
pub const MODEL_BYTES: u64 = MODEL_FILES[0].size + MODEL_FILES[1].size;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub installed: bool,
    pub download_bytes: u64,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

pub struct Models {
    pub dir: PathBuf,
    pub engine: Mutex<Option<fileforge_segment::Sam2>>,
    pub cache: Mutex<Option<CachedEmbedding>>,
}
pub struct CachedEmbedding {
    pub hash: [u8; 32],
    pub embedding: fileforge_segment::Embedding,
}
impl Models {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, engine: Mutex::new(None), cache: Mutex::new(None) }
    }
    pub fn status(&self) -> ModelStatus {
        ModelStatus {
            installed: MODEL_FILES
                .iter()
                .all(|pin| fileforge_segment::verify(&self.dir.join(pin.name), pin.size, pin.hash).is_ok()),
            download_bytes: MODEL_BYTES,
        }
    }
    pub fn unload(&self) {
        *self.cache.lock().unwrap_or_else(PoisonError::into_inner) = None;
        *self.engine.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
    pub fn remove(&self) -> Result<(), AppError> {
        self.unload();
        for pin in MODEL_FILES {
            match fs::remove_file(self.dir.join(pin.name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
        Ok(())
    }
    pub fn download(&self, cancel: impl Fn() -> bool, progress: impl Fn(DownloadProgress)) -> Result<(), AppError> {
        install_tls_provider();
        fs::create_dir_all(&self.dir)?;
        let client = reqwest::Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(600))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let allowed = matches!(
                    attempt.url().host_str(),
                    Some("github.com" | "objects.githubusercontent.com" | "release-assets.githubusercontent.com")
                );
                if allowed && attempt.url().scheme() == "https" && attempt.previous().len() < 4 {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(download_error)?;
        let mut downloaded = 0;
        for pin in MODEL_FILES {
            if cancel() {
                return Err(AppError::Cancelled);
            }
            let target = self.dir.join(pin.name);
            if fileforge_segment::verify(&target, pin.size, pin.hash).is_ok() {
                downloaded += pin.size;
                progress(DownloadProgress { downloaded, total: MODEL_BYTES });
                continue;
            }
            let url = format!("https://github.com/chepio-tech/file-forge/releases/download/{MODEL_TAG}/{}", pin.name);
            let response = tauri::async_runtime::block_on(client.get(url).send())
                .and_then(|r| r.error_for_status())
                .map_err(download_error)?;
            if response.content_length().is_some_and(|n| n != pin.size) {
                return Err(download_error("wrong Content-Length"));
            }
            let base = downloaded;
            write_verified(ResponseReader { response, pending: Vec::new(), offset: 0 }, &target, &pin, &cancel, |n| {
                progress(DownloadProgress { downloaded: base + n, total: MODEL_BYTES })
            })?;
            downloaded += pin.size;
        }
        Ok(())
    }
}
struct ResponseReader {
    response: reqwest::Response,
    pending: Vec<u8>,
    offset: usize,
}
impl Read for ResponseReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.offset == self.pending.len() {
            let chunk = tauri::async_runtime::block_on(self.response.chunk()).map_err(std::io::Error::other)?;
            let Some(chunk) = chunk else {
                return Ok(0);
            };
            if chunk.len() > 1024 * 1024 {
                return Err(std::io::Error::other("oversized network chunk"));
            }
            self.pending = chunk.to_vec();
            self.offset = 0;
        }
        let n = buf.len().min(self.pending.len() - self.offset);
        buf[..n].copy_from_slice(&self.pending[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
}

fn install_tls_provider() {
    // Model downloads can start before the updater initializes its provider.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn download_error(e: impl std::fmt::Display) -> AppError {
    AppError::ModelDownload(e.to_string())
}
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn write_verified(
    mut reader: impl Read,
    target: &Path,
    pin: &ModelFile,
    cancel: impl Fn() -> bool,
    progress: impl Fn(u64),
) -> Result<(), AppError> {
    let staging = Partial(target.with_extension("partial"));
    // All callers hold ResultStore's work slot, so stale partials cannot belong to another writer.
    match fs::remove_file(&staging.0) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(&staging.0)?;
    let mut sha = Sha256::new();
    let mut total = 0u64;
    let mut buf = [0; 65536];
    loop {
        if cancel() {
            return Err(AppError::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(download_error)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > pin.size {
            return Err(download_error("oversized model"));
        }
        sha.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        progress(total);
    }
    if total != pin.size || format!("{:x}", sha.finalize()) != pin.hash {
        return Err(download_error("model integrity check"));
    }
    if cancel() {
        return Err(AppError::Cancelled);
    }
    file.sync_all()?;
    drop(file);
    // Windows cannot replace a destination with rename. No valid pinned model is ever removed here.
    if target.exists() {
        fs::remove_file(target)?;
    }
    fs::rename(&staging.0, target)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_is_atomic_bounded_and_cancellable() {
        let dir = tempfile::tempdir().expect("dir");
        let target = dir.path().join("model.onnx");
        let hash = format!("{:x}", Sha256::digest(b"abc"));
        let pin = ModelFile { name: "test", size: 3, hash: Box::leak(hash.into_boxed_str()) };
        write_verified(b"abc".as_slice(), &target, &pin, || false, |_| {}).expect("verified");
        assert_eq!(fs::read(&target).expect("file"), b"abc");
        for input in [b"ab".as_slice(), b"abd", b"abcd"] {
            assert!(write_verified(input, &target, &pin, || false, |_| {}).is_err());
            assert_eq!(fs::read(&target).expect("old"), b"abc");
            assert!(!target.with_extension("partial").exists());
        }
        assert!(matches!(write_verified(b"abc".as_slice(), &target, &pin, || true, |_| {}), Err(AppError::Cancelled)));
        assert!(!target.with_extension("partial").exists());
    }
    #[test]
    fn local_http_download_checks_partial_and_corrupted_bodies() {
        install_tls_provider();
        use std::net::TcpListener;
        for (body, valid) in [(b"abc".as_slice(), true), (b"ab", false), (b"abd", false)] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
            let address = listener.local_addr().expect("address");
            let body = body.to_vec();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut request = [0; 4096];
                let n = stream.read(&mut request).expect("request");
                assert!(n > 0);
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\n").expect("headers");
                stream.write_all(&body).expect("body");
            });
            let response = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .expect("client")
                .get(format!("http://{address}/model"))
                .send()
                .expect("response");
            let dir = tempfile::tempdir().expect("dir");
            let target = dir.path().join("model");
            let pin = ModelFile {
                name: "test",
                size: 3,
                hash: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            };
            let outcome = write_verified(response, &target, &pin, || false, |_| {});
            assert_eq!(outcome.is_ok(), valid, "correct body succeeds; partial and corrupt bodies fail");
            if outcome.is_ok() {
                assert_eq!(fs::read(&target).expect("body"), b"abc");
            } else {
                assert!(!target.exists());
            }
            assert!(!target.with_extension("partial").exists());
            server.join().expect("server");
        }
    }

    #[test]
    fn network_failure_removes_partial_and_keeps_existing_model() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "offline"))
            }
        }
        let dir = tempfile::tempdir().expect("dir");
        let target = dir.path().join("model");
        fs::write(&target, b"previous").expect("old");
        let pin = ModelFile { name: "test", size: 3, hash: "wrong" };
        assert!(write_verified(Broken, &target, &pin, || false, |_| {}).is_err());
        assert_eq!(fs::read(target).expect("old"), b"previous");
    }
}
