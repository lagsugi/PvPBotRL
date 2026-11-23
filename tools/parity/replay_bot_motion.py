"""Check the moving bot's independent prediction and packets in an isolated trial."""

import argparse
import json
from pathlib import Path
import subprocess

from compare import compare
from provenance import ROOT, sha256


def motion(state):
    return {**state, **{key: dict(zip("xyz", state[key], strict=True)) for key in ("position", "velocity")}}


def replay(source, destination):
    capture = json.loads(source.read_text())
    if capture["status"] != "complete" or not capture["bot"]:
        raise ValueError("Expected a complete isolated bot trial")
    events = capture["motion_uplink"]
    starts = [row for row in events if row["event"] == "client_motion_initial"]
    frames = [row for row in events if row["event"] == "client_post_movement"]
    pre = [row for row in events if row["event"] == "client_pre_movement"]
    ticks = [row["server_tick"] for row in frames]
    if len(starts) != 1 or not ticks or ticks != [row["server_tick"] for row in pre] or ticks != list(range(ticks[0], ticks[-1] + 1)):
        raise ValueError("Missing or out-of-order bot prediction ticks")
    if any(start["monotonic_ns"] >= end["monotonic_ns"] for start, end in zip(pre, frames, strict=True)):
        raise ValueError("Prediction phases reversed")
    impulses = {row["server_tick"]: dict(zip("xyz", row["velocity"], strict=True))
                for row in capture["motion_transport"] if row["event"] == "motion_apply"}
    feedback = [{**{k: row[k] for k in ("sprinting", "movement_speed")},
                 **({"velocity_impulse": impulses[tick]} if tick in impulses else {})}
                for row, tick in zip(pre, ticks, strict=True)]
    inputs = [{"keys": {**row["keys"], "sprint": True}} for row in frames]
    initial = motion(starts[0]["state"])
    initial.update(input={}, sprint_trigger=0, crouching=False, attack_ticks=0)
    destination.mkdir(parents=True, exist_ok=False)
    scenario = destination / "scenario.json"
    initial_file = destination / "initial.json"
    measured = destination / "measured-feedback.json"
    expected = destination / "client.ndjson"
    scenario.write_text(json.dumps({"schema": 1, "id": "moving_bot", "floor_y": initial["position"]["y"],
                                    "floor_friction": 0.6, "actions": inputs}))
    initial_file.write_text(json.dumps(initial))
    measured.write_text(json.dumps(feedback))
    with expected.open("w") as output:
        def emit(row): output.write(json.dumps({"schema": 1, **row}) + "\n")
        emit({"event": "manifest", "scenario": "moving_bot", "engine": "isolated-paper-bot-prediction",
              "scenario_sha256": sha256(scenario), "initial_sha256": sha256(initial_file), "expected_samples": len(frames)})
        for step, row in enumerate(frames):
            emit({"event": "state", "step": step, "phase": "post_movement", "actor": 0, "channel": "local",
                  "server_tick": row["server_tick"], "logical_ns": step * 50_000_000, "state": motion(row["state"])})
        emit({"event": "end", "status": "complete", "samples": len(frames), "dropped": 0})
    subprocess.run([str(ROOT / "target/debug/examples/parity_motion.exe"), str(destination), str(measured),
                    "--physics-after-receive"], check=True)
    report = compare(expected, destination / "rust-with-feedback.ndjson")
    sent = {row["sequence"]: row for row in events if row["event"] == "movement_send"}
    applied = [row for row in events if row["event"] == "movement_apply"]
    handler_matches = sum(row["position_after"] == sent[row["sequence"]]["position"] for row in applied
                          if sent[row["sequence"]]["has_position"])
    report.update(scope="bot-prediction-from-recorded-motion-inputs", capture_sha256=sha256(source),
                  prediction_ticks=len(frames), received_motion_packets=len(impulses),
                  applied_sprint_flags=sum("sprinting" in row["value"] for row in events if row["event"] == "self_feedback_applied"),
                  applied_speed_updates=sum("movement_speed" in row["value"] for row in events if row["event"] == "self_feedback_applied"),
                  server_position_packets=sum(sent[row["sequence"]]["has_position"] for row in applied),
                  server_position_exact=handler_matches)
    (destination / "difference.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    result = replay(args.capture, args.output)
    print(json.dumps({k: result[k] for k in ("passed", "prediction_ticks", "received_motion_packets",
                                            "applied_sprint_flags", "applied_speed_updates",
                                            "server_position_packets", "server_position_exact")}))
    raise SystemExit(0 if result["passed"] else 1)
