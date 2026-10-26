//! Bounded reads retain accounting when metadata becomes stale or the input grows.
// Core
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;
// Domain
use crate::buffers::{Budget, Bytes};

pub fn read_bounded(path: &Path, maximum: usize, budget: &Budget) -> Result<Bytes, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > maximum as u64 {
        return Err("input must be a regular file within the input byte limit".into());
    }
    read(file, metadata.len() as usize, maximum, budget)
}

fn read(mut reader: impl Read, hint: usize, maximum: usize, budget: &Budget) -> Result<Bytes, String> {
    if hint > maximum {
        return Err("input byte limit exceeded".into());
    }
    let mut bytes = budget.empty(hint)?;
    let mut chunk = [0u8; 8192];
    loop {
        // Read one sentinel byte at the limit without growing the owned vector.
        let count = (maximum - bytes.len()).saturating_add(1).min(chunk.len());
        let count = match reader.read(&mut chunk[..count]) {
            Ok(0) => return Ok(bytes),
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        };
        let length =
            bytes.len().checked_add(count).filter(|n| *n <= maximum).ok_or("input grew beyond the input byte limit")?;
        if length > bytes.capacity() {
            let capacity = bytes.capacity().saturating_mul(2).max(length).min(maximum);
            bytes.grow(capacity)?;
        }
        bytes.append(&chunk[..count])?;
    }
}

#[cfg(test)]
mod tests;
