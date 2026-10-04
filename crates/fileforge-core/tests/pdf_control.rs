//! Progress reports and cancellation of a running compression (`Control`).

mod support;

// Core
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use fileforge_core::pdf::{
    Control, ImageOptions, PdfError, PdfOptions, Progress, Stage, compress, compress_controlled,
};
// Utils
use support::PdfBuilder;

const BALANCED: PdfOptions = PdfOptions {
    images: Some(ImageOptions { jpeg_quality: 85, max_dpi: Some(200), compress_flate_photos: false }),
    ..PdfOptions::LOSSLESS
};
/// Images decoded at once by the engine (`MAX_PARALLEL_IMAGES`).
const MAX_PARALLEL_IMAGES: u32 = 4;

/// Records every report and starts cancelling once `trigger` matches one.
struct Recorder {
    trigger: fn(&Progress) -> bool,
    cancelled: AtomicBool,
    log: Mutex<Vec<Progress>>,
}

impl Recorder {
    fn new(trigger: fn(&Progress) -> bool) -> Self {
        Self { trigger, cancelled: AtomicBool::new(false), log: Mutex::new(Vec::new()) }
    }

    fn observing() -> Self {
        Self::new(|_| false)
    }

    fn log(&self) -> Vec<Progress> {
        self.log.lock().expect("log").clone()
    }

    fn stages(&self) -> Vec<Stage> {
        let mut stages: Vec<Stage> = self.log().iter().map(|progress| progress.stage).collect();
        stages.dedup();
        stages
    }

    fn most_done(&self, stage: Stage) -> u32 {
        self.log().iter().filter(|progress| progress.stage == stage).map(|progress| progress.done).max().unwrap_or(0)
    }
}

impl Control for Recorder {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn report(&self, progress: Progress) {
        self.log.lock().expect("log").push(progress);
        if (self.trigger)(&progress) {
            self.cancelled.store(true, Ordering::SeqCst);
        }
    }
}

/// `pages` text pages, each showing its own photo large enough to be downsampled by Balanced.
fn photo_document(pages: usize) -> Vec<u8> {
    let mut pdf = PdfBuilder::default();
    for page in 0..pages {
        // Distinct widths keep the photos from being merged as duplicates.
        let width = 480 + 8 * u32::try_from(page).expect("few pages");
        let photo = pdf.jpeg_image(width, 360, 95, page % 2 == 1);
        // 72×54 pt = 1×0.75 in → at least 480 DPI; Balanced targets 200 DPI.
        pdf.page(&format!("BT /F1 12 Tf 72 720 Td (Page {page}) Tj ET q 72 0 0 54 72 500 cm /P Do Q"), &[("P", photo)]);
    }
    pdf.bytes()
}

#[test]
fn every_stage_is_reported_in_order_with_exact_counts() {
    let input = photo_document(3);
    let control = Recorder::observing();

    let output = compress_controlled(&input, &BALANCED, &control).expect("compresses");

    use Stage::*;
    assert_eq!(control.stages(), [Loading, Structure, Images, Streams, Saving, Verifying]);
    let log = control.log();
    let last = |stage: Stage| log.iter().rev().find(|progress| progress.stage == stage).copied().expect("reported");
    assert_eq!(last(Images), Progress { stage: Images, done: 3, total: 3 });
    let streams = last(Streams);
    assert!(streams.total > 0 && streams.done == streams.total, "{streams:?}");
    assert_eq!(output, compress(&input, &BALANCED).expect("compresses"), "observing must not change the result");
}

#[test]
fn counts_never_go_back_within_a_stage() {
    let control = Recorder::observing();

    compress_controlled(&photo_document(12), &BALANCED, &control).expect("compresses");

    let log = control.log();
    for pair in log.windows(2) {
        let [before, after] = pair else { unreachable!() };
        if before.stage == after.stage && before.total == after.total {
            assert!(after.done >= before.done, "{before:?} → {after:?}");
        }
        assert!(after.done <= after.total || after.total == 0, "{after:?}");
    }
}

#[test]
fn lossless_skips_the_image_stage() {
    let control = Recorder::observing();

    compress_controlled(&photo_document(2), &PdfOptions::LOSSLESS, &control).expect("compresses");

    assert!(!control.stages().contains(&Stage::Images));
}

#[test]
fn a_cancelled_run_stops_before_reading_the_file() {
    let control = Recorder::observing();
    control.cancelled.store(true, Ordering::SeqCst);

    // Not a PDF at all: a parse would fail with `Malformed`.
    let result = compress_controlled(b"%PDF-1.7 not really", &PdfOptions::LOSSLESS, &control);

    assert_eq!(result, Err(PdfError::Cancelled));
    assert!(control.log().is_empty());
}

#[test]
fn cancelling_during_the_image_pass_skips_the_remaining_images() {
    let control = Recorder::new(|progress| progress.stage == Stage::Images && progress.done >= 1);

    let result = compress_controlled(&photo_document(12), &BALANCED, &control);

    assert_eq!(result, Err(PdfError::Cancelled));
    // Only images already being encoded when the cancel arrived finish.
    assert!(control.most_done(Stage::Images) <= MAX_PARALLEL_IMAGES, "{:?}", control.log());
    assert!(!control.stages().contains(&Stage::Streams));
}

#[test]
fn cancelling_during_the_stream_pass_stops_at_the_next_stream() {
    let control = Recorder::new(|progress| progress.stage == Stage::Streams && progress.done >= 1);

    let result = compress_controlled(&photo_document(6), &PdfOptions::LOSSLESS, &control);

    assert_eq!(result, Err(PdfError::Cancelled));
    assert_eq!(control.most_done(Stage::Streams), 1);
    assert!(!control.stages().contains(&Stage::Saving));
}

#[test]
fn cancelling_while_saving_skips_verification() {
    let control = Recorder::new(|progress| progress.stage == Stage::Saving);

    let result = compress_controlled(&photo_document(1), &PdfOptions::LOSSLESS, &control);

    assert_eq!(result, Err(PdfError::Cancelled));
    assert_eq!(control.stages().last(), Some(&Stage::Saving));
}
