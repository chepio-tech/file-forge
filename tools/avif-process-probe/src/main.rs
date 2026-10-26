// Core
use std::ffi::OsString;
use std::process::ExitCode;
use std::time::Duration;
// Services
use fileforge_avif_process_probe::supervisor::{
    self, Cancellation, MAX_ALLOCATION, MAX_DEADLINE, MAX_PIXELS, Options, Outcome,
};

const USAGE: &str = "usage: fileforge-avif-process-probe --decoder PATH INPUT OUTPUT.rgba [--cancel-before-start] [--cancel-after-decode-ms N] [--timeout-ms N] [--max-pixels N] [--allocation-budget N]";

fn options(mut args: impl Iterator<Item = OsString>) -> Result<(Options, bool), String> {
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--decoder")) {
        return Err(USAGE.into());
    }
    let decoder = args.next().ok_or(USAGE)?.into();
    let input = args.next().ok_or(USAGE)?.into();
    let output = args.next().ok_or(USAGE)?.into();
    let mut options = Options {
        decoder,
        input,
        output,
        deadline: MAX_DEADLINE,
        cancel_after_decode: None,
        max_pixels: MAX_PIXELS,
        allocation_budget: MAX_ALLOCATION,
    };
    let mut before = false;
    while let Some(flag) = args.next() {
        if flag == "--cancel-before-start" {
            before = true;
            continue;
        }
        let value: u64 =
            args.next().and_then(|value| value.into_string().ok()).ok_or(USAGE)?.parse().map_err(|_| USAGE)?;
        match flag.to_str() {
            Some("--cancel-after-decode-ms") => options.cancel_after_decode = Some(Duration::from_millis(value)),
            Some("--timeout-ms") => options.deadline = Duration::from_millis(value),
            Some("--max-pixels") => options.max_pixels = value,
            Some("--allocation-budget") => options.allocation_budget = value,
            _ => return Err(USAGE.into()),
        }
    }
    Ok((options, before))
}

fn main() -> ExitCode {
    let (options, before) = match options(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(4);
        }
    };
    let cancellation = Cancellation::default();
    if before {
        cancellation.request();
    }
    let report = supervisor::run(&options, &cancellation);
    let (status, code) = match report.outcome {
        Outcome::Completed if report.cleanup_ok => ("ok", 0),
        Outcome::Cancelled => ("cancelled", 2),
        Outcome::Deadline => ("deadline", 3),
        _ => ("error", 4),
    };
    println!("status={status}");
    println!("child_reaped={}", report.child_reaped);
    println!("published={}", report.published);
    println!("cleanup_ok={}", report.cleanup_ok);
    println!("elapsed_ms={:.3}", report.elapsed_ms);
    if let Some(pid) = report.child_id {
        println!("child_pid={pid}");
    }
    if let Some(value) = report.cancel_reap_ms {
        println!("cancel_reap_ms={value:.3}");
    }
    if let Some(value) = report.cancel_settle_ms {
        println!("cancel_settle_ms={value:.3}");
    }
    if let Some((width, height, bytes)) = report.geometry {
        println!("width={width}\nheight={height}\noutput_bytes={bytes}");
    }
    if !report.error.is_empty() {
        println!("error={}", report.error.escape_default());
    }
    if !report.stderr.is_empty() {
        println!("decoder_stderr={}", report.stderr.escape_default());
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    // Services
    use super::*;

    #[test]
    fn parses_paths_and_tighter_bounds() {
        let (options, before) = options(
            [
                "--decoder",
                "decoder path",
                "in",
                "out",
                "--cancel-before-start",
                "--timeout-ms",
                "900",
                "--cancel-after-decode-ms",
                "5",
                "--max-pixels",
                "12",
                "--allocation-budget",
                "4096",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .expect("arguments");
        assert_eq!(options.decoder.to_str(), Some("decoder path"));
        assert_eq!(options.deadline, Duration::from_millis(900));
        assert_eq!(options.cancel_after_decode, Some(Duration::from_millis(5)));
        assert_eq!((options.max_pixels, options.allocation_budget), (12, 4096));
        assert!(before);
    }

    #[test]
    fn rejects_incomplete_unknown_or_non_numeric_arguments() {
        for args in [
            vec![],
            vec!["--decoder"],
            vec!["--decoder", "bin", "in", "out", "--unknown", "1"],
            vec!["--decoder", "bin", "in", "out", "--timeout-ms", "-1"],
            vec!["--decoder", "bin", "in", "out", "--timeout-ms"],
        ] {
            assert!(options(args.into_iter().map(OsString::from)).is_err());
        }
    }
}
