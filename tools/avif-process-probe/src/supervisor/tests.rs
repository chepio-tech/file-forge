// Core
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
// Services
use super::{Cancellation, MAX_ALLOCATION, MAX_DEADLINE, MAX_PIXELS, Options, Outcome, Report, run, run_with};

static NEXT: AtomicU64 = AtomicU64::new(0);
const PIXELS: [u8; 8] = [12, 20, 60, 255, 80, 40, 120, 100];

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("avif-process-test-{}-{stamp}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("exclusive test directory");
        fs::write(path.join("original.avif"), b"owned input").expect("input");
        Self(path)
    }

    fn options(&self, name: &str) -> Options {
        Options {
            decoder: PathBuf::from("unused-test-decoder"),
            input: self.0.join("original.avif"),
            output: self.0.join(name),
            deadline: Duration::from_secs(3),
            cancel_after_decode: None,
            max_pixels: MAX_PIXELS,
            allocation_budget: MAX_ALLOCATION,
        }
    }

    fn command(&self, mode: &str, staged: &Path, destination: &Path) -> Command {
        let mut command = Command::new(env::current_exe().expect("test executable"));
        command
            .args(["--ignored", "--exact", "supervisor::tests::worker_fixture", "--nocapture"])
            .env("AVIF_PROCESS_TEST_MODE", mode)
            .env("AVIF_PROCESS_TEST_OUTPUT", staged)
            .env("AVIF_PROCESS_TEST_DESTINATION", destination)
            .env("AVIF_PROCESS_TEST_ROOT", &self.0);
        command
    }

    fn run(&self, mode: &str, options: &Options, cancellation: &Cancellation) -> Report {
        run_with(options, cancellation, |staged| self.command(mode, staged, &options.output))
    }

    fn assert_clean(&self) {
        assert_eq!(fs::read(self.0.join("original.avif")).expect("original"), b"owned input");
        for entry in fs::read_dir(&self.0).expect("directory") {
            assert!(!entry.expect("entry").file_name().to_string_lossy().starts_with(".fileforge-avif-"));
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stopped(report: &Report, expected: Outcome) {
    assert_eq!(report.outcome, expected, "{report:?}");
    assert!(report.child_id.is_some() && report.child_reaped, "{report:?}");
    assert!(report.cleanup_ok && !report.published, "{report:?}");
}

#[test]
fn publishes_exact_bytes_only_after_success() {
    let fixture = Fixture::new();
    let options = fixture.options("result.rgba");
    let report = fixture.run("success", &options, &Cancellation::default());
    assert_eq!(report.outcome, Outcome::Completed, "{report:?}");
    assert!(report.published && report.child_reaped && report.cleanup_ok);
    assert_eq!(report.geometry, Some((2, 1, 8)));
    assert_eq!(fs::read(&options.output).expect("output"), PIXELS);
    fixture.assert_clean();
}

#[test]
fn pre_cancel_never_launches_or_creates_staging() {
    let fixture = Fixture::new();
    let options = fixture.options("result.rgba");
    let cancellation = Cancellation::default();
    cancellation.request();
    let report = fixture.run("success", &options, &cancellation);
    assert_eq!(report.outcome, Outcome::Cancelled);
    assert!(report.child_id.is_none() && !report.published);
    assert!(!options.output.exists());
    fixture.assert_clean();
}

#[test]
fn kills_reaps_and_removes_partial_output_then_recovers() {
    let fixture = Fixture::new();
    let mut options = fixture.options("cancelled.rgba");
    options.cancel_after_decode = Some(Duration::from_millis(5));
    let report = fixture.run("partialHang", &options, &Cancellation::default());
    stopped(&report, Outcome::Cancelled);
    assert!(report.cancel_reap_ms.expect("latency") < 250.0, "{report:?}");
    assert!(report.cancel_settle_ms.expect("settlement") < 250.0, "{report:?}");
    assert!(!options.output.exists());
    fixture.assert_clean();
    options.cancel_after_decode = None;
    let next = fixture.run("success", &options, &Cancellation::default());
    assert_eq!(next.outcome, Outcome::Completed, "{next:?}");
    assert_eq!(fs::read(&options.output).expect("next output"), PIXELS);
    fixture.assert_clean();
}

#[test]
fn external_cancellation_reaps_owned_child() {
    let fixture = Fixture::new();
    let options = fixture.options("result.rgba");
    let cancellation = Cancellation::default();
    thread::scope(|scope| {
        let worker = scope.spawn(|| fixture.run("partialHang", &options, &cancellation));
        let started = Instant::now();
        while !fixture.0.join("ready").exists() && started.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(2));
        }
        cancellation.request();
        let report = worker.join().expect("supervisor");
        assert!(fixture.0.join("ready").exists(), "worker did not start: {report:?}");
        stopped(&report, Outcome::Cancelled);
        assert!(report.cancel_reap_ms.expect("latency") < 250.0, "{report:?}");
    });
    assert!(!options.output.exists());
    fixture.assert_clean();
}

