"""24-hour GRU league campaign. --quick verifies the workflow, never adopts a bot."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import threading
from argparse import Namespace
import time

import numpy as np
import torch

from league import digest, inventory, model_id
from native import BUILD, ROOT, Batch, FIXED_EQUIPMENT_PROFILE
from nextgen_train import NextTrainer, run_stage
from paired_evaluate import dev_rank, execute, make_cells, plan
from policy import Policy, atomic_json, verify_export
from train import parser as training_parser

CHAMPION = ROOT / "build/recurrent-ppo/full/seed-61002/candidate-0007.pt"


class Campaign:
    def __init__(self, args):
        self.args = args
        self.root = args.output.resolve()
        self.root.mkdir(parents=True, exist_ok=True)
        self.state_path = self.root / "campaign.json"
        saved = None
        if self.state_path.exists():
            if not args.resume:
                raise ValueError("output exists; use --resume to continue the same frozen campaign")
            saved = json.loads(self.state_path.read_text(encoding="utf-8"))
        self.fixed_equipment = (saved is None or getattr(args, "fixed_equipment", False)
                                or saved["settings"].get("match_profile") == FIXED_EQUIPMENT_PROFILE)
        settings = {k: str(v.resolve()) if isinstance(v, Path) else v for k, v in vars(args).items()
                    if k not in ("resume", "fixed_equipment")}
        if self.fixed_equipment:
            settings.update(match_profile=FIXED_EQUIPMENT_PROFILE, unbreakable=True)
        if saved is not None:
            self.state = saved
            if self.state["settings"] != settings:
                raise ValueError("campaign settings changed on resume")
        else:
            if args.resume or any(self.root.iterdir()):
                raise ValueError("new experiment requires an empty directory")
            self.state = dict(settings=settings, elapsed=0., stages={}, candidates=[], shared=[],
                              quality={}, status="running")
        self.start = time.monotonic()
        self.previous = self.state["elapsed"]
        budget_path = self.root / "budget.json"
        if budget_path.exists():
            ledger = json.loads(budget_path.read_text(encoding="utf-8"))
            # After a hard crash charge the entire unsaved heartbeat interval.
            self.previous = max(self.previous, ledger["elapsed"] + (0 if ledger["clean"] else 5))
        self.budget = 900 if args.quick else 24 * 3600
        self.deadline = self.start + self.budget - self.previous
        self.champion = args.champion.with_suffix(".json").resolve()
        self.stage_deadline = self.deadline
        manifest = self.root / "opponents.json"
        if manifest.exists():
            self.models = json.loads(manifest.read_text(encoding="utf-8"))["models"]
        else:
            self.models = inventory(self.champion, args.history, manifest)
        for m in self.models:
            if digest(m["path"]) != m["sha256"]:
                raise ValueError("opponent inventory changed")
        checkpoint_hash = digest(args.champion)
        if self.state.get("initial_checkpoint_sha256", checkpoint_hash) != checkpoint_hash:
            raise ValueError("initial checkpoint changed")
        self.state["initial_checkpoint_sha256"] = checkpoint_hash
        self.save()
        self.stop_heartbeat = threading.Event()
        self.heartbeat = threading.Thread(target=self.track_budget, daemon=True)
        self.heartbeat.start()

    def track_budget(self):
        while not self.stop_heartbeat.is_set():
            atomic_json(self.root / "budget.json", dict(elapsed=self.previous + time.monotonic() - self.start, clean=False))
            self.stop_heartbeat.wait(5)

    def close(self):
        self.stop_heartbeat.set()
        self.heartbeat.join()
        self.save()
        atomic_json(self.root / "budget.json", dict(elapsed=self.state["elapsed"], clean=True))

    def save(self):
        self.state["elapsed"] = self.previous + time.monotonic() - self.start
        atomic_json(self.state_path, self.state)

    def dev_cells(self):
        history = [m for m in self.models if m["group"] == "history"]
        ids = np.linspace(0, len(history) - 1, min(8, len(history)), dtype=int)
        models = [history[i] for i in ids]
        cells = make_cells(models, *([1] * 5 if self.args.quick else [32, 16, 64, 32, 8]))
        if self.args.quick:
            cells = [c for c in cells if c["kind"] == "direct" or
                     (c["kind"] == "common" and c["rival"] in ("10", "1", "11", "8", "15", "16", "17") and c["net"] in (0, 5, 6)) or
                     (c["kind"] == "history" and c["rival"] in [m["path"] for m in models[:3]])]
        return cells

    def evaluate_dev(self, model, label, confirmation=False, deadline=None):
        output = self.root / "development" / label
        seed = 0x434F4E4649524D if confirmation else 0x444556454C4F50
        manifest = plan(model, self.champion, self.dev_cells(), seed, 80 if self.args.quick else 6000,
                        resamples=32 if self.args.quick else 500, smoke=self.args.quick,
                        fixed_equipment=self.fixed_equipment)
        # A completed dev cell can be resumed, but its model hash must still match.
        report = execute(manifest, output, self.args.threads, deadline=deadline or self.stage_deadline,
                         resume=(output / "manifest.json").exists(), adoption=False)
        report["rank"] = dev_rank(report)
        return report

    def refresh_shared(self, trainer):
        for m in self.state["shared"]:
            slot = trainer.league.add(m)
            trainer.league.models[slot]["active"] = m.get("active", True)
            trainer.batch.snapshot(slot, Path(m["path"]))

    def callback(self, path, trainer):
        if self.stage_deadline - time.monotonic() < (1 if self.args.quick else 30):
            return
        label = f"{path.parent.name}-{path.stem}"
        existing = next((r for r in self.state["candidates"] if r["label"] == label), None)
        if existing:
            self.refresh_shared(trainer)
            return
        report = self.evaluate_dev(path, label)
        row = dict(label=label, model=str(path.resolve()), checkpoint=str(path.with_suffix(".pt").resolve()),
                   rank=report["rank"], seconds=report["seconds"],
                   vector=[r["candidate_win"] for r in report["cells"]],
                   steps=trainer.total_steps, elapsed=trainer.elapsed,
                   eligible=not trainer.args.exploit and path.parent.name.startswith(("main-", "finish-")))
        self.state["candidates"].append(row)
        if not trainer.args.exploit:
            key = model_id(path)
            if not any(m["id"] == key for m in self.state["shared"] + self.models):
                self.state["shared"].append(dict(group="new", path=str(path.resolve()), id=key,
                                                  sha256=digest(path), vector=row["vector"], active=True))
            self.prune_shared()
            self.refresh_shared(trainer)
        self.save()
        print(json.dumps(dict(event="development", label=label, rank=row["rank"])), flush=True)

    def prune_shared(self):
        rows = [m for m in self.state["shared"] if m["group"] == "new"]
        selected = rows[-8:]
        older = rows[:-8]
        # Farthest-first on fixed dev matchup win rates keeps distinct older policies.
        while older and len(selected) < 16:
            best = max(older, key=lambda m: min(np.mean((np.array(m["vector"]) - s["vector"]) ** 2) for s in selected))
            selected.append(best)
            older.remove(best)
        for row in rows:
            row["active"] = row in selected

    def stage(self, label, seconds, init=None, seed=71001, sequence=32, burn_in=16,
              focus=.3, learning_rate=1e-4, exploit=False, target=None, cumulative=None):
        key = f"{label}:{cumulative or seconds}"
        if key in self.state["stages"]:
            if self.state["stages"][key].get("evaluation_budget_exhausted"):
                self.recover_evaluation(label, key)
            return self.state["stages"][key]
        remaining = self.deadline - time.monotonic()
        reserve = self.state.get("final_reserve", 60 if self.args.quick else 1800)
        if remaining < reserve + (2 if self.args.quick else 60):
            raise TimeoutError("insufficient budget for another training stage")
        output = self.root / label
        latest = output / "latest.pt"
        args = training_parser().parse_args([str(output)])
        args.seed, args.lanes = seed, self.args.lanes
        args.threads, args.torch_threads = self.args.threads, self.args.torch_threads
        args.rollout = 16 if self.args.quick else 256
        args.sequence, args.burn_in = (8 if self.args.quick else sequence), burn_in
        args.max_steps = 80 if self.args.quick else 6000
        args.fixed_equipment = self.fixed_equipment
        args.learning_rate, args.focus, args.exploit = learning_rate, focus, exploit
        args.target = target
        args.hp_beta, args.epochs, args.target_kl = .1, 4, .015
        args.ppo_device = getattr(self.args, "ppo_device", "cpu")
        args.minibatch_tokens = getattr(self.args, "minibatch_tokens", 2048)
        args.init_checkpoint = Path(init or self.args.champion)
        args.resume = latest if latest.exists() else None
        args.selection_interval = 1800
        args.seconds = cumulative or seconds
        old_elapsed = torch.load(latest, map_location="cpu", weights_only=False)["elapsed"] if latest.exists() else 0
        args.updates = None
        allowed = min(args.seconds - old_elapsed, remaining - reserve - 10)
        self.stage_deadline = min(self.deadline - reserve - 10, time.monotonic() + max(0, allowed))
        print(f"[{label}] cumulative budget {args.seconds:.1f}s, remaining stage {allowed:.1f}s", flush=True)
        try:
            # Leave time inside this stage for its final export and development evaluation.
            evaluation_margin = min(allowed * .6, 10 if self.args.quick else
                                    max(60, self.state.get("baseline", {}).get("seconds", 60) * 2 + 30))
            report = run_stage(args, self.models + self.state["shared"], self.callback,
                               self.stage_deadline - evaluation_margin)
        except subprocess.TimeoutExpired:
            # The last checkpoint is preserved; a partial dev cell is never ranked.
            report = dict(checkpoint=str(latest.resolve()), model=str((output / "last.json").resolve()),
                          elapsed=args.seconds, evaluation_budget_exhausted=True)
        self.state["stages"][key] = report
        self.save()
        return report

    def recover_evaluation(self, label, key):
        """Finish the frozen development candidate without repeating its training."""
        output = self.root / label
        paths = sorted(output.glob("candidate-*.json"))
        if not paths:
            raise RuntimeError(f"no saved development candidate in {output}")
        path = paths[-1]
        args = Namespace(**json.loads((output / "settings.json").read_text(encoding="utf-8")))
        args.output = output
        torch.set_num_threads(args.torch_threads)
        trainer = NextTrainer(args, self.models + self.state["shared"])
        trainer.resume(output / "latest.pt")
        reserve = self.state.get("final_reserve", 1800)
        self.stage_deadline = min(self.deadline - reserve,
                                  time.monotonic() + max(300, self.state["baseline"]["seconds"] * 3))
        print(f"[{label}] resuming saved development evaluation; no training", flush=True)
        with Batch(args.lanes, args.threads, "full", fixed_equipment=self.fixed_equipment) as batch:
            trainer.initialize(batch)
            self.callback(path, trainer)
        expected = f"{path.parent.name}-{path.stem}"
        if not any(r["label"] == expected for r in self.state["candidates"]):
            raise TimeoutError("insufficient budget to recover development evaluation")
        report = self.state["stages"][key]
        report.pop("evaluation_budget_exhausted")
        report.update(elapsed=trainer.elapsed, steps=trainer.total_steps,
                      steps_per_second=trainer.total_steps / max(trainer.elapsed, 1e-6))
        self.save()

    def best(self, confirm=True):
        pool = [r for r in self.state["candidates"] if r["eligible"]]
        if not pool:
            raise RuntimeError("no main-training candidate completed development evaluation")
        pool = sorted(pool, key=lambda r: r["rank"], reverse=True)[:3]
        if confirm:
            self.stage_deadline = min(self.deadline, time.monotonic() + (30 if self.args.quick else 300))
            for row in pool:
                if "confirmation" not in row:
                    row["confirmation"] = self.evaluate_dev(row["model"], row["label"] + "-confirm", True)["rank"]
                    self.save()
            # Require strength to persist on the second, independent development suite.
            return max(pool, key=lambda r: min(r["rank"], r["confirmation"]))
        return pool[0]

    def exploiter(self, label, target_model):
        if label in self.state.get("exploiters", {}):
            return
        target_key = model_id(target_model)
        models = self.models + self.state["shared"]
        target_index = next((i for i, m in enumerate(models) if m["id"] == target_key), None)
        if target_index is None:
            raise ValueError("exploiter target must be a registered frozen candidate")
        # Probe champion and three evenly spaced old snapshots; choose the best target score.
        history = [m for m in self.models if m["group"] == "history"]
        seeds = [self.models[0]] + [history[i] for i in np.linspace(0, len(history) - 1, min(3, len(history)), dtype=int)]
        ranked = []
        for i, m in enumerate(seeds):
            pt = Path(m["path"]).with_suffix(".pt")
            cells = [dict(kind="direct", rival="direct", net=n, pairs=1 if self.args.quick else 32) for n in (0, 5, 6)]
            out = self.root / label / f"initialization-{i}"
            p = plan(m["path"], target_model, cells, 0x455850494E4954, 80 if self.args.quick else 6000, 32,
                     fixed_equipment=self.fixed_equipment)
            result = execute(p, out, self.args.threads, deadline=self.deadline,
                             resume=(out / "manifest.json").exists(), adoption=False)
            score = np.mean([r["candidate_win"] + .5 * r["timeout"] for r in result["cells"]])
            ranked.append((score, m, pt))
        _, initial_model, init = max(ranked, key=lambda x: x[0])
        if not init.exists():
            # Old exports have no critic. Transfer their policy and normalization while
            # retaining the champion critic as an explicitly recorded warm-start estimate.
            state = torch.load(self.args.champion, map_location="cpu", weights_only=False)
            data = json.loads(Path(initial_model["path"]).read_text(encoding="utf-8"))
            weights = state["policy"]
            for layer, key in (("encoder", "encoder"), ("actor", "actor")):
                for field in ("weight", "bias"):
                    weights[f"{layer}.{field}"] = torch.tensor(data[key][field])
            for kind, key in (("ih", "input_gate"), ("hh", "hidden_gate")):
                for field in ("weight", "bias"):
                    weights[f"core.{field}_{kind}_l0"] = torch.tensor(data[key][field])
            for key in ("mean", "scale"):
                weights[key] = torch.tensor(data[key])
            init = self.root / label / "initial-policy.pt"
            init.parent.mkdir(parents=True, exist_ok=True)
            torch.save(state, init)
            atomic_json(init.with_suffix(".source.json"), dict(policy=initial_model, critic=str(self.args.champion)))
        report = self.stage(label, 30 if self.args.quick else 3600, init=init, seed=72001 if label.endswith("initial") else 72002,
                            exploit=True, target=18 + target_index)
        cells = [dict(kind="direct", rival="direct", net=n, pairs=1 if self.args.quick else 64) for n in range(13)]
        out = self.root / label / "qualification"
        p = plan(report["model"], target_model, cells, 0x4558505155414C, 80 if self.args.quick else 6000, 32,
                 fixed_equipment=self.fixed_equipment)
        result = execute(p, out, self.args.threads, deadline=self.deadline,
                         resume=(out / "manifest.json").exists(), adoption=False)
        # Match exploiter's network mix: 25% each high-ping plus 50% original distribution.
        weights = [.25] + [1 / 48] * 12
        weights[5] += .25
        weights[6] += .25
        score = sum(w * (r["candidate_win"] + .5 * r["timeout"]) for w, r in zip(weights, result["cells"]))
        timeouts = sum(w * r["timeout"] for w, r in zip(weights, result["cells"]))
        active = score >= .4 and timeouts < .01
        m = dict(group="exploiter", path=report["model"], id=model_id(report["model"]),
                 sha256=digest(report["model"]), active=active)
        if not any(x["id"] == m["id"] for x in self.models + self.state["shared"]):
            self.state["shared"].append(m)
        self.state.setdefault("exploiters", {})[label] = dict(score=score, timeout=timeouts, active=active)
        self.save()

    def run(self):
        if self.state["status"] == "complete":
            print(f"Already complete: {self.root / 'final/summary.json'}")
            return
        # Run regression checks in a child process with a bounded budget.
        if not self.state["quality"].get("tests"):
            subprocess.run([os.sys.executable, str(Path(__file__).with_name("test_nextgen.py"))],
                           check=True, timeout=120, env=dict(os.environ, PYTHONUNBUFFERED="1"))
            self.state["quality"].update(tests=True, reset=True, resume=True)
            self.save()
        initial_export = dict(checkpoint_sha256=self.state["initial_checkpoint_sha256"],
                              model_sha256=digest(self.champion))
        if self.state.get("verified_initial_export") != initial_export:
            state = torch.load(self.args.champion, map_location="cpu", weights_only=False)
            policy = Policy("full", False)
            policy.load_state_dict(state["policy"])
            with Batch(1, 1, "full") as batch:
                verify_export(policy, self.champion, batch, np.random.default_rng(113))
            self.state["verified_initial_export"] = initial_export
            self.save()
        if "baseline" not in self.state:
            self.stage_deadline = min(self.deadline, time.monotonic() + (120 if self.args.quick else 1800))
            self.state["baseline"] = self.evaluate_dev(self.champion, "baseline")
            self.stage("speed", 10 if self.args.quick else 30, burn_in=0)
            self.save()
        if "memory" not in self.state:
            pilot = []
            for name, seq, burn in (("A", 32, 0), ("B", 32, 16), ("C", 64, 16)):
                result = self.stage(f"memory-{name}", 30 if self.args.quick else 1200, sequence=seq, burn_in=burn)
                candidates = [r for r in self.state["candidates"] if r["label"].startswith(f"memory-{name}-")]
                if candidates:
                    pilot.append(dict(name=name, sequence=seq, burn_in=burn,
                                      rank=max(candidates, key=lambda r: r["rank"])["rank"],
                                      throughput=result.get("steps_per_second", 0)))
            if not pilot:
                raise RuntimeError("memory pilots did not complete evaluation")
            # A one-point dev score difference is treated as noise in the short pilot.
            top = max(pilot, key=lambda r: r["rank"])
            tied = [r for r in pilot if r["rank"][0] == top["rank"][0] and r["rank"][1] >= top["rank"][1] - .01]
            self.state["memory"] = max(tied, key=lambda r: r["throughput"])
            atomic_json(self.root / "memory-comparison.json", pilot)
            self.save()
        if "final_cells" not in self.state:
            cells = make_cells(self.models, *([1] * 5 if self.args.quick else [8192, 512, 8192, 2048, 64]))
            # Freeze counts before main training, using observed pilot pair variances.
            variance = {}
            for row in self.state["candidates"]:
                if row["label"].startswith("memory-"):
                    summary = json.loads((self.root / "development" / row["label"] / "summary.json").read_text())
                    for c in summary["cells"]:
                        key = (c["kind"], c["rival"], c["net"])
                        variance[key] = max(variance.get(key, 0), c["pair_variance"])
            if not self.args.quick:
                for cell in cells:
                    if cell["kind"] == "common" and cell["net"] == 6 and int(cell["rival"]) in (10, 1, 11, 8):
                        v = variance.get((cell["kind"], cell["rival"], cell["net"]), 1.)
                        cell["pairs"] = max(cell["pairs"], int(np.ceil(10.51 * v / .03**2)))
                def estimated_variance(c):
                    # Pilot variance is noisy: retain a floor and shrink toward .5.
                    v = variance.get((c["kind"], c["rival"], c["net"]))
                    n = 64 if c["net"] == 6 and c["rival"] in ("10", "1", "11", "8") else 16
                    return max(.1, (n * v + 8 * .5) / (n + 8)) if v is not None else .5
                common = lambda c: c["kind"] == "common"
                groups = {
                    "common": common,
                    "normal": lambda c: common(c) and c["net"] not in (5, 6),
                    "high": lambda c: common(c) and c["net"] in (5, 6),
                    "weak120": lambda c: common(c) and c["net"] == 5 and c["rival"] in ("10", "1", "11", "8"),
                    "history": lambda c: c["kind"] == "history",
                }
                power_design = {}
                for name, predicate in groups.items():
                    subset = [c for c in cells if predicate(c)]
                    se2 = sum(estimated_variance(c) / c["pairs"] for c in subset) / len(subset)**2
                    factor = max(1., 10.51 * se2 / .01**2)
                    for c in subset:
                        c["pairs"] = int(np.ceil(c["pairs"] * factor))
                    power_design[name] = dict(scale=factor, design_margin=.01,
                                             target_power=.9, assumption="pilot variance shrunk toward .5, floor .1")
                self.state["power_design"] = power_design
            baseline = self.state["baseline"]
            games = sum(c["pairs"] * (2 if c["kind"] == "direct" else 4) for c in cells)
            estimate = baseline["seconds"] / baseline["games"] * games
            self.state.update(final_cells=cells, final_reserve=60 if self.args.quick else max(1800, estimate * 1.5),
                              evaluation_design=dict(games=games, estimated_seconds=estimate,
                                                     variance_cells=len(variance), frozen_before_main=True,
                                                     cells=cells, power_design=self.state.get("power_design", {})))
            atomic_json(self.root / "evaluation-design.json", self.state["evaluation_design"])
            self.save()
        self.exploiter("exploiter-initial", self.champion)
        memory = self.state["memory"]
        chunk = 30 if self.args.quick else 7200
        for block in range(1, 5):
            for seed in (71001, 71002):
                self.stage(f"main-{seed}", chunk, seed=seed, cumulative=block * chunk,
                           sequence=memory["sequence"], burn_in=memory["burn_in"])
            if block == 2:
                self.exploiter("exploiter-middle", self.best()["model"])
        if "finish_parent" not in self.state:
            self.state["finish_parent"] = self.best()
            self.save()
        for name, focus in (("balanced", .3), ("high-ping", .5)):
            remaining = self.deadline - time.monotonic() - self.state["final_reserve"] - 30
            duration = min(30 if self.args.quick else 5400, max(0, remaining / (2 if name == "balanced" else 1)))
            # Stable stage budget across resume; finishing is the first stage shortened for evaluation.
            duration = self.state.setdefault("finish_budgets", {}).setdefault(name, duration)
            self.save()
            if duration > (1 if self.args.quick else 60):
                self.stage(f"finish-{name}", duration, init=self.state["finish_parent"]["checkpoint"],
                           seed=73001 if name == "balanced" else 73002, sequence=memory["sequence"],
                           burn_in=memory["burn_in"], focus=focus, learning_rate=5e-5)
        if "selected" not in self.state:
            self.state["selected"] = self.best()
            self.save()
        selected = self.state["selected"]
        model = Path(selected["model"])
        state = torch.load(selected["checkpoint"], map_location="cpu", weights_only=False)
        policy = Policy("full", False)
        policy.load_state_dict(state["policy"])
        with Batch(1, 1, "full") as batch:
            verify_export(policy, model, batch, np.random.default_rng(134))
        self.state["quality"]["parity"] = True
        equipment_args = [] if self.fixed_equipment else ["--legacy-durability"]
        for net in (0, 5, 6, 11):
            replay = self.root / f"replay-{net}.json"
            subprocess.run([str(BUILD / "pvp-rl-eval.exe"), "duel", str(model), str(net), "17", "99113",
                            str(replay), str(80 if self.args.quick else 6000), *equipment_args], check=True,
                           timeout=max(1, self.deadline - time.monotonic()), stdout=subprocess.DEVNULL)
            # Parse the serialized replay as well as completing a Rust duel.
            json.loads(replay.read_text(encoding="utf-8"))
        self.state["quality"]["replay"] = True
        self.save()
        manifest = plan(model, self.champion, self.state["final_cells"], 0x46494E414C3234,
                        80 if self.args.quick else 6000, 32 if self.args.quick else 2000,
                        quality=self.state["quality"], smoke=self.args.quick, fixed_equipment=self.fixed_equipment)
        report = execute(manifest, self.root / "final", self.args.threads, deadline=self.deadline,
                         resume=(self.root / "final/manifest.json").exists())
        self.state["status"] = "complete"
        self.save()
        print(f"Accepted: {report['accepted']}  Results: {self.root / 'final/summary.json'}", flush=True)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=Path)
    p.add_argument("--champion", "--init-checkpoint", dest="champion", type=Path, default=CHAMPION,
                   help="trusted initial champion .pt; matching .json required (optimizer starts fresh)")
    p.add_argument("--history", type=Path, default=CHAMPION.parent)
    p.add_argument("--lanes", type=int, default=512)
    p.add_argument("--threads", type=int, default=8)
    p.add_argument("--torch-threads", type=int, default=4)
    # Omit unspecified options from the frozen settings of existing campaigns.
    p.add_argument("--ppo-device", choices=("cpu", "cuda"), default=argparse.SUPPRESS,
                   help="device for PPO updates (CUDA or ROCm PyTorch uses 'cuda')")
    p.add_argument("--minibatch-tokens", type=int, default=argparse.SUPPRESS,
                   help="token budget per optimizer step (default 2048)")
    p.add_argument("--quick", action="store_true")
    p.add_argument("--resume", action="store_true")
    p.add_argument("--fixed-equipment", action="store_true",
                   help=f"Require {FIXED_EQUIPMENT_PROFILE} when resuming (default for new runs)")
    args = p.parse_args()
    if getattr(args, "minibatch_tokens", 2048) < 1:
        p.error("minibatch token budget must be positive")
    if getattr(args, "ppo_device", "cpu") == "cuda" and not torch.cuda.is_available():
        p.error("--ppo-device cuda requires a GPU-enabled PyTorch environment with an available device")
    if not 1 <= args.lanes <= 4096 or not 1 <= args.threads <= 64 or args.torch_threads <= 0:
        p.error("invalid environment or thread count")
    campaign = Campaign(args)
    try:
        campaign.run()
    except (TimeoutError, subprocess.TimeoutExpired):
        campaign.state["status"] = "inconclusive_budget"
        atomic_json(campaign.root / "inconclusive.json", dict(accepted=False, reason="wall-clock budget exhausted"))
        print("Budget exhausted; no bot adopted. Checkpoints and partial results are preserved.", flush=True)
    finally:
        campaign.close()


if __name__ == "__main__":
    main()
