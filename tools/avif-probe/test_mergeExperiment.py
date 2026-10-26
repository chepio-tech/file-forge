# Core
import copy
from contextlib import redirect_stdout
import hashlib
from io import StringIO
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
# Domain
from mergeExperiment.compare import compare, corpus_cases, identical
from mergeExperiment.prepare import code_member, prepare, source_pin, validate_lock


class MergeExperimentTests(unittest.TestCase):
    def test_exact_comparison_checks_content_and_length_across_chunks(self):
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory) / "a", Path(directory) / "b"
            a.write_bytes(b"a" * 65536 + b"bc")
            for contents, expected in [(a.read_bytes(), True), (b"a" * 65536 + b"bd", False),
                                       (b"a" * 65536 + b"bcd", False), (b"", False)]:
                b.write_bytes(contents)
                self.assertEqual(identical(a, b), expected)

    def test_archive_selects_code_and_license_and_rejects_escape_paths(self):
        self.assertEqual(code_member("avif-rust-0.1.2/src/lib.rs"), Path("src/lib.rs"))
        self.assertEqual(code_member("avif-rust-0.1.2/LICENSE"), Path("LICENSE"))
        self.assertIsNone(code_member("avif-rust-0.1.2/tests/photo.avif"))
        for path in ("/avif-rust-0.1.2/src/lib.rs", "avif-rust-0.1.2/src/../../outside", "wrong/src/lib.rs"):
            with self.assertRaises(ValueError):
                code_member(path)

    def test_only_approved_source_pin_can_change_in_experimental_lock(self):
        baseline = {"version": 4, "package": [
            {"name": "avif-rust", "version": "0.1.2", "source": "registry", "checksum": "abc"},
            {"name": "bin-rs", "version": "0.0.10", "checksum": "def"}]}
        self.assertEqual(source_pin(baseline), "abc")
        patched = copy.deepcopy(baseline)
        del patched["package"][0]["source"], patched["package"][0]["checksum"]
        validate_lock(baseline, patched)
        patched["package"][1]["version"] = "0.0.11"
        with self.assertRaises(ValueError):
            validate_lock(baseline, patched)
        baseline["package"][0]["version"] = "0.1.3"
        with self.assertRaises(ValueError):
            source_pin(baseline)

    def test_corpus_rejects_duplicate_names_escape_paths_and_missing_hashes(self):
        cases = [{"kind": "pixels", "name": name, "source_sha256": "a" * 64} for name in ("medium", "large")]
        self.assertEqual(corpus_cases({"cases": cases}), cases)
        for altered in ([*cases, cases[0]], [dict(cases[0], name="../medium"), cases[1]],
                        [dict(cases[0], source_sha256=""), cases[1]], [cases[0]]):
            with self.assertRaises(ValueError):
                corpus_cases({"cases": altered})

    def test_success_exit_without_output_is_incomplete_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "fixtures").mkdir()
            digest = hashlib.sha256(b"owned").hexdigest()
            cases = []
            for name in ("medium", "large"):
                (root / "fixtures" / (name + ".avif")).write_bytes(b"owned")
                cases.append({"kind": "pixels", "name": name, "source_sha256": digest})
            (root / "report.json").write_text(json.dumps({"cases": cases}))
            binary = root / "binary"
            binary.write_bytes(b"fixture")
            run = {"exit_code": 0, "timed_out": False, "original_unchanged": True,
                   "source_sha256": digest, "fields": {}, "peak_rss_bytes": None}
            with patch("mergeExperiment.compare.execute", side_effect=lambda *a, **kw: copy.deepcopy(run)), redirect_stdout(StringIO()):
                self.assertFalse(compare(binary, binary, root, root / "output", 1, 1))
            result = json.loads((root / "output/report.json").read_text())
            self.assertTrue(all(case["status"] == "incomplete" for case in result["comparisons"]))

    def test_wrong_archive_checksum_creates_no_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "archive"
            archive.write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                prepare(archive, root / "output")
            self.assertFalse((root / "output").exists())
