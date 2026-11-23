"""Install saved GRU policies and their provenance into a PvPBotRL data folder."""
import argparse
from datetime import datetime, timezone
from hashlib import sha256
import json
from pathlib import Path
import re
import tempfile


REPO = Path(__file__).resolve().parents[2]
INITIAL = "Recurrent PPO"
NEXTGEN = "Next-gen GRU"
BRANCHES = {
    "memory-A": "Memory comparison (32 steps, no burn-in)",
    "memory-B": "Memory comparison (32 steps, 16 burn-in)",
    "memory-C": "Memory comparison (64 steps, 16 burn-in)",
    "main-71001": "Main training",
    "main-71002": "Main training",
    "exploiter-initial": "Champion specialist",
    "exploiter-middle": "Mid-campaign candidate specialist",
    "finish-balanced": "Balanced fine-tuning",
    "finish-high-ping": "High-ping fine-tuning",
}
EXPORT = re.compile(r"(candidate|snapshot)-(\d+)\.json")


def read_json(path):
    return json.loads(path.read_bytes())


def install_file(path, content):
    # Publish each immutable weight before making it visible in the manifest.
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as file:
        temporary = Path(file.name)
    try:
        temporary.write_bytes(content)
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def initial_events(run):
    updates, selections = {}, []
    with (run / "training.jsonl").open(encoding="utf-8") as log:
        for line in log:
            event = json.loads(line)
            updates.setdefault(event["update"], event)
            if event["event"] in ("selection", "selection_budget_exhausted"):
                selections.append(event)
    return updates, selections


def exports(run, generation, branch, evaluated):
    settings = read_json(run / "settings.json")
    updates, selections = initial_events(run) if generation == INITIAL else ({}, [])
    for path in sorted(run.iterdir()):
        match = EXPORT.fullmatch(path.name)
        if not match or not path.is_file():
            continue
        kind, ordinal = match[1], int(match[2])
        saved = datetime.fromtimestamp(path.stat().st_mtime, timezone.utc)
        content = path.read_bytes()
        try:
            model = json.loads(content)
        except (ValueError, UnicodeError):
            print(f"Skipping invalid model JSON {run.name}/{path.name}")
            continue
        if (not isinstance(model, dict) or model.get("version") != 1
                or model.get("input") != "full" or not model.get("hidden_gate")):
            continue
        digest = sha256(content).hexdigest()
        sidecar = path.with_suffix(".source.json")
        source = read_json(sidecar) if sidecar.is_file() else {}
        if source.get("sha256") == digest:
            update, steps, phase = (source[key] for key in ("update", "steps", "phase"))
            saved_at, time_source = source["savedAt"], "export"
        else:
            saved_at, time_source = saved.isoformat(timespec="seconds"), "file"
            if generation == INITIAL:
                event = updates.get(ordinal) if kind == "snapshot" else (
                    selections[ordinal - 1] if 0 < ordinal <= len(selections) else None
                )
                if event is None:
                    print(f"Skipping {path.name} from {run.name} with no matching training event")
                    continue
                update, steps, phase = (event[key] for key in ("update", "steps", "phase"))
            else:
                update = ordinal
                steps = evaluated.get(path.stem, update * settings["lanes"] * settings["rollout"])
                phase = 1
        process = branch
        if generation == INITIAL:
            process = "Main training (win objective)" if phase == 0 else "Main training (win and remaining HP)"
        yield content, dict(
            sha256=digest, generation=generation, process=process, branch=run.name,
            seed=settings["seed"], update=update, steps=steps, savedAt=saved_at,
            timeSource=time_source, source=path.name,
        )


def install(repo, destination):
    history = repo / "build" / "recurrent-ppo"
    nextgen = history / "nextgen-24h"
    runs = [(history / "full" / f"seed-{seed}", INITIAL, "") for seed in (61001, 61002)]
    runs += [(nextgen / name, NEXTGEN, branch) for name, branch in BRANCHES.items()]
    if not any((run / "settings.json").is_file() for run, _, _ in runs):
        print("No training history found for PvPBotRL.")
        return

    campaign = nextgen / "campaign.json"
    evaluated = {}
    if campaign.is_file():
        for row in read_json(campaign)["candidates"]:
            branch, _, candidate = row["label"].rpartition("-candidate-")
            evaluated.setdefault(branch, {})["candidate-" + candidate] = row["steps"]
    excluded = {sha256(path.read_bytes()).hexdigest()
                for path in (repo / "models" / "bundled").glob("*.json")}
    destination.mkdir(parents=True, exist_ok=True)
    weights = destination / "weights"
    weights.mkdir(exist_ok=True)
    entries = []
    for run, generation, branch in runs:
        if not (run / "settings.json").is_file():
            continue
        for content, metadata in exports(run, generation, branch, evaluated.get(run.name, {})):
            digest = metadata["sha256"]
            if digest in excluded:
                continue
            excluded.add(digest)
            target = weights / f"{digest}.json"
            if not target.is_file() or sha256(target.read_bytes()).hexdigest() != digest:
                install_file(target, content)
            entries.append(metadata)
    manifest = json.dumps(dict(version=1, entries=entries), ensure_ascii=False, indent=2)
    install_file(destination / "catalog.json", (manifest + "\n").encode())
    for generation in (INITIAL, NEXTGEN):
        count = sum(row["generation"] == generation for row in entries)
        print(f"PvPBotRL training models from {generation}: {count}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path, help="Path to the plugin's training-models folder")
    parser.add_argument("--repo", type=Path, default=REPO, help="Repository with the original training outputs")
    args = parser.parse_args()
    install(args.repo, args.destination)
