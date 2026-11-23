"""Show the client's state and packet applications around a motion difference."""
import argparse
import json
from pathlib import Path
import subprocess

from compare import compare
from extract_impulse import read_telemetry
from provenance import ROOT, sha256


def report(root):
    rows = read_telemetry(root / "client/telemetry.ndjson")
    pre = {r["client_tick"]: r["data"] for r in rows if r["event"] == "local_pre_movement"}
    results = []
    for trial in sorted((root / "client").glob("*/difference.json")):
        comparison = json.loads(trial.read_text())
        first = comparison["first_difference"]
        if first is None:
            results.append({"trial": trial.parent.name, "passed": comparison["passed"]})
            continue
        sample = next(r for r in first["context_reference"] if r["step"] == first["key"][0])
        tick = sample["client_tick"]
        context = [r for r in rows[1:-1] if tick - 2 <= r["client_tick"] <= tick + 1 and r["event"] in {
            "self_metadata_applied", "self_attributes_applied", "local_pre_movement", "local_post_movement"}]
        if not any(r["client_tick"] == tick and r["event"] == "local_pre_movement" for r in context):
            raise ValueError(f"Missing pre-movement sample in {trial.parent.name}")
        capture = trial.parent / "client.ndjson"
        states = [row for line in capture.read_text().splitlines() if (row := json.loads(line))["event"] == "state"]
        inputs = [pre[state["client_tick"]] for state in states]
        feedback = trial.parent / "measured-feedback.json"
        feedback.write_text(json.dumps([{"sprinting": r["state"]["sprinting"], "movement_speed": r["speed"]}
                                        for r in inputs], indent=2) + "\n")
        subprocess.run([str(ROOT / "target/debug/examples/parity_motion.exe"), str(trial.parent), str(feedback)], check=True)
        conditioned = compare(capture, trial.parent / "rust-with-feedback.ndjson")
        conditioned["feedback_sha256"] = sha256(feedback)
        (trial.parent / "feedback-difference.json").write_text(json.dumps(conditioned, indent=2) + "\n")
        results.append({"trial": trial.parent.name, "passed": False, "first_difference": first,
                        "client_tick": tick, "client_events": context, "matched_with_measured_feedback": conditioned["passed"]})
    if not results: raise ValueError("No movement comparisons found")
    output = {"schema": 1, "scope": "movement-attribute-and-metadata-feedback", "trials": results}
    (root / "feedback-difference.json").write_text(json.dumps(output, indent=2) + "\n")
    return output


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    result = report(args.root)
    print(json.dumps({"trials": len(result["trials"]), "passed": sum(r["passed"] for r in result["trials"])}))
