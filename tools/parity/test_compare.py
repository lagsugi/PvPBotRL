import json
from pathlib import Path
import tempfile
import unittest

from compare import compare, FIELDS


class CompareTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.records = [
            {"schema": 1, "event": "manifest", "scenario": "test", "scenario_sha256": "x",
             "initial_sha256": "y", "expected_samples": 1},
            {"schema": 1, "event": "state", "step": 0, "phase": "post_movement", "actor": 0,
             "channel": "local", "logical_ns": 0, "state": {k: 0.0 for k in FIELDS["motion"]}},
            {"schema": 1, "event": "end", "status": "complete", "samples": 1, "dropped": 0}]
        self.records[1]["state"].update(on_ground=True, sprinting=False, pose="standing")

    def write(self, name, records):
        path = self.root / name
        path.write_text("\n".join(json.dumps(r) for r in records), encoding="utf-8")
        return path

    def check(self, edit):
        a = self.write("a", self.records)
        edit(self.records)
        return compare(a, self.write("b", self.records))

    def test_identical_and_small_roundoff(self):
        self.assertTrue(self.check(lambda r: r[1]["state"].update({"position.x": 5e-7}))["passed"])

    def test_first_difference_and_context(self):
        result = self.check(lambda r: r[1]["state"].update({"position.x": 2e-6}))
        self.assertFalse(result["passed"])
        self.assertEqual("position.x", result["first_difference"]["field"])

    def test_discrete_difference_never_uses_float_epsilon(self):
        self.assertFalse(self.check(lambda r: r[1]["state"].update(jump_delay=1))["passed"])

    def test_missing_field(self):
        self.assertFalse(self.check(lambda r: r[1]["state"].pop("velocity.y"))["passed"])

    def test_truncated_file(self):
        self.assertFalse(self.check(lambda r: r.pop())["passed"])

    def test_drop_count(self):
        self.assertFalse(self.check(lambda r: r[-1].update(dropped=1))["passed"])

    def test_nan(self):
        self.assertFalse(self.check(lambda r: r[1]["state"].update(yaw=float("nan")))["passed"])

    def test_matching_invalid_required_numbers_fail(self):
        for field, value in (("position.x", "unmeasured"), ("velocity.y", True),
                             ("health", "unmeasured"), ("health", float("inf"))):
            with self.subTest(field=field, value=value):
                state = self.records[1]["state"]
                state.update({key: 0 for key in FIELDS["combat"]})
                state["position.x"] = state["velocity.y"] = 0
                state[field] = value
                reference = self.write("a", self.records)
                candidate = self.write("b", self.records)
                result = compare(reference, candidate, profiles=("motion", "combat"))
                self.assertFalse(result["passed"])
                self.assertIn(f"Invalid required field {field}", result["failures"][0])

    def test_no_realigning_ticks(self):
        self.assertFalse(self.check(lambda r: r[1].update(step=1))["passed"])

    def test_logical_time_is_required_in_both_captures(self):
        state = self.records[1]
        del state["logical_ns"]
        path = self.write("trace", self.records)
        self.assertIn("Expected integer logical_ns", compare(path, path)["failures"][0])

        for value in (None, False, 0.0, "0"):
            with self.subTest(value=value):
                state["logical_ns"] = value
                path = self.write("trace", self.records)
                self.assertIn("Expected integer logical_ns", compare(path, path)["failures"][0])

        state["logical_ns"] = 0
        result = self.check(lambda rows: rows[1].update(logical_ns=50_000_000))
        self.assertIn("Logical time mismatch", result["failures"][0])

    def test_different_initial_conditions(self):
        self.assertFalse(self.check(lambda r: r[0].update(initial_sha256="different"))["passed"])

    def test_extra_event_cannot_be_silently_ignored(self):
        self.assertFalse(self.check(lambda r: r.insert(1, {"schema": 1, "event": "attack"}))["passed"])

    def test_boolean_and_number_are_not_interchangeable(self):
        self.assertFalse(self.check(lambda r: r[1]["state"].update(on_ground=1))["passed"])


if __name__ == "__main__":
    unittest.main()
