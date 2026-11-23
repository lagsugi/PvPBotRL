import copy
import json
import unittest

from analyze_transport import check_transport
from provenance import ROOT


class TransportComparisonTest(unittest.TestCase):
    def setUp(self):
        capture = ROOT / "tests/fixtures/parity/combat/motion_transport_zero/bot.json"
        self.events = json.loads(capture.read_text())["motion_transport"]

    def test_real_send_and_application_preserve_wire_decoding(self):
        for name, delay, lateness in (("motion_transport_zero", 0, 0), ("motion_transport_51ms", 51_000_000, 49_000_000)):
            with self.subTest(name=name):
                path = ROOT / "tests/fixtures/parity/combat" / name / "bot.json"
                sends, applications = check_transport(json.loads(path.read_text())["motion_transport"])
                self.assertEqual(len(sends), 2)
                self.assertEqual(sends[1]["lpvec3_hex"], "f9ffb3315c59")
                self.assertEqual(sends[1]["decoded_velocity"], applications[1]["velocity"])
                self.assertEqual(applications[1]["before_velocity"], [0.0, -0.0784000015258789, 0.0])
                self.assertEqual(sends[1]["ready_ns"] - sends[1]["sent_ns"], delay)
                self.assertEqual(applications[1]["applied_ns"] - sends[1]["ready_ns"], lateness)

    def test_missing_ordered_or_malformed_records_fail(self):
        mutations = [
            lambda e: e.pop(),
            lambda e: e.reverse(),
            lambda e: e[3].update(ready_ns=e[3]["applied_ns"]+1),
            lambda e: e[3].update(applied_ns=e[3]["applied_ns"]-1),
            lambda e: e[3].update(velocity=[]),
            lambda e: e[3].update(velocity=[0, float("nan"), 0]),
            lambda e: e[3].update(phase="post_physics"),
            lambda e: e[3].update(sequence=True),
            lambda e: e[2].update(ready_ns=e[2]["sent_ns"]-1),
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                events = copy.deepcopy(self.events)
                mutation(events)
                with self.assertRaises(ValueError): check_transport(events)


if __name__ == "__main__": unittest.main()
