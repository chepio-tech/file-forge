//! Standalone evaluation of the bounded AVIF API. This is not an application decoder.
// Core
use avif_rust::{NativeDecodeLimits, decode_frame_bytes_strict_with_limits};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
// Domain
use transforms::orient;

mod transforms;

const MAX_INPUT: u64 = 256 * 1024 * 1024;
const MAX_PIXELS: usize = 32_000_000;
const MAX_ALLOCATION: usize = 512 * 1024 * 1024;

#[derive(Debug)]
struct Options {
    input: PathBuf,
    output: PathBuf,
    max_pixels: usize,
    allocation_budget: usize,
    cancel_after: Option<Duration>,
}

fn positive(value: Option<String>, flag: &str) -> Result<usize, String> {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{flag} requires a positive integer"))
}

fn options(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut args = args.into_iter();
    let usage =
        "usage: fileforge-avif-probe INPUT OUTPUT.rgba [--max-pixels N] [--allocation-budget N] [--cancel-after-ms N]";
    let mut result = Options {
        input: args.next().ok_or(usage)?.into(),
        output: args.next().ok_or(usage)?.into(),
        max_pixels: MAX_PIXELS,
        allocation_budget: MAX_ALLOCATION,
        cancel_after: None,
    };
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--max-pixels" => result.max_pixels = positive(args.next(), &flag)?.min(MAX_PIXELS),
            "--allocation-budget" => {
                result.allocation_budget = positive(args.next(), &flag)?.min(MAX_ALLOCATION);
            }
            "--cancel-after-ms" => {
                let ms = args.next().and_then(|v| v.parse::<u64>().ok()).ok_or("invalid cancel delay")?;
                result.cancel_after = Some(Duration::from_millis(ms));
            }
            _ => return Err(format!("unknown option: {flag}")),
        }
    }
    Ok(result)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT {
        return Err("input must be a regular file of at most 256 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_INPUT + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err("input grew beyond 256 MiB".into());
    }
    Ok(bytes)
}

fn limits(opts: &Options) -> Result<NativeDecodeLimits, String> {
    NativeDecodeLimits::new(
        MAX_INPUT as usize,
        MAX_PIXELS,
        MAX_PIXELS,
        opts.max_pixels,
        MAX_PIXELS * 8,
        16 * 1024 * 1024,
        4 * 1024 * 1024,
        128,
        256,
        64,
        8,
        1,
    )
    .with_max_live_allocation_bytes(opts.allocation_budget)
    .map_err(|error| error.to_string())
}

fn checkpoint(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) { Err("cancelled".into()) } else { Ok(()) }
}

fn decode(opts: &Options, cancelled: &AtomicBool) -> Result<(), String> {
    checkpoint(cancelled)?;
    let bytes = read_bounded(&opts.input)?;
    checkpoint(cancelled)?;
    let start = Instant::now();
    println!("stage=decode");
    std::io::stdout().flush().map_err(|error| error.to_string())?;
    let native = decode_frame_bytes_strict_with_limits(&bytes, &limits(opts)?).map_err(|error| error.to_string())?;
    println!("decode_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
    checkpoint(cancelled)?;
    let (mut frame, information) = native.into_parts();
    let info = information.info();
    if info.width.map(|w| w as usize) != Some(frame.width) || info.height.map(|h| h as usize) != Some(frame.height) {
        return Err("container dimensions differ from decoded frame".into());
    }
    println!("bit_depth={}", frame.bit_depth);
    println!("color={:?}", frame.color_config);
    println!("container_color={:?}", frame.color_information);
    println!("native_premultiplied={}", frame.alpha_premultiplied);
    println!("container_premultiplied={}", info.alpha_premultiplied);
    println!("items={}", information.item_count());
    println!("properties={}", information.property_count());
    println!("metadata_bytes={}", information.metadata_bytes());
    if frame.bit_depth != 8 {
        return Err("prototype refuses precision reduction of non-8-bit input".into());
    }
    let color_start = Instant::now();
    // Carry the reported flag; 0.1.2 still misses the prem reference emitted by the reference encoder.
    frame.alpha_premultiplied = info.alpha_premultiplied;
    let image = frame.to_rgba8().map_err(|error| error.to_string())?;
    let image = orient(image, information.ordered_primary_properties())?;
    println!("color_ms={:.3}", color_start.elapsed().as_secs_f64() * 1000.0);
    checkpoint(cancelled)?;
    let count = image
        .width
        .checked_mul(image.height)
        .filter(|pixels| *pixels <= opts.max_pixels)
        .ok_or("output pixel limit exceeded")?;
    if count.checked_mul(4) != Some(image.rgba.len()) {
        return Err("output sample count differs from geometry".into());
    }
    // create_new protects the input, aliases and any pre-existing output. No file exists on decode failure.
    let mut output =
        OpenOptions::new().write(true).create_new(true).open(&opts.output).map_err(|error| error.to_string())?;
    output.write_all(&image.rgba).map_err(|error| error.to_string())?;
    println!("width={}", image.width);
    println!("height={}", image.height);
    println!("output_bytes={}", image.rgba.len());
    Ok(())
}

fn main() {
    let start = Instant::now();
    let result = options(std::env::args().skip(1)).and_then(|opts| {
        let cancelled = Arc::new(AtomicBool::new(opts.cancel_after == Some(Duration::ZERO)));
        if let Some(delay) = opts.cancel_after.filter(|delay| !delay.is_zero()) {
            let flag = cancelled.clone();
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                flag.store(true, Ordering::Release);
            });
        }
        // Panics are recorded as failures, never as supported input or successful refusal.
        std::panic::catch_unwind(|| decode(&opts, &cancelled)).unwrap_or_else(|_| Err("decoder panic".into()))
    });
    println!("total_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
    match result {
        Ok(()) => println!("status=ok"),
        Err(error) => {
            println!("status=error");
            eprintln!("{error}");
            std::process::exit(if error == "decoder panic" { 3 } else { 2 });
        }
    }
}

#[cfg(test)]
mod tests {
    // Core
    use std::sync::atomic::AtomicBool;
    // Domain
    use super::*;

    #[test]
    fn command_line_cannot_relax_resource_ceilings() {
        let opts =
            options(["in", "out", "--max-pixels", "999999999", "--allocation-budget", "9999999999"].map(str::to_owned))
                .expect("options");
        assert_eq!(opts.max_pixels, MAX_PIXELS);
        assert_eq!(opts.allocation_budget, MAX_ALLOCATION);
    }

    #[test]
    fn invalid_or_missing_bounds_fail_instead_of_becoming_unlimited() {
        for args in [
            vec!["in", "out", "--max-pixels", "0"],
            vec!["in", "out", "--allocation-budget"],
            vec!["in", "out", "--unknown"],
            vec!["in"],
        ] {
            assert!(options(args.into_iter().map(str::to_owned)).is_err());
        }
    }

    #[test]
    fn pre_cancelled_work_does_not_open_input_or_output() {
        let opts = options(["nonexistent-input", "nonexistent-output"].map(str::to_owned)).expect("options");
        assert_eq!(decode(&opts, &AtomicBool::new(true)), Err("cancelled".into()));
    }
}
