"""Independent native matches and paired, stratified evaluation summaries."""
import argparse
from fractions import Fraction
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess

import numpy as np

from native import BUILD
from policy import atomic_json

DEV_SEED = 0x524C444556
TEST_SEED = 0x524C54455354
SMOKE_SEED = 0x524C534D4F4B45


def matches(model, final, threads, direct, common, max_steps, anchor=None, timeout=None):
    seed = DEV_SEED
    if final:
        seed = TEST_SEED if (direct, common, max_steps) == (768, 256, 6000) else SMOKE_SEED
    args = [
        str(BUILD / "pvp-rl-eval.exe"), "evaluate", str(model.resolve()),
        "final" if final else "selection", str(seed),
        str(direct), str(common), str(max_steps),
    ]
    if anchor:
        args.append(str(anchor.resolve()))
    args.append("--legacy-durability")
    env = dict(os.environ, RAYON_NUM_THREADS=str(threads))
    return json.loads(subprocess.check_output(args, env=env, timeout=timeout))


def ranking(rows):
    def win_rate(row):
        trials = row["trials"]
        return Fraction(sum(t["new"]["outcome"] == 1 for t in trials), len(trials))

    direct = [win_rate(row) for row in rows if row["rival"] == "League-G6"]
    common = [row for row in rows if row["rival"] != "League-G6"]
    rates = sorted(win_rate(row) for row in common)
    worst = rates[:math.ceil(len(rates) / 10)]
    score = (2 * sum(direct) / len(direct) + 2 * sum(rates) / len(rates) + sum(worst) / len(worst)) / 5
    hp_score = np.mean([
        t["new"]["hp"] / 20 if t["new"]["outcome"] == 1 else 0
        for row in common for t in row["trials"]
    ])
    return score, float(hp_score)


def interval(samples):
    return np.percentile(samples, [2.5, 97.5]).tolist()


def wilson(wins, total):
    z = 1.959963984540054
    p = wins / total
    center = (p + z * z / (2 * total)) / (1 + z * z / total)
    radius = z * math.sqrt(p * (1 - p) / total + z * z / (4 * total**2)) / (1 + z * z / total)
    return [center - radius, center + radius]


def hp_pairs(rows, baseline, rng, replicates):
    hp = []
    totals = np.zeros(replicates)
    counts = np.zeros(replicates, dtype=np.int64)
    for row in rows:
        trials = [t for t in row["trials"] if t[baseline] is not None]
        if not trials:
            continue
        won = np.array([t["new"]["outcome"] == t[baseline]["outcome"] == 1 for t in trials])
        if not won.any():
            continue
        diff = np.array([t["new"]["hp"] - t[baseline]["hp"] for t in trials])
        hp.extend(diff[won])
        for offset in range(0, replicates, 128):
            size = min(128, replicates - offset)
            picks = rng.integers(0, len(trials), size=(size, len(trials)))
            totals[offset:offset + size] += (diff * won)[picks].sum(1)
            counts[offset:offset + size] += won[picks].sum(1)
    if not hp:
        return None
    valid = counts > 0
    return {"pairs": len(hp), "mean_delta": float(np.mean(hp)), "ci95": interval(totals[valid] / counts[valid])}


