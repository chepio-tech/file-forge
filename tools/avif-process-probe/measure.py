#!/usr/bin/env python3
"""Measure process supervision on the existing owned corpus; never alters decoder acceptance gates."""
# Core
import argparse
import json
import math
import os
from pathlib import Path
import platform
import signal
import statistics
import subprocess
import sys
import time
# Services
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "avif-probe"))
from mergeExperiment.compare import corpus_cases, identical
from verify import file_hash

TARGET_MS = 250


def fields(text):
    result = {}
    for line in text.splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            if key in result:
                raise ValueError("duplicate report field")
            result[key] = value
    return result


def latency(value):
    try:
        number = float(value)
        return math.isfinite(number) and 0 <= number <= TARGET_MS
    except (ValueError, TypeError):
        return False


def cancelled(run):
    values = run["fields"]
    return (run["exit_code"] == 2 and not run["timed_out"] and run["original_unchanged"]
            and not run["output_exists"] and run["staging_clean"] and run["child_absent"] is True
            and values.get("status") == "cancelled" and values.get("child_reaped") == "true"
            and values.get("published") == "false" and values.get("cleanup_ok") == "true"
            and latency(values.get("cancel_reap_ms")) and latency(values.get("cancel_settle_ms")))


def completed(run, supervised):
    values = run["fields"]
    good = (run["exit_code"] == 0 and not run["timed_out"] and run["original_unchanged"]
            and run["output_exists"] and values.get("status") == "ok" and run["staging_clean"])
    return good and (not supervised or (values.get("child_reaped") == "true"
                    and values.get("published") == "true" and values.get("cleanup_ok") == "true"
                    and run["child_absent"] is True))


def absent(pid):
    if pid is None:
        return None
    try:
        os.kill(int(pid), 0)
        return False
    except ProcessLookupError:
        return True


