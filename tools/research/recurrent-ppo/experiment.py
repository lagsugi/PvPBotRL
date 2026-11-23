"""Run the planned CPU experiment, or exercise its full workflow with --quick."""
import argparse
from fractions import Fraction
import json
from pathlib import Path
import subprocess

from evaluate import evaluate, matches, ranking
from native import BUILD
from policy import atomic_json
from train import DEFAULT_LANES, parser as training_parser, train


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--lanes", type=int, default=DEFAULT_LANES)
    parser.add_argument("--threads", type=int, default=8)
    parser.add_argument("--torch-threads", type=int, default=4)
    parser.add_argument("--quick", action="store_true", help="workflow verification with short duels")
    args = parser.parse_args()
    if not 1 <= args.lanes <= 4096:
        parser.error("--lanes must be between 1 and 4096")
    if args.output.exists() and any(args.output.iterdir()):
        parser.error("choose an empty experiment directory")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)

    def run(label, seconds, lanes, *options, seed=61001):
        params = [
            str(args.output / label), "--seconds", str(seconds), "--lanes", str(lanes),
            "--threads", str(args.threads), "--torch-threads", str(args.torch_threads),
            "--seed", str(seed), *options,
        ]
        if args.quick:
            params += ["--rollout", "32", "--sequence", "8", "--max-steps", "80",
                       "--dev-direct", "1", "--dev-common", "1", "--curriculum-seconds", "1"]
        print(f"[{label}]", flush=True)
        return train(training_parser().parse_args(params))

    lanes = args.lanes
    pilot = {lanes: run(f"pilot-{lanes}", 2 if args.quick else 30, lanes,
                        "--no-selection", "--win-only")}
    atomic_json(args.output / "pilot.json", {"lanes": lanes, "runs": pilot})
    seeds = (61001, 61002)
    runs = []
    for seed in seeds:
        phase = ("--win-seconds", "3", "--selection-interval", "3") if args.quick else ()
        report = run(f"seed-{seed}", 8 if args.quick else 14400, lanes, *phase, seed=seed)
        if report["best"] is not None:
            runs.append((seed, report))
    variants = {
        "legacy": ("--input", "legacy"),
        "no-opponent-hp": ("--input", "no_opponent_hp"),
        "no-previous-action": ("--input", "no_action"),
        "no-gru": ("--feedforward",),
    }
    comparison = {}
    reference = args.output / "seed-61001" / "candidate-0001.json"
    for label, flags in variants.items():
        comparison[label] = run(label, 3 if args.quick else 1800, lanes, *flags,
                                "--win-only", "--no-selection")
    if reference.exists():
        with (reference.parent / "training.jsonl").open(encoding="utf-8") as log:
            first_selection = next(event for event in map(json.loads, log) if event["event"] == "selection")
        comparison["full"] = {"steps": first_selection["steps"], "elapsed": first_selection["seconds"]}
        for label, row in comparison.items():
            model = reference if label == "full" else args.output / label / "last.json"
            trials = matches(model, False, args.threads, 1 if args.quick else 32,
                             1 if args.quick else 16, 80 if args.quick else 6000, timeout=300)
            score, hp = ranking(trials)
            row.update(model=str(model), score=float(score), win_weighted_hp_rate=hp)
    atomic_json(args.output / "comparisons.json", comparison)
    if not runs:
        raise RuntimeError("no checkpoint completed selection within the budget")
    seed, selected = max(runs, key=lambda item: (
        Fraction(item[1]["best"]["score"]), item[1]["best"]["hp"],
    ))
    path = args.output / f"seed-{seed}"
    model = path / selected["best"]["model"]
    anchor = path / selected["anchor"]["model"] if selected["anchor"] else None
    atomic_json(args.output / "selected.json", {
        "seed": seed, "model": str(model), "anchor": str(anchor) if anchor else None,
        "selection": selected["best"],
    })
    report = evaluate(model, args.output / "final", args.threads, anchor,
                      timeout=90 if args.quick else 5400,
                      direct=1 if args.quick else 768, common=1 if args.quick else 256,
                      max_steps=80 if args.quick else 6000)
    for net in (0, 5, 11):
        subprocess.run([
            str(BUILD / "pvp-rl-eval.exe"), "duel", str(model), str(net), "17",
            str(0x5245504C4159), str(args.output / "final" / f"replay-{net}.json"),
            str(80 if args.quick else 6000),
        ], check=True)
    print(f"Accepted: {report['accepted']}   Results: {args.output / 'final' / 'summary.json'}")


if __name__ == "__main__":
    main()
