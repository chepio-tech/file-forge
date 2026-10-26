"""Tests for evidence classification, independent of the optional Pillow oracle."""
# Core
import tempfile
import unittest
from pathlib import Path
# Domain
from verify import pixel_metrics, refused
from referencePlanes import compare_planes


class EvidenceTests(unittest.TestCase):
    def test_native_plane_check_requires_samples_geometry_and_every_channel(self):
        fields = {"plane_0_width": "2", "plane_0_height": "1", "plane_0_hex": "0080"}
        self.assertTrue(compare_planes(fields, {0: (2, 1, b"\0\x80")}))
        self.assertFalse(compare_planes(fields, {0: (2, 1, b"\0\x81")}))
        self.assertFalse(compare_planes(fields, {0: (1, 2, b"\0\x80")}))
        self.assertFalse(compare_planes(fields, {0: (2, 1, b"\0\x80"), 3: (2, 1, b"\xff\xff")}))
        for change in [{"plane_0_width": "invalid"}, {"plane_0_hex": "xyz"}, {"plane_0_hex": "0"}]:
            self.assertFalse(compare_planes(fields | change, {0: (2, 1, b"\0\x80")}))

    def test_alpha_errors_are_never_hidden_by_transparent_rgb(self):
        result = pixel_metrics(bytes([200, 30, 40, 1]), bytes([0, 0, 0, 0]))
        self.assertEqual(result["alpha_max_error"], 1)

    def test_invisible_rgb_does_not_pollute_display_error(self):
        result = pixel_metrics(bytes([200, 30, 40, 0, 12, 20, 30, 255]), bytes([0, 0, 0, 0, 10, 20, 30, 255]))
        self.assertEqual(result["rgb_max_error"], 2)
        self.assertAlmostEqual(result["rgb_mean_error"], 2 / 3)

    def test_geometry_mismatch_is_not_reported_as_pixel_equality(self):
        with self.assertRaises(ValueError):
            pixel_metrics(b"1234", b"12345678")

    def test_panics_timeouts_and_written_output_are_not_safe_refusals(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "result"
            good = {"exit_code": 2, "timed_out": False, "original_unchanged": True, "fields": {"status": "error"}}
            self.assertTrue(refused(good, output))
            for change in [{"exit_code": 3}, {"exit_code": -9}, {"timed_out": True}, {"original_unchanged": False}]:
                self.assertFalse(refused(good | change, output))
            output.write_bytes(b"partial")
            self.assertFalse(refused(good, output))


if __name__ == "__main__":
    unittest.main()
