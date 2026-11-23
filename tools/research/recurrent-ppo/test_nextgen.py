"""Focused regression tests; native release binaries must be built first."""
import json
from datetime import datetime, timedelta
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import numpy as np
import torch

from league import League, capped, digest, model_id
from native import Batch, FIXED_EQUIPMENT_PROFILE
from nextgen import Campaign
from nextgen_train import NextTrainer, run_stage
from paired_evaluate import execute, make_cells, plan, summarize_cell, judge, aggregate
from policy import Policy
from train import parser

CHAMPION: Path


def setUpModule():
    global CHAMPION
    directory = tempfile.TemporaryDirectory()
    unittest.addModuleCleanup(directory.cleanup)
    CHAMPION = Path(directory.name) / "champion.pt"
    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(61002)
        model = Policy("full", False)
    torch.save(dict(input="full", feedforward=False, policy=model.state_dict()), CHAMPION)
    model.export(CHAMPION.with_suffix(".json"))


def options(output):
    args = parser().parse_args([str(output), "--lanes", "4", "--rollout", "8", "--sequence", "4",
                                "--threads", "1", "--torch-threads", "1", "--max-steps", "5"])
    args.learning_rate, args.burn_in, args.focus = 1e-4, 3, .3
    args.exploit, args.target, args.hp_beta = False, None, .1
    args.epochs, args.target_kl, args.minibatch_tokens = 4, .015, 16
    args.init_checkpoint, args.seconds, args.updates = CHAMPION, 30, 1
    return args


def champion_row():
    path = CHAMPION.with_suffix(".json")
    return dict(path=str(path), id=model_id(path), sha256=digest(path), group="champion")


class LeagueTests(unittest.TestCase):
    def test_capped_distribution(self):
        p = capped([1000] + [1] * 109)
        self.assertAlmostEqual(p.sum(), 1)
        self.assertLessEqual(p.max(), .100000001)
        np.testing.assert_allclose(capped([0, 0]), [.5, .5])

    def test_target_coverage_and_history(self):
        models = [champion_row()] + [dict(id=str(i), group="history") for i in range(110)]
        league, rng = League(models), np.random.default_rng(9)
        counts = {}
        for _ in range(12000):
            r, n = league.choose(rng)
            counts[r, n] = counts.get((r, n), 0) + 1
        for r in (10, 1, 11, 8):
            self.assertGreater(counts[r, 5], 50)
            self.assertGreater(counts[r, 6], 100)
        self.assertEqual(len({r for r, n in counts if r > 18}), 110)

    def test_history_window_and_restore(self):
        league = League([champion_row()])
        for i in range(300):
            league.record(18, 6, outcome=1 if i < 44 else 2, steps=1)
        self.assertEqual(len(league.results[league.key(18, 6)]), 256)
        self.assertLess(league.score(18, 6), .05)
        restored = League([], league.state())
        self.assertEqual(restored.state(), league.state())