#[test]
fn whole_operation_deadline_covers_silent_startup_and_decode() {
    for mode in ["silentHang", "partialHang"] {
        let fixture = Fixture::new();
        let mut options = fixture.options("result.rgba");
        options.deadline = Duration::from_millis(150);
        let report = fixture.run(mode, &options, &Cancellation::default());
        stopped(&report, Outcome::Deadline);
        assert!(report.elapsed_ms < 2000.0, "{report:?}");
        assert!(!options.output.exists());
        fixture.assert_clean();
    }
}

#[test]
fn zero_delay_cancels_even_when_worker_exits_immediately() {
    let fixture = Fixture::new();
    for _ in 0..12 {
        let mut options = fixture.options("result.rgba");
        options.cancel_after_decode = Some(Duration::ZERO);
        let report = fixture.run("success", &options, &Cancellation::default());
        stopped(&report, Outcome::Cancelled);
        assert!(!options.output.exists());
        fixture.assert_clean();
    }
}

#[test]
fn failures_crashes_and_invalid_results_never_publish() {
    for mode in [
        "exitFailure",
        "panic",
        "statusError",
        "missing",
        "short",
        "oversized",
        "directory",
        "noStage",
        "duplicate",
        "wrongBytes",
        "overflow",
        "zero",
        "utf8",
    ] {
        let fixture = Fixture::new();
        let options = fixture.options("result.rgba");
        let report = fixture.run(mode, &options, &Cancellation::default());
        stopped(&report, Outcome::Failed);
        assert!(!report.error.is_empty(), "{mode}: {report:?}");
        assert!(!options.output.exists(), "{mode}");
        fixture.assert_clean();
    }
}

#[test]
fn bounded_protocol_readers_interrupt_flooding_workers() {
    for mode in ["longLine", "stdoutFlood", "stderrFlood"] {
        let fixture = Fixture::new();
        let options = fixture.options("result.rgba");
        let report = fixture.run(mode, &options, &Cancellation::default());
        stopped(&report, Outcome::Failed);
        assert!(report.elapsed_ms < 2000.0, "{report:?}");
        assert!(!options.output.exists());
        fixture.assert_clean();
    }
}

#[test]
fn existing_destination_original_and_hard_link_alias_are_preserved() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("existing"), b"keep").expect("existing output");
    fs::hard_link(fixture.0.join("original.avif"), fixture.0.join("alias")).expect("hard link");
    for name in ["existing", "original.avif", "alias"] {
        let options = fixture.options(name);
        let before = fs::read(&options.output).expect("before");
        let report = fixture.run("success", &options, &Cancellation::default());
        assert_eq!(report.outcome, Outcome::Failed);
        assert!(report.child_id.is_none() && !report.published);
        assert_eq!(fs::read(&options.output).expect("after"), before);
    }
    fixture.assert_clean();
}

#[test]
fn racing_destination_is_not_overwritten_or_removed() {
    let fixture = Fixture::new();
    let options = fixture.options("result.rgba");
    let report = fixture.run("race", &options, &Cancellation::default());
    stopped(&report, Outcome::Failed);
    assert_eq!(fs::read(&options.output).expect("racing output"), b"concurrent owner");
    fixture.assert_clean();
}

