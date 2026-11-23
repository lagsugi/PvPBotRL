import copy
import json
import unittest

from analyze_uplink import correlate
from extract_impulse import read_telemetry
from provenance import ROOT


class UplinkTraceTest(unittest.TestCase):
    def setUp(self):
        root = ROOT / "tests/fixtures/parity/combat/measured_uplink"
        self.server = json.loads((root / "player.json").read_text())
        self.client = read_telemetry(root / "telemetry.ndjson")

    def test_captured_motion_values_match_on_both_sides(self):
        result = correlate(self.server, self.client)
        self.assertFalse(result["passed"])
        self.assertEqual(result["matched_packet_prefix"], 8)
        self.assertEqual(result["first_changed_server_tick_end"] - result["damage_server_tick"], 1)
        self.assertEqual(result["impulse_outbound"]["packet"]["lpvec3_hex"], "f9ffb3315c59")

    def test_dropped_and_reordered_receipts_are_rejected(self):
        for field in ("dropped", "order"):
            with self.subTest(field=field):
                data = copy.deepcopy(self.server)
                connection = data["player_connections"]["opponent"]
                if field == "dropped":
                    connection["dropped"] = 1
                else:
                    connection["events"][1], connection["events"][2] = connection["events"][2], connection["events"][1]
                with self.assertRaises(ValueError):
                    correlate(data, self.client)
