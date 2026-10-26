// Core
use std::io::Read;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub(super) const MAX_STDOUT: usize = 64 * 1024;
pub(super) const MAX_STDERR: usize = 8 * 1024;
const MAX_LINE: usize = 4096;

pub(super) enum Event {
    Started(Instant),
    Fault,
}

#[derive(Default)]
pub(super) struct Protocol {
    pub started: Option<Instant>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub bytes: Option<u64>,
    pub status: Option<String>,
}

fn number(slot: &mut Option<u64>, value: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err("duplicate geometry field".into());
    }
    *slot = Some(value.parse().map_err(|_| "invalid geometry field")?);
    Ok(())
}

impl Protocol {
    fn line(&mut self, bytes: &[u8], events: &SyncSender<Event>) -> Result<(), String> {
        let line = std::str::from_utf8(bytes).map_err(|_| "non-UTF-8 decoder stdout")?.trim_end_matches('\r');
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "stage" => {
                    if value != "decode" || self.started.is_some() {
                        return Err("invalid or duplicate decoder stage".into());
                    }
                    let now = Instant::now();
                    self.started = Some(now);
                    let _ = events.try_send(Event::Started(now));
                }
                "width" => number(&mut self.width, value)?,
                "height" => number(&mut self.height, value)?,
                "output_bytes" => number(&mut self.bytes, value)?,
                "status" => {
                    if self.status.is_some() || !matches!(value, "ok" | "error") {
                        return Err("invalid or duplicate decoder status".into());
                    }
                    self.status = Some(value.to_owned());
                }
                _ => {} // Other bounded probe diagnostics are not authority to publish output.
            }
        }
        Ok(())
    }

    pub fn geometry(&self, max_pixels: u64) -> Result<(u64, u64, u64), String> {
        if self.started.is_none() || self.status.as_deref() != Some("ok") {
            return Err("decoder did not report a complete successful operation".into());
        }
        let width = self.width.ok_or("missing width")?;
        let height = self.height.ok_or("missing height")?;
        let pixels = width.checked_mul(height).filter(|count| *count > 0 && *count <= max_pixels);
        let bytes = pixels.and_then(|count| count.checked_mul(4)).ok_or("invalid output geometry")?;
        if self.bytes != Some(bytes) {
            return Err("decoder byte count differs from geometry".into());
        }
        Ok((width, height, bytes))
    }
}

pub(super) fn stdout(mut reader: impl Read, events: SyncSender<Event>) -> Result<Protocol, String> {
    let result = (|| {
        let mut protocol = Protocol::default();
        let mut line = Vec::with_capacity(MAX_LINE);
        let mut buffer = [0; 1024];
        let mut total = 0;
        loop {
            let count = reader.read(&mut buffer).map_err(|error| error.to_string())?;
            if count == 0 {
                if !line.is_empty() {
                    protocol.line(&line, &events)?;
                }
                return Ok(protocol);
            }
            total += count;
            if total > MAX_STDOUT {
                return Err("decoder stdout exceeds limit".into());
            }
            for &byte in &buffer[..count] {
                if byte == b'\n' {
                    protocol.line(&line, &events)?;
                    line.clear();
                } else if line.len() == MAX_LINE {
                    return Err("decoder stdout line exceeds limit".into());
                } else {
                    line.push(byte);
                }
            }
        }
    })();
    if result.is_err() {
        let _ = events.try_send(Event::Fault);
    }
    result
}

pub(super) fn stderr(reader: impl Read, events: SyncSender<Event>) -> Result<String, String> {
    let mut bytes = Vec::new();
    let result = reader.take((MAX_STDERR + 1) as u64).read_to_end(&mut bytes);
    if result.is_err() || bytes.len() > MAX_STDERR {
        let _ = events.try_send(Event::Fault);
        return Err("decoder stderr read failed or exceeded limit".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
