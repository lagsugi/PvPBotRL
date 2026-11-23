import copy
import json
import unittest
from analyze_combat import compare_trials
from provenance import ROOT


class CombatComparisonTest(unittest.TestCase):
    def setUp(self):
        directory = ROOT / "tests/fixtures/parity/combat/single_charged_hit"
        self.data = [json.loads((directory / name).read_text()) for name in ("player.json", "bot.json")]
        self.oracle = {"result": {"damage": {"final_damage": 0.6803999543190002}}, "health_after": 19.319599151611328}

    def test_real_divergence_is_preserved(self):
        result = compare_trials(self.data, self.oracle)
        self.assertFalse(result["passed"])
        self.assertFalse(result["measured_checks_passed"])
        self.assertTrue(result["damage_matches_rust"])
        self.assertTrue(result["health_matches_rust"])
        self.assertEqual(result["first_motion_difference"]["tick"], 12)
        self.assertAlmostEqual(result["final_position_difference"][2], 6.072146879443352e-05)
        self.assertEqual(len(result["context"]), 5)

    def test_missing_and_malformed_measurements_fail(self):
        mutations = [
            lambda d: d[0].update(status="incomplete"),
            lambda d: d[0]["events"].pop(),
            lambda d: d[0]["events"][1]["opponent"].update(velocity=[]),
            lambda d: d[0]["events"][1]["opponent"].update(health=float("nan")),
            lambda d: d[0]["events"][1]["opponent"].update(on_ground=1),
            lambda d: d[0]["events"][1]["opponent"].pop("charge"),
            lambda d: d[0]["events"][1].update(server_tick=-1),
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                data = copy.deepcopy(self.data)
                mutation(data)
                with self.assertRaises((ValueError, KeyError)):
                    compare_trials(data, self.oracle)

    def test_identical_measurements_cannot_certify_missing_gates(self):
        data = [self.data[0], copy.deepcopy(self.data[0])]
        data[1]["bot"] = True
        result = compare_trials(data, self.oracle)
        self.assertTrue(result["measured_checks_passed"])
        self.assertFalse(result["passed"])
        self.assertEqual(result["acceptance_status"], "incomplete")

    def test_shorter_complete_trial_cannot_match_the_reference(self):
        bot = copy.deepcopy(self.data[0])
        bot.update(bot=True, capture_ticks=60)
        bot["events"] = [event for event in bot["events"]
                         if event["event"] != "state" or event["tick"] <= 60]
        with self.assertRaisesRegex(ValueError, "Combat state ticks differ"):
            compare_trials([self.data[0], bot], self.oracle)

    def test_discrete_difference_never_uses_numeric_tolerance(self):
        data = [self.data[0], copy.deepcopy(self.data[0])]
        data[1]["bot"] = True
        data[1]["events"][2]["opponent"]["on_ground"] = False
        result = compare_trials(data, self.oracle)
        self.assertFalse(result["measured_checks_passed"])
        self.assertEqual(result["first_state_difference"]["field"], "on_ground")


if __name__ == "__main__": unittest.main()
