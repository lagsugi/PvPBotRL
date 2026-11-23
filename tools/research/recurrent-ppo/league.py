"""Immutable, content-addressed opponents and per-seed PFSP statistics."""
from collections import deque
import hashlib
import json
from pathlib import Path

import numpy as np

from policy import atomic_json

WEAK = (10, 1, 11, 8)
HIGH = (5, 6)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def model_id(path):
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if data.get("version") != 1 or data.get("input") != "full" or data.get("hidden_gate") is None:
        raise ValueError(f"expected full-input GRU: {path}")
    shapes = {"encoder": (64, 44), "input_gate": (192, 64),
              "hidden_gate": (192, 64), "actor": (10, 64)}
    for key, (rows, cols) in shapes.items():
        for field, shape in (("weight", (rows, cols)), ("bias", (rows,))):
            a = np.asarray(data[key][field], dtype=np.float32)
            if a.shape != shape or not np.isfinite(a).all():
                raise ValueError(f"invalid {key}.{field}: {path}")
    for field in ("mean", "scale"):
        a = np.asarray(data[field], dtype=np.float32)
        if a.shape != (44,) or not np.isfinite(a).all() or (field == "scale" and (a <= 0).any()):
            raise ValueError(f"invalid {field}: {path}")
    return hashlib.sha256(json.dumps(data, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode()).hexdigest()


def inventory(champion, history, output):
    seen, rows = set(), []
    paths = [("champion", champion)] + [("history", p) for p in sorted(Path(history).glob("snapshot-*.json"))]
    if len(paths) == 1:
        raise ValueError("no historical snapshots found")
    for group, path in paths:
        key = model_id(path)
        row = dict(group=group, path=str(Path(path).resolve()), id=key, sha256=digest(path))
        if key not in seen:
            rows.append(row)
            seen.add(key)
    atomic_json(output, {"source_count": len(paths) - 1, "models": rows})
    return rows


def capped(weights, cap=.1):
    """Water filling; fewer than ten opponents necessarily relax the 10% cap."""
    weights = np.asarray(weights, dtype=float)
    n = len(weights)
    if not n:
        raise ValueError("empty sampling pool")
    cap = max(cap, 1 / n)
    result = np.zeros(n)
    free = np.ones(n, dtype=bool)
    for _ in range(n + 1):
        w = weights[free]
        p = (w / w.sum() if w.sum() else np.ones(len(w)) / len(w)) * (1 - result.sum())
        saturated = p > cap + 1e-12
        ids = np.flatnonzero(free)
        if not saturated.any():
            result[ids] = p
            break
        result[ids[saturated]] = cap
        free[ids[saturated]] = False
    return result / result.sum()


class League:
    def __init__(self, models, state=None):
        self.models = []
        self.results = {}
        self.counts = {}
        if state:
            self.models = state["models"]
            self.results = {k: deque(v, maxlen=256) for k, v in state["results"].items()}
            self.counts = state["counts"]
        for row in models:
            slot = self.add(row)
            self.models[slot]["active"] = row.get("active", True)

    def add(self, row):
        for i, existing in enumerate(self.models):
            if existing["id"] == row["id"]:
                return i
        self.models.append(dict(row))
        return len(self.models) - 1

    def register(self, batch):
        for i, row in enumerate(self.models):
            if digest(row["path"]) != row["sha256"]:
                raise ValueError(f"opponent changed: {row['path']}")
            batch.snapshot(i, Path(row["path"]))

    def state(self):
        return {"models": self.models, "results": {k: list(v) for k, v in self.results.items()},
                "counts": self.counts}

    def key(self, rival, net):
        return f"{self.models[rival - 18]['id'] if rival >= 18 else rival}:{net}"

    def record(self, rival, net, outcome=None, steps=0, tokens=0, start=False):
        key = self.key(rival, net)
        row = self.counts.setdefault(key, dict(starts=0, steps=0, tokens=0, wins=0, losses=0, timeouts=0))
        row["starts"] += int(start)
        row["steps"] += steps
        row["tokens"] += tokens
        if outcome:
            row[{1: "wins", 2: "losses", 3: "timeouts"}[outcome]] += 1
            score = {1: 1., 2: 0., 3: .5}[outcome]
            for k in (key, key.rsplit(":", 1)[0] + ":all"):
                self.results.setdefault(k, deque(maxlen=256)).append(score)

    def score(self, rival, net):
        key = self.key(rival, net)
        cell = self.results.get(key, ())
        all_net = self.results.get(key.rsplit(":", 1)[0] + ":all", ())
        prior = (1 + sum(all_net)) / (2 + len(all_net))
        # 16 pseudo-matches shrink sparse cells toward this opponent's all-net score.
        return (1 + sum(cell) + 16 * prior) / (18 + len(cell))

    def choose(self, rng, focus=.3, exploit=False, target=None):
        general_net = lambda: 0 if rng.random() < .5 else int(rng.integers(1, 13))
        if exploit:
            u = rng.random()
            net = 6 if u < .25 else 5 if u < .5 else general_net()
            rival = target if rng.random() < .8 else (17 if rng.random() < .5 else int(rng.integers(15)))
            return rival, net
        if rng.random() < focus:
            return int(rng.choice(WEAK)), 6 if rng.random() < 2 / 3 else 5
        net = general_net()
        groups = {g: [18 + i for i, m in enumerate(self.models) if m["group"] == g and m.get("active", True)]
                  for g in ("champion", "history", "new", "exploiter")}
        champion = groups["champion"][0]
        u = rng.random()
        if u < .3:
            rival = champion
        elif u < .5:
            pool = groups["history"]
            if pool:
                w = np.array([(1 - self.score(i, net)) ** 2 for i in pool])
                w = .3 / len(pool) + .7 * (w / w.sum() if w.sum() else np.ones(len(pool)) / len(pool))
                rival = int(rng.choice(pool, p=capped(w)))
            else:
                rival = champion
        elif u < .6:
            rival = int(rng.choice(groups["new"])) if groups["new"] else champion
        elif u < .8:
            if groups["exploiter"]:
                rival = int(rng.choice(groups["exploiter"]))
            else:
                rival = int(rng.choice(groups["new"])) if groups["new"] and rng.random() < .5 else champion
        else:
            rival = 17 if rng.random() < .5 else int(rng.integers(15))
        return rival, net