class TrainingTests(unittest.TestCase):
    def test_resume_rechecks_a_failed_initial_export(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            champion = root / "initial.pt"
            shutil.copyfile(CHAMPION, champion)
            shutil.copyfile(CHAMPION.with_suffix(".json"), root / "snapshot-0001.json")
            with torch.random.fork_rng(devices=[]):
                torch.manual_seed(61003)
                Policy("full", False).export(champion.with_suffix(".json"))
            args = SimpleNamespace(champion=champion, history=root, output=root / "campaign",
                                   quick=True, resume=False)
            with patch("nextgen.subprocess.run") as regression_tests:
                for resume in (False, True):
                    args.resume = resume
                    campaign = Campaign(args)
                    try:
                        with patch.object(campaign, "evaluate_dev", side_effect=RuntimeError("Reached baseline")) as baseline:
                            with self.assertRaises(AssertionError):
                                campaign.run()
                            baseline.assert_not_called()
                        self.assertNotIn("verified_initial_export", campaign.state)
                    finally:
                        campaign.close()
                regression_tests.assert_called_once()

    def test_burn_in_boundaries_and_rollout_prefix(self):
        args = options(Path("unused"))
        trainer = NextTrainer(args, [champion_row()])
        obs = torch.arange(8).float()[:, None, None].expand(8, 4, 44).clone()
        hidden = torch.zeros(8, 4, 64)
        done = np.zeros((8, 4), bool)
        done[2, :] = True
        rollout = obs, None, hidden, None, None, None, None, done
        segments = trainer.segments(rollout)
        lane0 = [s for s in segments if s[0] == 0]
        self.assertEqual([(s[1], s[2], len(s[3])) for s in lane0], [(0, 3, 0), (3, 7, 0), (7, 8, 3)])
        self.assertEqual([float(p[0][0]) for p in trainer.prefix[0]], [5, 6, 7])
        done[:] = False
        again = trainer.segments(rollout)
        self.assertEqual(len(again[0][3]), 3)
        done[-1] = True
        trainer.segments(rollout)
        self.assertEqual(trainer.prefix[0], [])

    def test_native_slot_pinning_and_reset(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "other.json"
            torch.manual_seed(31)
            Policy("full", False).export(path)
            original = CHAMPION.with_suffix(".json")
            with Batch(1, 1, "full") as a, Batch(1, 1, "full") as b:
                for batch in (a, b):
                    batch.snapshot(120, original)
                    batch.reset(0, 17, 6, 138, 20)
                a.snapshot(120, path)
                for _ in range(10):
                    for batch in (a, b):
                        batch.step(np.zeros((1, 4), dtype=np.uint8), .1)
                    np.testing.assert_array_equal(a.features, b.features)
                    np.testing.assert_array_equal(a.rewards, b.rewards)
                a.snapshot(120, original)
                for batch in (a, b):
                    batch.reset(0, 17, 6, 138, 20)
                np.testing.assert_array_equal(a.features, b.features)

    def test_checkpoint_resume_and_token_accounting(self):
        torch.set_num_threads(1)
        with tempfile.TemporaryDirectory() as tmp:
            args = options(Path(tmp))
            args.fixed_equipment = True
            first = run_stage(args, [champion_row()])
            state = torch.load(Path(tmp) / "latest.pt", weights_only=False)
            self.assertEqual(state["match_profile"], FIXED_EQUIPMENT_PROFILE)
            self.assertEqual(first["updates"], 1)
            self.assertEqual(sum(v["steps"] for v in state["league"]["counts"].values()), 32)
            token_count = sum(v["tokens"] for v in state["league"]["counts"].values())
            self.assertIn(token_count, (32, 64, 96, 128))
            args.resume, args.updates = Path(tmp) / "latest.pt", 2
            second = run_stage(args, [champion_row()])
            self.assertEqual(second["updates"], 2)
            self.assertEqual(second["steps"], 64)
            model = Path(tmp) / "candidate-0000002.json"
            source = json.loads(model.with_suffix(".source.json").read_text(encoding="utf-8"))
            self.assertEqual((source["update"], source["steps"]), (2, 64))
            self.assertEqual(source["sha256"], digest(model))
            self.assertEqual(datetime.fromisoformat(source["savedAt"]).utcoffset(), timedelta(0))
            restored = NextTrainer(args, [champion_row()])
            restored.resume(args.resume)
            with Batch(4, 1, "full", fixed_equipment=True) as batch:
                restored.initialize(batch)
                self.assertEqual(float(restored.memory.abs().sum()), 0)
            args.focus = .5
            with self.assertRaises(ValueError):
                NextTrainer(args, [champion_row()]).resume(args.resume)
            args.focus = .3
            args.fixed_equipment = False
            with self.assertRaisesRegex(ValueError, "match_profile"):
                NextTrainer(args, [champion_row()]).resume(args.resume)

    @unittest.skipUnless(torch.cuda.is_available(), "requires GPU PyTorch")
    def test_gpu_update_and_resume(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = options(Path(tmp))
            args.ppo_device = "cuda"
            trainer = NextTrainer(args, [champion_row()])
            trainer.init_checkpoint(CHAMPION)
            before = trainer.policy.actor.weight.detach().clone()
            with Batch(args.lanes, 1, "full") as batch:
                trainer.initialize(batch)
                stats = trainer.optimize(trainer.collect())
            self.assertEqual(stats["valid_tokens"], args.lanes * args.rollout * stats["epochs"])
            cells, counts = np.unique(trainer.labels.reshape(-1, 2), axis=0, return_counts=True)
            for (rival, net), count in zip(cells, counts):
                row = trainer.league.counts[trainer.league.key(rival, net)]
                self.assertEqual(row["steps"], count)
                self.assertEqual(row["tokens"], count * stats["epochs"])
            self.assertFalse(torch.equal(trainer.policy.actor.weight, before))
            self.assertEqual(trainer.learner.actor.weight.device.type, "cuda")
            trainer.checkpoint()
            restored = NextTrainer(args, [champion_row()])
            restored.resume(Path(tmp) / "latest.pt")
            for expected, actual in zip(trainer.policy.parameters(), restored.learner.parameters()):
                torch.testing.assert_close(expected, actual.cpu(), rtol=0, atol=0)
            expected_adam = trainer.optimizer.state[trainer.learner.actor.weight]
            restored_adam = restored.optimizer.state[restored.learner.actor.weight]
            for key in ("step", "exp_avg", "exp_avg_sq"):
                torch.testing.assert_close(restored_adam[key], expected_adam[key], rtol=0, atol=0)
            with Batch(args.lanes, 1, "full") as batch:
                restored.initialize(batch)
                restored.optimize(restored.collect())
            self.assertFalse(torch.equal(trainer.policy.actor.weight, restored.policy.actor.weight))


class EvaluationTests(unittest.TestCase):
    def test_same_model_pairs_and_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            model = CHAMPION.with_suffix(".json")
            cells = [dict(kind="direct", rival="direct", net=8, pairs=2),
                     dict(kind="common", rival="10", net=6, pairs=2)]
            manifest = plan(model, model, cells, 123, 6000, 32, fixed_equipment=True)
            result = execute(manifest, tmp, threads=1, adoption=False)
            self.assertEqual(json.loads((Path(tmp) / "manifest.json").read_text())["match_profile"],
                             FIXED_EQUIPMENT_PROFILE)
            self.assertTrue(all(c["difference"]["mean"] == 0 for c in result["cells"]))
            self.assertEqual(result["cells"][1]["weighted_hp"], 0)
            with self.assertRaises(ValueError):
                execute(manifest, tmp, threads=1, adoption=False)
            execute(manifest, tmp, threads=1, adoption=False, resume=True)

    def test_statistics_equal_cells_and_pair_correlation(self):
        rows = [dict(kind="common", difference=dict(mean=.1)), dict(kind="common", difference=dict(mean=.5))]
        samples = [dict(d=np.full(32, .1)), dict(d=np.full(32, .5))]
        self.assertAlmostEqual(aggregate(rows, samples, lambda r: True)["mean"], .3)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "pairs.jsonl"
            records = []
            for i in range(200):
                outcome = 1 if i < 120 else 2
                records.append(dict(seed=i, candidate=[dict(outcome=outcome, hp=10)] * 2, champion=None))
            path.write_text("\n".join(json.dumps(r) for r in records))
            cell = dict(kind="direct", pairs=200)
            row, samples = summarize_cell(path, cell, 2000, np.random.default_rng(10))
            self.assertAlmostEqual(row["difference"]["mean"], .2)
            self.assertGreater(row["pair_variance"], .9)  # not the halved independent-game variance
            self.assertGreater(row["difference"]["lower"], 0)
            for record in records:
                for match in record["candidate"]:
                    match["outcome"] = 2 if record["seed"] < 120 else 1
            path.write_text("\n".join(json.dumps(r) for r in records))
            negative, _ = summarize_cell(path, cell, 2000, np.random.default_rng(10))
            self.assertLess(negative["difference"]["upper"], 0)
            path.write_text(path.read_text() + "\n" + json.dumps(records[0]))
            with self.assertRaises(ValueError):
                summarize_cell(path, cell, 32, np.random.default_rng(1))

    def test_required_weak_cells_and_missing_hp(self):
        model = dict(group="history", path="unused", id="a", sha256="a")
        cells = make_cells([model], 2, 2, 2, 2, 2)
        rows, samples = [], []
        for c in cells:
            rows.append(dict(c, difference=dict(mean=.1, lower=.08, upper=.12), timeout=0,
                             hp_sum=.1, hp_count=1., weighted_hp=.1))
            samples.append(dict(d=np.full(32, .1), hp_sum=np.full(32, .1),
                                hp_count=np.ones(32), weighted_hp=np.full(32, .1)))
        quality = dict.fromkeys(("parity", "reset", "resume", "replay", "tests"), True)
        self.assertTrue(judge(rows, samples, quality)["accepted"])
        self.assertFalse(judge(rows, samples, quality, smoke=True)["accepted"])
        weak = next(r for r in rows if r["kind"] == "common" and r["rival"] == "10" and r["net"] == 6)
        weak["difference"]["lower"] = -.01
        self.assertFalse(judge(rows, samples, quality)["accepted"])
        for r, s in zip(rows, samples):
            r["hp_count"] = 0
            s["hp_count"][:] = 0
        self.assertFalse(judge(rows, samples, quality)["checks"]["hp"])


if __name__ == "__main__":
    torch.set_num_threads(1)
    unittest.main()