#[test]
fn concurrent_operations_own_separate_staging_and_cancellation() {
    let fixture = Fixture::new();
    let mut cancelled = fixture.options("cancelled.rgba");
    cancelled.cancel_after_decode = Some(Duration::from_millis(10));
    let completed = fixture.options("completed.rgba");
    thread::scope(|scope| {
        let first = scope.spawn(|| fixture.run("partialHang", &cancelled, &Cancellation::default()));
        let second = scope.spawn(|| fixture.run("slowSuccess", &completed, &Cancellation::default()));
        stopped(&first.join().expect("cancelled"), Outcome::Cancelled);
        let report = second.join().expect("completed");
        assert_eq!(report.outcome, Outcome::Completed, "{report:?}");
    });
    assert!(!cancelled.output.exists());
    assert_eq!(fs::read(completed.output).expect("independent result"), PIXELS);
    fixture.assert_clean();
}

#[test]
fn competing_publications_have_one_winner() {
    let fixture = Fixture::new();
    let options = fixture.options("result.rgba");
    thread::scope(|scope| {
        let first = scope.spawn(|| fixture.run("slowSuccess", &options, &Cancellation::default()));
        let second = scope.spawn(|| fixture.run("slowSuccess", &options, &Cancellation::default()));
        let reports = [first.join().expect("first"), second.join().expect("second")];
        assert_eq!(reports.iter().filter(|report| report.published).count(), 1, "{reports:?}");
        assert_eq!(reports.iter().filter(|report| report.outcome == Outcome::Completed).count(), 1, "{reports:?}");
        assert!(reports.iter().all(|report| report.cleanup_ok && (report.child_id.is_none() || report.child_reaped)));
    });
    assert_eq!(fs::read(options.output).expect("winner"), PIXELS);
    fixture.assert_clean();
}

#[cfg(unix)]
#[test]
fn cleanup_failure_is_visible_without_removing_published_output() {
    // Core
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let options = fixture.options("result.rgba");
    let report = fixture.run("readonlyStage", &options, &Cancellation::default());
    // Restore the fixture's permission before assertions so test teardown can remove its own files.
    for entry in fs::read_dir(&fixture.0).expect("directory") {
        let entry = entry.expect("entry");
        if entry.file_name().to_string_lossy().starts_with(".fileforge-avif-") {
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o700)).expect("restore staging");
            fs::remove_dir_all(entry.path()).expect("remove fixture staging");
        }
    }
    assert_eq!(report.outcome, Outcome::Failed, "{report:?}");
    assert!(!report.cleanup_ok && report.published && report.child_reaped, "{report:?}");
    assert!(report.error.contains("staging cleanup failed"));
    assert_eq!(fs::read(&options.output).expect("published output"), PIXELS);
    fixture.assert_clean();
}

