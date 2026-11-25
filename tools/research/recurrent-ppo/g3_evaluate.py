"""Select on development seeds and evaluate one frozen G3 hash on swapped seats."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

import numpy as np
import torch

from g3_budget import remaining
from g3_train import BASELINES, G3Policy, HOLDOUT
from league import digest
from native import BUILD, Batch
from paired_evaluate import NETWORK_NAMES, split_seed
from policy import atomic_json, verify_export


METRICS = ("win", "win_hp", "end_hp", "even_fraction", "clean_fraction", "opening_diversity",
           "timeout", "repeated_even_fraction", "longest_even_streak")
COMMON = ("1", "10", "17", *map(str, HOLDOUT), str(BASELINES[-1]))
FINAL_SEED = 938765123
DEV_SEED = 834011


def values(game):
    win = game["outcome"] == 1
    exchange = game["exchanges"]
    counts = exchange["profitable_openings"]
    total = sum(counts)
    # Probability that two different profitable windows used different movement categories.
    diversity = (1 - sum(c * (c - 1) for c in counts) / (total * (total - 1))) if total > 1 else 0.
    windows = max(1, exchange["windows"])
    return np.array([win, game["hp"] if win else 0., game["hp"], exchange["even"] / windows,
                     exchange["clean"] / windows, diversity, game["outcome"] == 3,
                     exchange["repeated_even"] / windows, exchange["longest_even_streak"]], dtype=float)


def run_cell(root, model, baseline, rival, net, pairs, base_seed, until, threads):
    name = Path(rival).stem
    path = root / f"{baseline.stem}-{name}-{net:02}.jsonl"
    if not path.exists():
        seed = split_seed(base_seed, f"{name}/{net}")  # common random numbers across all policies
        partial = path.with_suffix(".partial")
        with partial.open("wb") as out:
            subprocess.run([str(BUILD / "pvp-rl-eval.exe"), "paired", str(model), str(baseline),
                            rival, str(net), str(seed), str(pairs), "6000", "--fixed-equipment"],
                           check=True, stdout=out, env=dict(os.environ, RAYON_NUM_THREADS=str(threads)),
                           timeout=max(.01, remaining(until)))
        partial.replace(path)
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    if len(rows) != pairs or len({r["seed"] for r in rows}) != pairs:
        raise ValueError(f"incomplete seat pairs in {path}")
    if any(r["seats"] != [0, 1] for r in rows):
        raise ValueError(f"invalid seat order in {path}")
    return rows


def manifest(root, settings):
    root.mkdir(parents=True, exist_ok=True)
    path = root / "manifest.json"
    if path.exists() and json.loads(path.read_text()) != settings:
        raise ValueError(f"frozen evaluation changed in {root}")
    atomic_json(path, settings)


def development(model, root, until, threads, pairs):
    manifest(root, dict(candidate=digest(model), baselines=[digest(p) for p in BASELINES],
                        seed=DEV_SEED, pairs=pairs, networks=[0, 6, 9], common=["1", "10", "17"]))
    rows = []
    for net in (0, 6, 9):
        for baseline in BASELINES:
            for trial in run_cell(root, model, baseline, "direct", net, pairs, DEV_SEED, until, threads):
                rows.extend(values(g) for g in trial["candidate"])
        for rival in ("1", "10", "17"):
            for trial in run_cell(root, model, BASELINES[-1], rival, net, pairs, DEV_SEED, until, threads):
                rows.extend(values(g) for g in trial["candidate"])
    means = np.mean(rows, axis=0)
    score = means[0] + .35 * means[1] / 20 - .12 * means[3] + .05 * means[5] - means[6]
    print(f"Development {model.name}  score={score:.4f}  win={means[0]:.2%}", flush=True)
    return dict(path=str(model.resolve()), sha256=digest(model), score=float(score),
                metrics=dict(zip(METRICS, means.tolist())))


def ci(point, samples):
    lo, hi = np.quantile(samples, [.025, .975], axis=0)
    return dict(mean=float(point), lower=float(lo), upper=float(hi))


def write_report(root, result):
    def estimate(stat, factor=1):
        if stat is None:
            return "Unavailable"
        return f"{stat['mean'] * factor:.2f} [{stat['lower'] * factor:.2f}, {stat['upper'] * factor:.2f}]"

    lines = ["# G3 simulator evaluation", "", f"Status **{result['status']}**. "
             f"All performance gates passed **{result['accepted']}**.", "",
             f"Selected weights `{result['candidate']}`", "", f"SHA-256 `{result['sha256']}`", "",
             "Estimates include marginal 95% paired-bootstrap intervals. Gains compare G3 with the named control.",
             "", "| Metric | PPO | G2 | G2+ |", "| --- | ---: | ---: | ---: |"]
    reports = [result["results"][p.stem] for p in BASELINES]
    stats = [("Direct win rate (%)", [r["direct_win"] for r in reports], 100),
             ("HP gain in mutual wins", [r["joint_win_hp"] for r in reports], 1),
             ("Net HP gain after a style switch (per 12 decisions)",
              [r["post_switch_net_hp_per_12_decisions"] for r in reports], 1)]
    for key, title, factor in [
        ("win", "Common-opponent win gain (percentage points)", 100),
        ("win_hp", "Win-weighted HP gain", 1),
        ("even_fraction", "Even-exchange change (percentage points)", 100),
        ("repeated_even_fraction", "Repeated even-exchange change (percentage points)", 100),
        ("clean_fraction", "Clean-exchange change (percentage points)", 100),
        ("opening_diversity", "Profitable opening diversity change", 1),
    ]:
        stats.append((title, [r["common_difference"][key] for r in reports], factor))
    for title, cells, factor in stats:
        lines.append("| " + " | ".join([title, *(estimate(c, factor) for c in cells)]) + " |")
    lines.extend(["", "| Control | Failed criteria |", "| --- | --- |"])
    for path, report in zip(BASELINES, reports):
        failed = ", ".join(k for k, passed in report["checks"].items() if not passed)
        lines.append(f"| {path.stem} | {failed or 'None'} |")
    lines.extend(["", "The switching result measures performance against the held-out simulated opponent. "
                  "The per-network results and timeout rates are in `summary.json`."])
    tmp = root / "report.md.tmp"
    tmp.write_text("\n".join(lines) + "\n", encoding="utf-8")
    tmp.replace(root / "report.md")


def verify_selected(model, output, until):
    checkpoint = model.parent / "latest.pt" if model.name == "last.json" else model.with_suffix(".pt")
    state = torch.load(checkpoint, weights_only=False, map_location="cpu")
    policy = G3Policy("g3", False)
    policy.load_state_dict(state["policy"])
    with Batch(1, 1, "g3") as probe:
        verify_export(policy, model, probe, np.random.default_rng(80912))
    results = []
    for net in (0, 6):
        replay = output / f"replay-{net}.json"
        proc = subprocess.run([str(BUILD / "pvp-rl-eval.exe"), "duel", str(model),
                               str(net), "16", "114691", str(replay), "6000", "--fixed-equipment"],
                              check=True, capture_output=True, timeout=max(.01, remaining(until)))
        results.append(json.loads(proc.stdout))
    atomic_json(output / "verification.json", dict(policy=str(model.resolve()), sha256=digest(model),
                checkpoint=str(checkpoint.resolve()), checkpoint_sha256=digest(checkpoint),
                update=state["update"], steps=state["total_steps"], torch_rust_sequence=64, replays=results))


def final(model, root, until, threads, pairs, smoke):
    seed = 914887 if smoke else FINAL_SEED
    manifest(root, dict(candidate=digest(model), baselines=[digest(p) for p in BASELINES], seed=seed,
                        pairs=pairs, networks=list(NETWORK_NAMES), rivals=list(COMMON), smoke=smoke,
                        equipment="sword_pvp_fixed_equipment_v1", rules="paper_flat_v2",
                        thresholds=dict(direct_win_lower=.50, common_win_gain_lower=.0, win_hp_gain_lower=.0,
                                        even_gain_upper=.0, repeated_even_gain_upper=.0,
                                        clean_gain_lower=.0, diversity_gain_lower=.0,
                                        switch_damage_gain_lower=.0, switch_win_margin=-.01,
                                        worst_network_gain=-.02, timeout=.01)))
    report = {}
    for baseline in BASELINES:
        rng = np.random.default_rng(seed)
        metrics, switch, response, direct, joint_hp, cells = [], [], [], [], [], []
        for net in range(len(NETWORK_NAMES)):
            print(f"Evaluating {baseline.stem} on {NETWORK_NAMES[net]} ({net + 1}/{len(NETWORK_NAMES)})", flush=True)
            for rival in ("direct", *COMMON):
                rows = run_cell(root, model, baseline, rival, net, pairs, seed, until, threads)
                actor = np.array([[values(g) for g in r["candidate"]] for r in rows])
                a = actor.mean(1)
                # Resample complete seat pairs, preserving candidate/control covariance.
                ix = rng.integers(pairs, size=(500, pairs))
                cell = dict(network=NETWORK_NAMES[net], rival=Path(rival).stem,
                            candidate=dict(zip(METRICS, a.mean(0).tolist())))
                if rival == "direct":
                    direct.append(np.r_[a[:, 0].mean(), a[ix, 0].mean(1)])
                else:
                    control = np.array([[values(g) for g in r["champion"]] for r in rows])
                    b = control.mean(1)
                    both = (actor[:, :, 0] * control[:, :, 0])
                    joint = np.stack(((both * (actor[:, :, 1] - control[:, :, 1])).mean(1),
                                      both.mean(1)), axis=1)
                    joint_hp.append(np.vstack((joint.mean(0), joint[ix].mean(1))))
                    delta = a - b
                    boot = np.vstack((delta.mean(0), delta[ix].mean(1)))
                    metrics.append(boot)
                    if rival == "16":
                        switch.append(boot)
                        exposure = np.array([
                            [sum(g["switch_response"][key] for g in r[side]) for side in ("candidate", "champion")
                             for key in ("net_damage", "decisions")] for r in rows])
                        response.append(np.vstack((exposure.mean(0), exposure[ix].mean(1))))
                    cell.update(baseline=dict(zip(METRICS, b.mean(0).tolist())),
                                difference=dict(zip(METRICS, delta.mean(0).tolist())))
                cells.append(cell)
        d, m, s = np.mean(direct, axis=0), np.mean(metrics, axis=0), np.mean(switch, axis=0)
        joint = np.mean(joint_hp, axis=0)
        joint_ci = None
        if (joint[:, 1] > 0).all():
            ratios = joint[:, 0] / joint[:, 1]
            joint_ci = ci(ratios[0], ratios[1:])
        post = np.mean(response, axis=0)
        post_ci = None
        if (post[:, [1, 3]] > 0).all():
            diff = 12 * (post[:, 0] / post[:, 1] - post[:, 2] / post[:, 3])
            post_ci = ci(diff[0], diff[1:])
        stats = {key: ci(m[0, i], m[1:, i]) for i, key in enumerate(METRICS)}
        summary = dict(direct_win=ci(d[0], d[1:]), common_difference=stats, joint_win_hp=joint_ci,
                       switching_win_gain=ci(s[0, 0], s[1:, 0]), post_switch_net_hp_per_12_decisions=post_ci,
                       worst_network_win_gain=min(float(np.mean([c["difference"]["win"] for c in cells
                                                  if "difference" in c and c["network"] == net])) for net in NETWORK_NAMES),
                       timeout=float(np.mean([c["candidate"]["timeout"] for c in cells])))
        checks = dict(direct=summary["direct_win"]["lower"] > .5, common=stats["win"]["lower"] > 0,
                      hp=stats["win_hp"]["lower"] > 0 and joint_ci is not None and joint_ci["lower"] > 0,
                      trade=(stats["repeated_even_fraction"]["upper"] < 0 and stats["even_fraction"]["upper"] < 0
                             and stats["clean_fraction"]["lower"] > 0),
                      diversity=stats["opening_diversity"]["lower"] > 0,
                      switch=post_ci is not None and post_ci["lower"] > 0 and summary["switching_win_gain"]["lower"] >= -.01,
                      stable=summary["worst_network_win_gain"] >= -.02, timeout=summary["timeout"] < .01)
        report[baseline.stem] = dict(**summary, checks=checks, cells=cells)
    passed = all(all(r["checks"].values()) for r in report.values())
    result = dict(candidate=str(model.resolve()), sha256=digest(model),
                  status="smoke_only" if smoke else "complete", accepted=passed and not smoke, results=report)
    write_report(root, result)
    atomic_json(root / "summary.json", result)
    print(f"Evaluation complete. Report saved to {(root / 'report.md').resolve()}", flush=True)


def main(argv=None, active_limit=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("training", type=Path)
    p.add_argument("output", type=Path)
    p.add_argument("--until", type=float, required=True)
    p.add_argument("--pairs", type=int, default=128)
    p.add_argument("--dev-pairs", type=int, default=12)
    p.add_argument("--threads", type=int, default=8)
    p.add_argument("--smoke", action="store_true")
    p.add_argument("--wait-for-training", action="store_true",
                   help="start evaluation when this run publishes its verified export")
    args = p.parse_args(argv)
    if active_limit is not None:
        args.until = active_limit
    elif (args.training.parent / "migrate-active-budget").exists():
        from g3_budget import run
        return run(args.training.parent, 24, args.smoke, build=False)
    if min(args.pairs, args.dev_pairs, args.threads) <= 0:
        p.error("counts must be positive")
    args.output.mkdir(parents=True, exist_ok=True)
    # Selection is frozen before accessing any final result, including after restart.
    selected = args.output / "selection.json"
    try:
        if args.wait_for_training:
            summary = args.training / "training-summary.json"
            while not summary.exists():
                if remaining(args.until) <= 0:
                    raise subprocess.TimeoutExpired("training completion", 0)
                time.sleep(min(30, max(.01, remaining(args.until))))
            if json.loads(summary.read_text())["sha256"] != digest(args.training / "last.json"):
                raise ValueError("completed export does not match training summary")
        if not selected.exists():
            paths = sorted(args.training.glob("candidate-*.json"))
            paths = [p for p in paths if not p.name.endswith(".source.json")]
            # Sample up to eight checkpoints uniformly across the run, plus the final policy.
            indices = np.linspace(0, len(paths) - 1, min(8, len(paths)), dtype=int)
            candidates = [paths[i] for i in indices] + [args.training / "last.json"]
            ranked = [development(p, args.output / "dev" / p.stem, args.until, args.threads, args.dev_pairs)
                      for p in candidates]
            atomic_json(selected, dict(selected=max(ranked, key=lambda r: r["score"]), candidates=ranked))
        best = json.loads(selected.read_text())["selected"]
        model = Path(best["path"])
        if digest(model) != best["sha256"]:
            raise ValueError("selected candidate changed")
        verify_selected(model, args.output, args.until)
        final(model, args.output / "final", args.until, args.threads, args.pairs, args.smoke)
    except subprocess.TimeoutExpired:
        atomic_json(args.output / "inconclusive.json", dict(status="inconclusive_budget", accepted=False))
        raise


if __name__ == "__main__":
    main()
