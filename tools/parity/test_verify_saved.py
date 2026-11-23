from contextlib import redirect_stderr
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import verify_saved


class SavedComparisonTest(unittest.TestCase):
    def test_missing_sprint_fixture_fails_before_replay(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture_root = root / "tests/fixtures/parity/motion"
            for scenario in verify_saved.REQUIRED_SCENARIOS - {"sprint_feedback"}:
                (fixture_root / scenario).mkdir(parents=True)
            stderr = io.StringIO()
            with patch.object(verify_saved, "ROOT", root), patch("verify_saved.subprocess.run") as run:
                with redirect_stderr(stderr):
                    self.assertEqual(verify_saved.main(), 1)
                run.assert_not_called()
            self.assertEqual(json.loads(stderr.getvalue()), {"missing_scenarios": ["sprint_feedback"]})
