import json
from pathlib import Path
import tempfile
import unittest

from extract_impulse import extract
from provenance import ROOT


class ImpulseExtractionTest(unittest.TestCase):
    def setUp(self):
        self.source = ROOT / "tests/fixtures/parity/combat/received_impulse/telemetry.ndjson"

    def test_saved_received_impulse_is_reproducible(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "fixture"
            extract(self.source, target)
            saved = ROOT / "tests/fixtures/parity/motion/received_knockback_idle"
            for name in ("scenario.json", "initial.json", "client.ndjson"):
                self.assertEqual((target / name).read_bytes(), (saved / name).read_bytes())

    def test_missing_end_or_tick_is_not_accepted(self):
        rows = [json.loads(line) for line in self.source.read_text().splitlines()]
        packet = next(i for i, r in enumerate(rows) if r["event"] == "self_motion_applied")
        for removed in (-1, packet+1):
            with self.subTest(removed=removed), tempfile.TemporaryDirectory() as temporary:
                data = list(rows)
                data.pop(removed)
                source = Path(temporary) / "source.ndjson"
                source.write_text("\n".join(json.dumps(r) for r in data))
                with self.assertRaises(ValueError): extract(source, Path(temporary) / "fixture")

    def test_changed_sequence_or_input_cannot_be_hidden(self):
        for field in ("client_tick", "input"):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                rows = [json.loads(line) for line in self.source.read_text().splitlines()]
                packet = next(i for i, r in enumerate(rows) if r["event"] == "self_motion_applied")
                row = rows[packet+1]
                if field == "client_tick": row["client_tick"] += 1
                else: row["data"]["state"]["input"]["forward"] = True
                source = Path(temporary) / "source.ndjson"
                source.write_text("\n".join(json.dumps(r) for r in rows))
                with self.assertRaises(ValueError): extract(source, Path(temporary) / "fixture")


if __name__ == "__main__": unittest.main()
