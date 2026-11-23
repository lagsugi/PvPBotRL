"""Collect available local verification results without treating missing coverage as passing."""
import argparse
import json
from pathlib import Path

from scenarios import combat_suite, movement_suite


def motion(roots, ids):
    passed, failed, missing = [], [], []
    for name in ids:
        comparisons = [(root / "client" / name / "difference.json", root / "client" / name / "bot-difference.json")
                       for root in roots]
        completed = [files for files in comparisons if all(path.is_file() for path in files)]
        if not completed:
            missing.append(name)
        elif all(json.loads(path.read_text())["passed"] is True for files in completed for path in files):
            passed.append(name)
        else:
            failed.append(name)
    return {"expected_cases": len(ids), "passing_cases": len(passed), "failed_cases": failed, "missing_cases": missing}


def combat(roots):
    trials = {}
    for root in roots:
        for path in root.glob("*/server.json"):
            request = json.loads(path.with_name("request.json").read_text())
            capture = json.loads(path.read_text())
            if capture["status"] != "complete" or request["bot"] != capture["bot"]:
                raise ValueError(f"Incomplete trial {path}")
            case = request["scenario"]
            if case == "critical" and not any(e["event"] == "damage" and e["raw"] == 10.5 and not e["cancelled"]
                                             for e in capture["events"]):
                continue
            key = (case, request["striker"], capture["bot"])
            trials[key] = trials.get(key, 0) + 1
    return {case: {name: {kind: trials.get((case, name, kind == "bot"), 0) for kind in ("ordinary", "bot")}
                   for name in ("ParityA", "ParityB")} for case in combat_suite()}


def collect(root):
    cases = [f"{n}_seat{s}_{i}" for n in movement_suite() for s in range(2) for i in range(5)]
    random = [f"random_{seed:03}_seat0_0" for seed in range(100)]
    fixed = motion([root / "motion-fixed-5"], cases)
    long = motion([root / "motion-long-6000"], ["long_idle_seat0_0"])
    random = motion(list(root.glob("motion-random-*")), random)
    matrix = root / "combat-network-matrix/suite-difference.json"
    trials = json.loads(matrix.read_text())["trials"] if matrix.exists() else []
    capture_roots = [root / name for name in (
        "combat-fixed-5", "combat-critical-5", "combat-overlap-fixed-5", "combat-critical-rechecked-5")]
    ordinary_replays = {}
    for cases in capture_roots:
        summary = cases / "rust-replay-checked/summary.json"
        if not summary.exists():
            summary = cases / "rust-replay/summary.json"
        if summary.exists():
            rows = json.loads(summary.read_text())["trials"]
            fields = ("matched_states", "damage_matches", "fall_damage_matches", "decoded_impulse_matches")
            ordinary_replays[cases.name] = {"total": len(rows),
                "all_measured_fields_matched": sum(all(row.get(field) is True for field in fields) for row in rows),
                "incomplete": sum("incomplete" in row for row in rows)}
    mixed = {}
    for run in sorted(root.glob("combat-mixed-*")):
        for capture_path in run.glob("mixed_*/server.json"):
            capture = json.loads(capture_path.read_text())
            request = json.loads(capture_path.with_name("request.json").read_text())
            striker = request["striker"]
            scripted_clients = [striker] if capture["bot"] else ["ParityA", "ParityB"]
            if (capture["status"] == "complete" and capture["capture_ticks"] == request["capture_ticks"] and
                    all((run / client / capture_path.parent.name / "complete.json").exists() for client in scripted_clients)):
                key = f'{request["seed"]:03}/{len(request["opponent_actions"])}'
                mixed.setdefault(key, []).append({"run": run.name, "trial": capture_path.parent.name,
                                                "bot": capture["bot"], "striker": striker})
    return {"schema": 1, "passed": False, "scope": "combined-three-way-verification",
            "movement": {"fixed": fixed, "random": random, "long": long},
            "combat_completed_captures": combat(capture_roots), "ordinary_measured_combat_replays": ordinary_replays,
            "mixed_combat_captures": mixed,
            "network": {"profiles_measured": len(trials),
                        "profiles_matching_measured_combat_fields": sum(r["measured_checks_passed"] for r in trials)},
            "outstanding": ["equivalent live bot/client communication and observation", "complete three-way fight replay",
                            "complete mixed-input duel repetition, seed, timing and FPS coverage"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    report = collect(args.root)
    (args.root / "coverage.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passed": report["passed"], "movement_passes": {
        key: value["passing_cases"] for key, value in report["movement"].items()}}))
    raise SystemExit(0 if report["passed"] else 1)