def execute(command, source, destination, directory, label):
    """POSIX watchdog owns a fresh session, so a broken supervisor cannot strand its worker."""
    before = file_hash(source)
    started = time.monotonic()
    timed_out = False
    with (directory / f"{label}.stdout").open("x") as stdout, (directory / f"{label}.stderr").open("x") as stderr:
        process = subprocess.Popen([str(part) for part in command], stdin=subprocess.DEVNULL,
                                   stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            try:
                process.wait(timeout=25)
            except subprocess.TimeoutExpired:
                timed_out = True
        finally:
            if process.returncode is None:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
    values = fields((directory / f"{label}.stdout").read_text())
    return {"exit_code": process.returncode, "timed_out": timed_out,
            "wall_ms": (time.monotonic() - started) * 1000, "fields": values,
            "original_unchanged": file_hash(source) == before, "source_sha256": before,
            "output_exists": destination.exists(), "staging_clean": not list(directory.glob(".fileforge-avif-*")),
            "child_absent": absent(values.get("child_pid")),
            "stderr": (directory / f"{label}.stderr").read_text()}


def measure(supervisor, decoder, unpatched, corpus, output, repetitions):
    cases = corpus_cases(json.loads((corpus / "report.json").read_text()))
    for case in cases:
        if file_hash(corpus / "fixtures" / f"{case['name']}.avif") != case["source_sha256"]:
            raise ValueError("owned fixture hash changed")
    hashes = {case["name"]: case["source_sha256"] for case in cases}
    output.mkdir(parents=True, exist_ok=False)
    binaries = {"supervisor": supervisor, "decoder": decoder, "unpatched": unpatched}
    report = {"platform": platform.platform(), "python": platform.python_version(),
              "binary_sha256": {name: file_hash(path) for name, path in binaries.items()},
              "corpus_report_sha256": file_hash(corpus / "report.json"), "cancel_target_ms": TARGET_MS,
              "runs": [], "checks": [], "summary": {}}

    def save():
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    def check(name, passed):
        report["checks"].append({"name": name, "passed": bool(passed)})
        save()
        print(f"{name}: {'pass' if passed else 'FAIL'}", flush=True)

    def invoke(name, fixture, worker, supervised=True, extra=()):
        source = corpus / "fixtures" / f"{fixture}.avif"
        destination = output / f"{name}.rgba"
        command = ([supervisor, "--decoder", worker] if supervised else [worker]) + [source, destination, *extra]
        run = execute(command, source, destination, output, name)
        run["original_unchanged"] &= run["source_sha256"] == hashes[fixture]
        run.update(name=name, fixture=fixture, supervised=supervised, decoder_sha256=file_hash(worker))
        report["runs"].append(run)
        save()
        return run, destination

    def pair(fixture, repeat):
        runs, destinations = {}, {}
        for supervised in ([False, True] if repeat % 2 == 0 else [True, False]):
            label = f"{fixture}-{repeat}-{'supervised' if supervised else 'direct'}"
            runs[supervised], destinations[supervised] = invoke(label, fixture, decoder, supervised)
        equal = (completed(runs[False], False) and completed(runs[True], True)
                 and all(runs[False]["fields"].get(key) == runs[True]["fields"].get(key)
                         for key in ("width", "height", "output_bytes"))
                 and identical(destinations[False], destinations[True]))
        check(f"{fixture}-{repeat}-exact-output", equal)

    for case in cases:
        pair(case["name"], 0)
    for fixture in ("medium", "large"):
        for repeat in (1, 2):
            pair(fixture, repeat)

    for variant, worker in (("patched", decoder), ("unpatched", unpatched)):
        for delay in (5, 100):
            for repeat in range(repetitions):
                name = f"cancel-{variant}-{delay}-{repeat}"
                run, _ = invoke(name, "large", worker, extra=("--cancel-after-decode-ms", str(delay)))
                check(name, cancelled(run))

    before, _ = invoke("pre-cancel", "large", decoder, extra=("--cancel-before-start",))
    check("pre-cancel-no-child", before["exit_code"] == 2 and before["fields"].get("status") == "cancelled"
          and "child_pid" not in before["fields"] and not before["output_exists"]
          and before["original_unchanged"] and before["staging_clean"] and not before["timed_out"])
    deadline, _ = invoke("deadline", "large", decoder, extra=("--timeout-ms", "100"))
    check("whole-operation-deadline", deadline["exit_code"] == 3 and deadline["fields"].get("status") == "deadline"
          and deadline["fields"].get("child_reaped") == "true" and deadline["child_absent"] is True
          and not deadline["output_exists"] and deadline["original_unchanged"] and deadline["staging_clean"]
          and deadline["fields"].get("cleanup_ok") == "true" and not deadline["timed_out"])
    refused, _ = invoke("pixel-limit", "large", decoder, extra=("--max-pixels", "1"))
    check("decoder-limit-refusal", refused["exit_code"] == 4 and refused["fields"].get("status") == "error"
          and refused["fields"].get("child_reaped") == "true" and refused["child_absent"] is True
          and not refused["output_exists"] and refused["original_unchanged"] and refused["staging_clean"]
          and refused["fields"].get("cleanup_ok") == "true" and not refused["timed_out"])
    recovery, destination = invoke("recovery", "large", decoder)
    check("complete-after-cancel-and-refusal", completed(recovery, True)
          and identical(destination, output / "large-0-direct.rgba"))

    for variant in ("patched", "unpatched"):
        runs = [run for run in report["runs"] if run["name"].startswith(f"cancel-{variant}-")]
        summary = {"runs": len(runs), "passed": sum(cancelled(run) for run in runs)}
        for key in ("cancel_reap_ms", "cancel_settle_ms"):
            values = [float(run["fields"][key]) for run in runs if key in run["fields"]]
            summary[key] = {"median": statistics.median(values) if values else None, "max": max(values, default=None)}
        report["summary"][variant] = summary
    for fixture in ("medium", "large"):
        report["summary"][fixture] = {}
        for supervised in (False, True):
            suffix = "supervised" if supervised else "direct"
            runs = [run for run in report["runs"] if run["fixture"] == fixture and run["name"].endswith(suffix)]
            key = "elapsed_ms" if supervised else "total_ms"
            values = [float(run["fields"][key]) for run in runs if key in run["fields"]]
            report["summary"][fixture][suffix] = {"operation_ms": values,
                "median_operation_ms": statistics.median(values) if values else None}
    report["passed"] = all(check["passed"] for check in report["checks"])
    save()
    print(json.dumps(report["summary"], indent=2))
    return report["passed"]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("supervisor", "decoder", "unpatched", "corpus", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--repetitions", type=int, default=5, choices=range(1, 11))
    args = parser.parse_args()
    if os.name != "posix":
        parser.error("the diagnostic watchdog uses POSIX sessions; portable lifecycle tests run through cargo test")
    good = measure(args.supervisor.resolve(strict=True), args.decoder.resolve(strict=True),
                   args.unpatched.resolve(strict=True), args.corpus.resolve(strict=True), args.output.resolve(),
                   args.repetitions)
    raise SystemExit(0 if good else 1)
