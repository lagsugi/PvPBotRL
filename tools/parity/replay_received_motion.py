"""Replay ordinary-client battle motion with the actually received impulses and speed inputs."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess

from compare import compare, load
from extract_impulse import read_telemetry
from provenance import ROOT, sha256


def replay(source, destination):
    capture = source / "client.ndjson"
    _, actions = load(capture)
    telemetry = source.parent / "telemetry.ndjson"
    before = {}
    last_impulse = None
    for row in read_telemetry(telemetry):
        match row["event"]:
            case "self_motion_applied":
                last_impulse = row["data"]["decoded_velocity"]
                if last_impulse != row["data"]["state"]["velocity"]:
                    raise ValueError("Received motion packet did not replace local velocity")
            case "local_pre_movement":
                tick = row["client_tick"]
                if tick in before:
                    raise ValueError("Duplicate pre-movement capture")
                data = row["data"]
                feedback = {"sprinting": data["state"]["sprinting"], "movement_speed": data["speed"]}
                if last_impulse is not None:
                    feedback["velocity_impulse"] = last_impulse
                before[tick] = feedback
                last_impulse = None
    ticks = [a["client_tick"] for a in actions]
    if ticks != list(range(ticks[0], ticks[0] + len(ticks))):
        raise ValueError("Missing or reordered client movement ticks")
    feedback = [before[tick] for tick in ticks]
    destination.mkdir(parents=True, exist_ok=False)
    for name in ("client.ndjson", "scenario.json", "initial.json"):
        shutil.copy2(source / name, destination / name)
    inputs = destination / "measured-feedback.json"
    inputs.write_text(json.dumps(feedback, indent=2) + "\n")
    subprocess.run([str(ROOT / "target/debug/examples/parity_motion.exe"), str(destination), str(inputs),
                    "--physics-after-receive"], check=True)
    report = compare(destination / "client.ndjson", destination / "rust-with-feedback.ndjson")
    report.update(scope="client-physics-after-measured-receive", telemetry_sha256=sha256(telemetry),
                  feedback_sha256=sha256(inputs), movement_ticks=len(ticks),
                  applied_impulses=sum("velocity_impulse" in row for row in feedback))
    (destination / "difference.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    result = replay(args.source, args.destination)
    print(json.dumps({key: result[key] for key in ("passed", "movement_ticks", "applied_impulses")}))
    raise SystemExit(0 if result["passed"] else 1)