def summary(rows, replicates=4000):
    rng = np.random.default_rng(0x50414952)
    boot_direct = np.zeros(replicates)
    boot_delta = np.zeros(replicates)
    direct_wins = direct_n = common_n = common_new = common_old = 0
    timeout = count = 0
    conditions = {}
    cells = []
    hp, steps = [], []
    anchor_n = anchor_old = anchor_new = 0
    for row in rows:
        trials = row["trials"]
        n = len(trials)
        won = np.array([t["new"]["outcome"] == 1 for t in trials], dtype=np.int8)
        wins = int(won.sum())
        timed_out = sum(t["new"]["outcome"] == 3 for t in trials)
        timeout += timed_out
        count += n
        hp.extend(t["new"]["hp"] for t in trials if t["new"]["outcome"] == 1)
        steps.extend(t["new"]["steps"] for t in trials)
        cell = {"rival": row["rival"], "network": row["network"], "n": n,
                "wins": wins, "win_ci95": wilson(wins, n), "timeouts": timed_out}
        if row["rival"] == "League-G6":
            direct_wins += wins
            direct_n += n
            boot_direct += rng.binomial(n, wins / n, replicates)
        else:
            old = np.array([t["g6"]["outcome"] == 1 for t in trials], dtype=np.int8)
            delta = won - old
            probs = np.bincount(delta + 1, minlength=3) / n
            resampled = rng.multinomial(n, probs, replicates)
            boot_delta += resampled[:, 2] - resampled[:, 0]
            old_wins = int(old.sum())
            cell["g6_wins"] = old_wins
            common_new += wins
            common_old += old_wins
            common_n += n
            net = conditions.setdefault(row["network"], [0, 0, 0])
            net[0] += wins
            net[1] += old_wins
            net[2] += n
            if trials[0]["anchor"] is not None:
                anchor_n += n
                anchor_new += wins
                anchor_old += sum(t["anchor"]["outcome"] == 1 for t in trials)
        cells.append(cell)
    network_deltas = {key: (new - old) / n for key, (new, old, n) in conditions.items()}
    direct = direct_wins / direct_n
    direct_ci = interval(boot_direct / direct_n)
    common_delta = (common_new - common_old) / common_n
    common_ci = interval(boot_delta / common_n)
    anchor_hp = hp_pairs(rows, "anchor", rng, replicates)
    return {
        "direct_win_rate": direct, "direct_ci95": direct_ci,
        "common_win_rate": common_new / common_n, "g6_common_win_rate": common_old / common_n,
        "common_delta": common_delta, "common_delta_ci95": common_ci,
        "network_deltas": network_deltas, "timeout_rate": timeout / count,
        "accepted": direct >= .60 and direct_ci[0] > .50 and common_delta > 0 and common_ci[0] > 0
        and min(network_deltas.values()) >= -.05 and timeout / count < .01,
        "win_hp": None if not hp else {
            "mean": float(np.mean(hp)), "median": float(np.median(hp)), "p10": float(np.percentile(hp, 10)),
        },
        "win_weighted_hp": sum(hp) / count, "mean_seconds": float(np.mean(steps)) / 20,
        "paired_hp_vs_g6": hp_pairs(rows, "g6", rng, replicates),
        "anchor": None if anchor_n == 0 else {
            "wins": anchor_old, "candidate_wins": anchor_new, "n": anchor_n,
            "paired_hp": anchor_hp,
            "hp_improved": anchor_new >= anchor_old and anchor_hp is not None and anchor_hp["ci95"][0] > 0,
        },
        "cells": cells,
    }


def evaluate(model, output, threads, anchor=None, timeout=None, direct=768, common=256, max_steps=6000):
    output.mkdir(parents=True, exist_ok=True)
    rows = matches(model, True, threads, direct, common, max_steps, anchor, timeout)
    atomic_json(output / "trials.json", rows)
    result = summary(rows)
    result.update({
        "test_seed": TEST_SEED if (direct, common, max_steps) == (768, 256, 6000) else SMOKE_SEED,
        "direct_per_cell": direct, "common_per_cell": common,
        "max_steps": max_steps, "model": str(model.resolve()),
        "model_sha256": hashlib.sha256(model.read_bytes()).hexdigest(),
        "anchor_model": str(anchor.resolve()) if anchor else None,
    })
    result["accepted"] = result["accepted"] and direct == 768 and common == 256 and max_steps == 6000
    atomic_json(output / "summary.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--anchor", type=Path)
    parser.add_argument("--threads", type=int, default=8)
    parser.add_argument("--direct", type=int, default=768)
    parser.add_argument("--common", type=int, default=256)
    parser.add_argument("--max-steps", type=int, default=6000)
    parser.add_argument("--seconds", type=float, default=5400)
    args = parser.parse_args()
    report = evaluate(args.model, args.output, args.threads, args.anchor, args.seconds,
                      args.direct, args.common, args.max_steps)
    print(json.dumps({k: v for k, v in report.items() if k != "cells"}, ensure_ascii=False))


if __name__ == "__main__":
    main()
