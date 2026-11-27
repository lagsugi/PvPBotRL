"""Frozen paired evaluations, equal-cell estimands and stratified cluster bootstrap."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

import numpy as np

from league import WEAK, digest
from native import BUILD, FIXED_EQUIPMENT_PROFILE
from policy import atomic_json

GATES = {"noninferiority": -.01, "weak_gain": .03, "hp_lower": -.1,
         "timeout_upper": .01, "confidence": .95}
NETWORK_NAMES = ("Mixed", "Both0", "Both2", "Both20", "Both60", "Both120", "Both150",
                 "LocalRival", "LocalActor", "Adverse", "Bot20Rival0", "EarlyBot20Rival0", "Legacy")
RIVAL_NAMES = ("Duel-RL", "Critical+", "Critical", "Pressure", "Orbit", "Counter", "Baseline",
               "League-G5", "League-RL", "League-G1", "League-G2", "League-G3", "G2 challenger",
               "G3 challenger", "G4 challenger", "Reach", "Orbit/Pressure", "League-G6")


def split_seed(base, key):
    return int.from_bytes(hashlib.sha256(f"{base}:{key}".encode()).digest()[:8], "little")


def make_cells(models, direct=8192, common=512, weak150=8192, weak120=2048, history=64):
    cells = [dict(kind="direct", rival="direct", net=n, pairs=direct) for n in range(13)]
    for r in range(18):
        for n in range(13):
            count = weak150 if r in WEAK and n == 6 else weak120 if r in WEAK and n == 5 else common
            cells.append(dict(kind="common", rival=str(r), net=n, pairs=count))
    for m in models:
        if m["group"] == "history":
            for n in (0, 5, 6):
                cells.append(dict(kind="history", rival=m["path"], id=m["id"], sha256=m["sha256"], net=n, pairs=history))
    return cells


def plan(candidate, champion, cells, seed, max_steps, resamples=2000, quality=None, smoke=False,
         fixed_equipment=True):
    cells = [dict(c, seed=split_seed(seed, f"{c['kind']}:{c.get('id', c['rival'])}:{c['net']}"),
                  network=NETWORK_NAMES[c["net"]], opponent="champion" if c["kind"] == "direct" else
                  RIVAL_NAMES[int(c["rival"])] if c["kind"] == "common" else "history:" + c["id"][:12]) for c in cells]
    if len({c["seed"] for c in cells}) != len(cells):
        raise ValueError("duplicate cell seeds")
    manifest = dict(version=1, candidate=str(Path(candidate).resolve()), candidate_sha256=digest(candidate),
                champion=str(Path(champion).resolve()), champion_sha256=digest(champion), cells=cells,
                seed=seed, max_steps=max_steps, resamples=resamples, gates=GATES,
                quality=quality or {}, smoke=smoke,
                unit="seed with two swapped physical seats; independent seeds per cell",
                weighting="equal cells; common-win HP is ratio of equal-cell joint-win HP and joint-win count")
    if fixed_equipment:
        manifest.update(match_profile=FIXED_EQUIPMENT_PROFILE, unbreakable=True)
    return manifest


def interval(point, replicates):
    lo, hi = np.quantile(replicates, [.025, .975])
    return dict(mean=float(point), lower=float(lo), upper=float(hi))


def summarize_cell(path, cell, resamples, rng):
    differences, hp_sum, hp_count, weighted, timeouts, wins, win_hp = [], [], [], [], [], [], []
    champion_timeouts, champion_hp = [], []
    seen = set()
    with Path(path).open(encoding="utf-8") as stream:
        for row in map(json.loads, stream):
            if row["seed"] in seen:
                raise ValueError("duplicate pair seed in cell")
            seen.add(row["seed"])
            a = row["candidate"]
            if len(a) != 2 or any(x["outcome"] not in (1, 2, 3) for x in a):
                raise ValueError("invalid candidate seat pair")
            aw = np.array([x["outcome"] == 1 for x in a])
            timeouts.append(sum(x["outcome"] == 3 for x in a) / 2)
            wins.append(aw.mean())
            win_hp.extend(x["hp"] for x in a if x["outcome"] == 1)
            if cell["kind"] == "direct":
                differences.append(sum({1: 1, 2: -1, 3: 0}[x["outcome"]] for x in a) / 2)
            else:
                b = row["champion"]
                if len(b) != 2 or any(x["outcome"] not in (1, 2, 3) for x in b):
                    raise ValueError("invalid champion seat pair")
                bw = np.array([x["outcome"] == 1 for x in b])
                champion_timeouts.append(sum(x["outcome"] == 3 for x in b) / 2)
                champion_hp.extend(x["hp"] for x in b if x["outcome"] == 1)
                differences.append(float(aw.mean() - bw.mean()))
                hp_sum.append(sum(x["hp"] - y["hp"] for x, y in zip(a, b) if x["outcome"] == y["outcome"] == 1) / 2)
                hp_count.append(float((aw & bw).mean()))
                weighted.append(sum((x["hp"] if x["outcome"] == 1 else 0) -
                                    (y["hp"] if y["outcome"] == 1 else 0) for x, y in zip(a, b)) / 2)
    if len(differences) != cell["pairs"]:
        raise ValueError("incomplete evaluation cell")
    d = np.array(differences)
    values, counts = np.unique(d, return_counts=True)
    boot = rng.multinomial(len(d), counts / len(d), size=resamples) @ values / len(d)
    result = dict(cell, difference=interval(d.mean(), boot), candidate_win=float(np.mean(wins)),
                  timeout=float(np.mean(timeouts)), pair_variance=float(d.var(ddof=1)) if len(d) > 1 else 0.,
                  win_hp_p10=float(np.quantile(win_hp, .1)) if win_hp else None)
    samples = dict(d=boot)
    if hp_sum:
        data = np.array([hp_sum, hp_count, weighted]).T
        estimates = np.empty((resamples, 3))
        for offset in range(0, resamples, 32):
            ids = rng.integers(len(data), size=(min(32, resamples - offset), len(data)))
            estimates[offset:offset + len(ids)] = data[ids].mean(1)
        result.update(hp_sum=float(np.mean(hp_sum)), hp_count=float(np.mean(hp_count)),
                      weighted_hp=float(np.mean(weighted)), champion_timeout=float(np.mean(champion_timeouts)),
                      timeout_delta=float(np.mean(timeouts) - np.mean(champion_timeouts)),
                      champion_win_hp_p10=float(np.quantile(champion_hp, .1)) if champion_hp else None)
        samples.update(hp_sum=estimates[:, 0], hp_count=estimates[:, 1], weighted_hp=estimates[:, 2])
    return result, samples


def aggregate(rows, samples, predicate):
    ids = [i for i, r in enumerate(rows) if predicate(r)]
    if not ids:
        raise ValueError("required evaluation group missing")
    return interval(np.mean([rows[i]["difference"]["mean"] for i in ids]),
                    np.mean([samples[i]["d"] for i in ids], axis=0))


def judge(rows, samples, quality, smoke=False):
    common = lambda r: r["kind"] == "common"
    weak = lambda r: common(r) and int(r["rival"]) in WEAK
    groups = dict(direct=lambda r: r["kind"] == "direct", common=common,
                  normal=lambda r: common(r) and r["net"] not in (5, 6),
                  high=lambda r: common(r) and r["net"] in (5, 6),
                  weak150=lambda r: weak(r) and r["net"] == 6,
                  weak120=lambda r: weak(r) and r["net"] == 5,
                  history=lambda r: r["kind"] == "history")
    metrics = {k: aggregate(rows, samples, predicate) for k, predicate in groups.items()}
    ids = [i for i, r in enumerate(rows) if common(r)]
    denominator = np.mean([rows[i]["hp_count"] for i in ids])
    boot_denominator = np.mean([samples[i]["hp_count"] for i in ids], axis=0)
    if denominator > 0 and (boot_denominator > 0).all():
        hp = interval(np.mean([rows[i]["hp_sum"] for i in ids]) / denominator,
                      np.mean([samples[i]["hp_sum"] for i in ids], axis=0) / boot_denominator)
    else:
        hp = dict(mean=None, lower=None, upper=None)
    metrics["common_win_hp"] = hp
    metrics["weighted_hp"] = float(np.mean([rows[i]["weighted_hp"] for i in ids]))
    checks = {k: metrics[k]["lower"] >= GATES["noninferiority"]
              for k in ("common", "normal", "high", "weak120", "history")}
    checks["direct"] = metrics["direct"]["lower"] > 0
    checks["weak150_mean"] = metrics["weak150"]["mean"] >= .03 and metrics["weak150"]["lower"] > 0
    weak_rows = [r for r in rows if weak(r) and r["net"] == 6]
    checks["weak150_each"] = len(weak_rows) == 4 and all(r["difference"]["lower"] > 0 for r in weak_rows)
    checks["hp"] = hp["mean"] is not None and hp["mean"] >= 0 and hp["lower"] >= -.1 and metrics["weighted_hp"] >= 0
    for kind in ("direct", "common"):
        metrics[f"{kind}_timeout"] = float(np.mean([r["timeout"] for r in rows if r["kind"] == kind]))
        checks[f"{kind}_timeout"] = metrics[f"{kind}_timeout"] < .01
    checks["execution_quality"] = all(quality.get(k) is True for k in ("parity", "reset", "resume", "replay", "tests"))
    return dict(accepted=bool(all(checks.values()) and not smoke), status="smoke_only" if smoke else "complete",
                checks={k: bool(v) for k, v in checks.items()}, metrics=metrics, cells=rows)


def execute(manifest, output, threads=8, deadline=None, resume=False, adoption=True):
    profile = manifest.get("match_profile")
    if profile is not None and (profile != FIXED_EQUIPMENT_PROFILE or manifest.get("unbreakable") is not True):
        raise ValueError("invalid evaluation match profile")
    equipment_args = [] if profile == FIXED_EQUIPMENT_PROFILE else ["--legacy-durability"]
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    manifest_path = output / "manifest.json"
    if manifest_path.exists():
        if not resume or json.loads(manifest_path.read_text(encoding="utf-8")) != manifest:
            raise ValueError("evaluation manifest exists or changed; cannot reuse final test")
    else:
        atomic_json(manifest_path, manifest)
    for role in ("candidate", "champion"):
        if digest(manifest[role]) != manifest[role + "_sha256"]:
            raise ValueError(f"{role} changed after freeze")
    started = time.monotonic()
    rows, samples = [], []
    env = dict(os.environ, RAYON_NUM_THREADS=str(threads))
    for i, cell in enumerate(manifest["cells"]):
        if deadline is not None and time.monotonic() >= deadline:
            raise subprocess.TimeoutExpired("paired evaluation", time.monotonic() - started)
        if cell.get("sha256") and digest(cell["rival"]) != cell["sha256"]:
            raise ValueError("historical opponent changed")
        path = output / f"cell-{i:04d}.jsonl"
        if not path.exists():
            tmp = path.with_suffix(".partial")
            with tmp.open("wb") as stream:
                subprocess.run([str(BUILD / "pvp-rl-eval.exe"), "paired", manifest["candidate"],
                                manifest["champion"], cell["rival"], str(cell["net"]), str(cell["seed"]),
                                str(cell["pairs"]), str(manifest["max_steps"]), *equipment_args], stdout=stream, env=env,
                               check=True, timeout=max(.01, deadline - time.monotonic()) if deadline else None)
            tmp.replace(path)
        row, sample = summarize_cell(path, cell, manifest["resamples"],
                                     np.random.default_rng(split_seed(manifest["seed"], f"bootstrap:{i}")))
        rows.append(row)
        samples.append(sample)
    if deadline is not None and time.monotonic() >= deadline:
        raise subprocess.TimeoutExpired("evaluation analysis", time.monotonic() - started)
    report = judge(rows, samples, manifest["quality"], manifest["smoke"]) if adoption else dict(cells=rows)
    report.update(seconds=time.monotonic() - started, games=sum(c["pairs"] * (2 if c["kind"] == "direct" else 4) for c in manifest["cells"]))
    atomic_json(output / "summary.json", report)
    return report


def dev_rank(report):
    rows = report["cells"]
    mean = lambda cells, key: float(np.mean([r[key] for r in cells]))
    common = [r for r in rows if r["kind"] == "common"]
    ordinary = [r for r in common if r["net"] not in (5, 6)]
    weak = [r for r in common if r["net"] == 6 and int(r["rival"]) in WEAK]
    direct = mean([r for r in rows if r["kind"] == "direct"], "candidate_win")
    normal_delta = np.mean([r["difference"]["mean"] for r in ordinary])
    common_delta = np.mean([r["difference"]["mean"] for r in common])
    # Dev tolerances are deliberately looser than the independent final gates.
    eligible = normal_delta >= -.02 and common_delta >= -.02
    weak_mean, weak_min = mean(weak, "candidate_win"), min(r["candidate_win"] for r in weak)
    return [int(eligible), float(direct + weak_mean), float(weak_min), float(direct), mean(common, "weighted_hp")]
