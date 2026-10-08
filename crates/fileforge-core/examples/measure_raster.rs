//! Measures the image engine on real JPEG and PNG files without modifying them:
//! `cargo run --release -p fileforge-core --example measure_raster -- a.jpg b.png`
//! Set `MEASURE_OUT=<dir>` to also write each result there for visual checks.

// Core
use std::time::Instant;

use fileforge_core::raster::{RasterOptions, compress};

fn main() {
    let presets = [
        ("lossless", RasterOptions::LOSSLESS),
        ("balanced", RasterOptions { jpeg_quality: Some(85), png_level: 4, ..RasterOptions::LOSSLESS }),
        (
            "maximum",
            RasterOptions { jpeg_quality: Some(75), png_level: 6, png_zopfli: true, ..RasterOptions::LOSSLESS },
        ),
        ("no-meta", RasterOptions { strip_metadata: true, ..RasterOptions::LOSSLESS }),
    ];
    for path in std::env::args().skip(1) {
        let Ok(input) = std::fs::read(&path) else {
            eprintln!("{path}: unreadable");
            continue;
        };
        let name = path.rsplit('/').next().unwrap_or(&path);
        for (preset, options) in &presets {
            let started = Instant::now();
            match compress(&input, options) {
                Ok(output) => {
                    if let Ok(dir) = std::env::var("MEASURE_OUT") {
                        let _ = std::fs::write(format!("{dir}/{preset}-{name}"), &output.bytes);
                    }
                    let report = output.report;
                    println!(
                        "{name:36.36} {preset:9} {:?} {:>9.1} KB → {:>9.1} KB ({:>5.1}%) {:>6} ms  {}×{}{}{}{}",
                        report.format,
                        report.original_size as f64 / 1000.0,
                        report.output_size as f64 / 1000.0,
                        100.0 * (1.0 - report.output_size as f64 / report.original_size as f64),
                        started.elapsed().as_millis(),
                        report.width,
                        report.height,
                        if report.reencoded { "  re-encoded" } else { "" },
                        if report.metadata_removed { "  metadata removed" } else { "" },
                        report.kept.map(|kept| format!("  (kept: {kept:?})")).unwrap_or_default(),
                    );
                }
                Err(error) => println!("{name:36.36} {preset:9} error: {error} ({} ms)", started.elapsed().as_millis()),
            }
        }
    }
}