#[test]
fn failed_spawn_and_invalid_bounds_leave_no_staging() {
    let fixture = Fixture::new();
    let mut options = fixture.options("result.rgba");
    options.decoder = fixture.0.join("missing-decoder");
    let report = run(&options, &Cancellation::default());
    assert_eq!(report.outcome, Outcome::Failed);
    assert!(report.child_id.is_none());
    fixture.assert_clean();
    for (pixels, allocation, deadline) in [
        (0, MAX_ALLOCATION, MAX_DEADLINE),
        (MAX_PIXELS + 1, MAX_ALLOCATION, MAX_DEADLINE),
        (MAX_PIXELS, 0, MAX_DEADLINE),
        (MAX_PIXELS, MAX_ALLOCATION + 1, MAX_DEADLINE),
        (MAX_PIXELS, MAX_ALLOCATION, Duration::ZERO),
        (MAX_PIXELS, MAX_ALLOCATION, MAX_DEADLINE + Duration::from_millis(1)),
    ] {
        options.max_pixels = pixels;
        options.allocation_budget = allocation;
        options.deadline = deadline;
        let report = fixture.run("success", &options, &Cancellation::default());
        assert_eq!(report.outcome, Outcome::Failed);
        assert!(report.child_id.is_none());
        fixture.assert_clean();
    }
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_never_published_or_overwritten() {
    // Core
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    symlink("missing", fixture.0.join("broken")).expect("broken link");
    symlink("original.avif", fixture.0.join("alias")).expect("input alias");
    for name in ["broken", "alias"] {
        let report = fixture.run("success", &fixture.options(name), &Cancellation::default());
        assert!(report.child_id.is_none() && !report.published);
        assert!(fs::symlink_metadata(fixture.0.join(name)).expect("link").file_type().is_symlink());
    }
    let options = fixture.options("result.rgba");
    let report = fixture.run("symlink", &options, &Cancellation::default());
    stopped(&report, Outcome::Failed);
    assert!(!options.output.exists());
    fixture.assert_clean();
}

// This ignored entry point runs only as the child selected by an exact test-name filter.
#[test]
#[ignore]
fn worker_fixture() {
    let mode = env::var("AVIF_PROCESS_TEST_MODE").expect("fixture mode");
    let output = PathBuf::from(env::var_os("AVIF_PROCESS_TEST_OUTPUT").expect("fixture output"));
    let root = PathBuf::from(env::var_os("AVIF_PROCESS_TEST_ROOT").expect("fixture root"));
    if mode == "silentHang" {
        thread::sleep(Duration::from_secs(30));
        return;
    }
    if mode == "partialHang" {
        fs::write(&output, [1, 2, 3]).expect("partial output");
        fs::write(root.join("ready"), b"ready").expect("ready");
    }
    if mode != "noStage" {
        println!("\nstage=decode");
    }
    io::stdout().flush().expect("stage flush");
    match mode.as_str() {
        "partialHang" => {
            thread::sleep(Duration::from_secs(30));
            return;
        }
        "exitFailure" => std::process::exit(7),
        "panic" => panic!("fixture decoder crash"),
        "slowSuccess" => thread::sleep(Duration::from_millis(80)),
        "duplicate" => println!("stage=decode"),
        "utf8" => {
            io::stdout().write_all(&[0xff, b'\n']).expect("invalid utf8");
        }
        "longLine" => println!("{}", "x".repeat(4097)),
        "stdoutFlood" => {
            for _ in 0..800 {
                println!("{}", "x".repeat(100));
            }
        }
        "stderrFlood" => eprintln!("{}", "x".repeat(8193)),
        "race" => fs::write(env::var_os("AVIF_PROCESS_TEST_DESTINATION").expect("destination"), b"concurrent owner")
            .expect("racing destination"),
        _ => {}
    }
    if matches!(mode.as_str(), "longLine" | "stdoutFlood" | "stderrFlood") {
        io::stdout().flush().expect("flood flush");
        thread::sleep(Duration::from_secs(30));
        return;
    }
    match mode.as_str() {
        "missing" => {}
        "directory" => fs::create_dir(&output).expect("directory output"),
        "short" => fs::write(&output, [1]).expect("short output"),
        "oversized" => {
            fs::File::create(&output).expect("large output").set_len(MAX_PIXELS * 4 + 1).expect("sparse output")
        }
        #[cfg(unix)]
        "symlink" => std::os::unix::fs::symlink(root.join("original.avif"), &output).expect("output link"),
        _ => fs::write(&output, PIXELS).expect("output"),
    }
    #[cfg(unix)]
    if mode == "readonlyStage" {
        // Core
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(output.parent().expect("staging"), fs::Permissions::from_mode(0o500))
            .expect("read-only staging");
    }
    let width = match mode.as_str() {
        "overflow" => u64::MAX,
        "zero" => 0,
        _ => 2,
    };
    let bytes = if mode == "wrongBytes" { 9 } else { 8 };
    let status = if mode == "statusError" { "error" } else { "ok" };
    println!("width={width}\nheight=1\noutput_bytes={bytes}\nstatus={status}");
}
