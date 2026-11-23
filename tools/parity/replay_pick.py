"""Compare the ordinary client's attack selection with Rust from the same geometry."""
import argparse
import json
from pathlib import Path
import subprocess

from analyze_combat import validate
from extract_impulse import read_telemetry
from provenance import ROOT


def body(fields):
    return {k: v for k, v in fields.items() if k != "jump_delay"} | {"attack_ticks": 0, "cached_speed": 0.1}


def replay(directory, trial):
    folder = directory / trial
    request = json.loads((folder / "request.json").read_text())
    server = json.loads((folder / "server.json").read_text())
    states, _ = validate(server, scripted=True)
    if server["bot"]: raise ValueError("Expected a two-client reference")
    client = read_telemetry(directory / request["striker"] / "telemetry.ndjson")
    target_id = server["player_connections"]["opponent"]["entity_id"]
    start, end = states[0]["epoch_ms"], states[-1]["epoch_ms"]
    picks = [e for e in client[1:] if e["event"] == "attack_input_pick" and start <= e["epoch_ms"] <= end]
    if not picks:
        raise ValueError("Attack input selection is missing")
    cases, expected = [], []
    for pick in picks:
        data = pick["data"]
        targets = [e["state"] for e in data["replicas"] if e["id"] == target_id]
        if len(targets) != 1 or data.get("picked_entity_id", target_id) != target_id:
            raise ValueError("Wrong or missing target in the attack geometry")
        cases.append({"viewer": body(data["local"]), "target": body(targets[0]),
                      "floor_y": states[0]["human"]["position"][1]})
        expected.append({"ENTITY": "entity", "BLOCK": "floor", "MISS": "miss"}[data["kind"]])
    actual = json.loads(subprocess.check_output([str(ROOT / "target/debug/examples/parity_pick.exe")],
                                               input=json.dumps(cases).encode()))
    comparison = [{"sequence": row["sequence"], "reference": want, "rust": result, "matches": want == result}
                  for row, want, result in zip(picks, expected, actual, strict=True)]
    report = {"schema": 1, "scope": "same-observed-attack-geometry", "passed": all(row["matches"] for row in comparison),
              "results": comparison}
    (folder / "pick-difference.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("trial")
    args = parser.parse_args()
    report = replay(args.directory, args.trial)
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report["passed"] else 1)
