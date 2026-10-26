# Core
import copy
import unittest
# Domain
from measure import cancelled, completed, fields, latency


class EvidenceTests(unittest.TestCase):
    def cancellation(self):
        return {"exit_code": 2, "timed_out": False, "original_unchanged": True, "output_exists": False,
                "staging_clean": True, "child_absent": True, "fields": {"status": "cancelled",
                "child_reaped": "true", "published": "false", "cleanup_ok": "true",
                "cancel_reap_ms": "2.5", "cancel_settle_ms": "3.2"}}

    def test_rejects_missing_slow_non_finite_or_negative_latency(self):
        for value in (None, "", "NaN", "inf", "-1", "250.001", "not a number"):
            self.assertFalse(latency(value), value)
        for value in ("0", "249.999", "250"):
            self.assertTrue(latency(value), value)

    def test_cancellation_requires_actual_reaping_cleanup_and_unchanged_input(self):
        good = self.cancellation()
        self.assertTrue(cancelled(good))
        for key, value in (("exit_code", 0), ("timed_out", True), ("original_unchanged", False),
                           ("output_exists", True), ("staging_clean", False), ("child_absent", False),
                           ("child_absent", None)):
            changed = copy.deepcopy(good)
            changed[key] = value
            self.assertFalse(cancelled(changed), key)
        for key in good["fields"]:
            changed = copy.deepcopy(good)
            del changed["fields"][key]
            self.assertFalse(cancelled(changed), key)

    def test_success_requires_published_output_and_supervised_child_termination(self):
        good = self.cancellation()
        good.update(exit_code=0, output_exists=True)
        good["fields"].update(status="ok", published="true")
        self.assertTrue(completed(good, True))
        for key in ("child_reaped", "published", "cleanup_ok"):
            changed = copy.deepcopy(good)
            changed["fields"][key] = "false"
            self.assertFalse(completed(changed, True))
        good["output_exists"] = False
        self.assertFalse(completed(good, False))

    def test_report_parser_preserves_values_and_refuses_ambiguous_duplicate_keys(self):
        self.assertEqual(fields("status=ok\nerror=some=value\n"), {"status": "ok", "error": "some=value"})
        with self.assertRaises(ValueError):
            fields("status=ok\nstatus=cancelled\n")


if __name__ == "__main__":
    unittest.main()
