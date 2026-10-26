#!/usr/bin/env python3
"""Compare unchanged owned fixtures, then repeat timing pairs without changing acceptance gates."""
# Core
import argparse
import json
from pathlib import Path
import platform
import re
import statistics
import sys
# Services
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify import execute, file_hash


def identical(left, right):
    with left.open("rb") as a, right.open("rb") as b:
        while True:
            first, second = a.read(65536), b.read(65536)
            if first != second:
                return False
            if not first:
                return True


def corpus_cases(report):
    cases = [case for case in report["cases"] if case["kind"] == "pixels"]
    names = set()
    for case in cases:
        name = case["name"]
        if not re.fullmatch(r"[a-zA-Z0-9_-]+", name) or name in names:
            raise ValueError("corpus requires unique safe fixture names")
        if not re.fullmatch(r"[a-f0-9]{64}", case["source_sha256"]):
            raise ValueError("corpus requires source hashes")
        names.add(name)
    if not {"medium", "large"} <= names:
        raise ValueError("corpus needs medium and large timing fixtures")
    return cases


def compare(baseline, patched, corpus, output, repetitions, timeout):
    cases = corpus_cases(json.loads((corpus / "report.json").read_text()))
    for case in cases:
        if file_hash(corpus / "fixtures" / (case["name"] + ".avif")) != case["source_sha256"]:
            raise ValueError("fixture differs from its recorded source hash")
    binaries = {"baseline": baseline, "patched": patched}
    output.mkdir(parents=True, exist_ok=False)
    report = {"platform": platform.platform(), "python": platform.python_version(),
              "binary_sha256": {name: file_hash(path) for name, path in binaries.items()},
              "corpus_report_sha256": file_hash(corpus / "report.json"),
              "diagnostic_deadline_seconds": timeout, "repetitions": repetitions,
              "runs": [], "comparisons": [], "summary": {}}

    def pair(case, repeat):
        name = case["name"]
        source = corpus / "fixtures" / (name + ".avif")
        runs, outputs = {}, {}
        order = ("baseline", "patched") if repeat % 2 == 0 else ("patched", "baseline")
        for variant in order:
            label = f"{name}-{repeat}-{variant}"
            destination = output / (label + ".rgba")
            run = execute(binaries[variant], source, destination, output / label, timeout=timeout)
            run.update(fixture=name, repetition=repeat, variant=variant)
            report["runs"].append(run)
            runs[variant], outputs[variant] = run, destination
        complete = all(run["exit_code"] == 0 and not run["timed_out"] and run["original_unchanged"]
                       and run["source_sha256"] == case["source_sha256"] and outputs[variant].is_file()
                       for variant, run in runs.items())
        dimensions_equal = all(runs["baseline"]["fields"].get(field) == runs["patched"]["fields"].get(field)
                               for field in ("width", "height", "output_bytes"))
        equal = complete and dimensions_equal and identical(outputs["baseline"], outputs["patched"])
        report["comparisons"].append({"fixture": name, "repetition": repeat,
                                      "status": "equal" if equal else "mismatch" if complete else "incomplete"})
        print(f"{name} pair {repeat + 1}: " + report["comparisons"][-1]["status"], flush=True)
        save()

    def save():
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    # Small fixtures are compared once; medium/large pairs are alternated and repeated.
    for case in cases:
        if case["name"] not in ("medium", "large"):
            pair(case, 0)
    for name in ("medium", "large"):
        case = next(case for case in cases if case["name"] == name)
        for repeat in range(repetitions):
            pair(case, repeat)
        report["summary"][name] = {}
        for variant in binaries:
            runs = [run for run in report["runs"] if run["fixture"] == name and run["variant"] == variant]
            times = [float(run["fields"]["decode_ms"]) for run in runs if "decode_ms" in run["fields"]]
            peaks = [run["peak_rss_bytes"] for run in runs if run["peak_rss_bytes"] is not None]
            report["summary"][name][variant] = {"completed": len(times), "requested": repetitions,
                "decode_ms": times, "median_decode_ms": statistics.median(times) if times else None,
                "peak_rss_bytes": peaks, "maximum_peak_rss_bytes": max(peaks) if peaks else None}
        save()
    report["passed"] = all(case["status"] == "equal" for case in report["comparisons"])
    save()
    print(json.dumps(report["summary"], indent=2))
    return report["passed"]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--patched", required=True, type=Path)
    parser.add_argument("--corpus", required=True, type=Path, help="existing owned verify.py output")
    parser.add_argument("--output", required=True, type=Path, help="new evidence directory")
    parser.add_argument("--repetitions", type=int, default=3, choices=range(1, 10))
    parser.add_argument("--timeout", type=int, default=90, choices=range(1, 301),
                        help="diagnostic child deadline; does not alter verify.py's 20-second gate")
    args = parser.parse_args()
    good = compare(args.baseline.resolve(strict=True), args.patched.resolve(strict=True),
                   args.corpus.resolve(strict=True), args.output.resolve(), args.repetitions, args.timeout)
    raise SystemExit(0 if good else 1)
