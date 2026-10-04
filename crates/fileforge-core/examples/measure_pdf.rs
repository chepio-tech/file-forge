//! Measures the PDF engine on real files without modifying them:
//! `cargo run --release -p fileforge-core --example measure_pdf -- a.pdf b.pdf`
//! Set `MEASURE_OUT=<dir>` to also write each result there for visual checks.

// Core
use std::time::Instant;

use fileforge_core::pdf::{ImageOptions, PdfOptions, compress};

fn main() {
    let presets = [
        ("lossless", PdfOptions::LOSSLESS),
        (
            "balanced",
            PdfOptions { images: Some(ImageOptions { jpeg_quality: 85, max_dpi: Some(200) }), ..PdfOptions::LOSSLESS },
        ),
        (
            "maximum",
            PdfOptions { images: Some(ImageOptions { jpeg_quality: 70, max_dpi: Some(150) }), ..PdfOptions::LOSSLESS },
        ),
        (
            "screen",
            PdfOptions { images: Some(ImageOptions { jpeg_quality: 65, max_dpi: Some(100) }), ..PdfOptions::LOSSLESS },
        ),
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
                        "{name:40.40} {preset:9} {:>8.1} KB → {:>8.1} KB ({:>5.1}%) {:>6} ms  pages {:>4}  img {}/{}  dup {}  unused {}{}",
                        report.original_size as f64 / 1000.0,
                        report.output_size as f64 / 1000.0,
                        100.0 * (1.0 - report.output_size as f64 / report.original_size as f64),
                        started.elapsed().as_millis(),
                        report.pages,
                        report.images_recompressed,
                        report.images_downsampled,
                        report.duplicates_merged,
                        report.unused_objects_removed,
                        if report.kept_original { "  (kept original)" } else { "" },
                    );
                }
                Err(error) => println!("{name:40.40} {preset:9} error: {error} ({} ms)", started.elapsed().as_millis()),
            }
        }
    }
}
