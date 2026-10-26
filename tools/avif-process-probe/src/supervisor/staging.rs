// Core
use std::fs::{self, DirBuilder};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Staging {
    directory: PathBuf,
    pub output: PathBuf,
    destination: PathBuf,
}

impl Staging {
    pub fn new(destination: &Path) -> Result<Self, String> {
        let filename = destination.file_name().ok_or("output needs a file name")?;
        let parent = destination.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let parent = parent.canonicalize().map_err(|error| error.to_string())?;
        let destination = parent.join(filename);
        match fs::symlink_metadata(&destination) {
            Ok(_) => return Err("output already exists".into()),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_nanos();
        for _ in 0..128 {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let directory = parent.join(format!(".fileforge-avif-{}-{stamp}-{sequence}", std::process::id()));
            let builder = DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                // Core
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            match builder.create(&directory) {
                Ok(()) => return Ok(Self { output: directory.join("result.rgba"), directory, destination }),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("could not create an exclusive staging directory".into())
    }

    pub fn validate(&self, bytes: u64) -> Result<(), String> {
        let metadata = fs::symlink_metadata(&self.output).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file() || metadata.len() != bytes {
            return Err("decoder result is not a regular file of the expected size".into());
        }
        Ok(())
    }

    pub fn publish(&self) -> Result<(), String> {
        // Both names are on the same filesystem. An existing destination is never replaced.
        fs::hard_link(&self.output, &self.destination).map_err(|error| error.to_string())
    }

    pub fn cleanup(&self) -> Result<(), String> {
        match fs::remove_dir_all(&self.directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
