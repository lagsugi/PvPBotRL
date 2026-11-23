"""Compare production Citizens decision inputs with the tracker replica in the same match."""

import argparse
import json
import math
from pathlib import Path

from provenance import sha256


def compare(capture):
    if not capture["bot"] or capture["status"] != "complete":
        raise ValueError("Expected a complete bot capture")
    frames = {r["server_tick"]: r for r in capture["remote_replica"] if r["phase"] == "remote_replica_tick"}
    samples = [r for r in capture["bot_observations"] if r["phase"] == "bot_sample"]
    if not samples:
        raise ValueError("Missing bot decision samples")
    comparisons = []
    for sample in samples:
        if sample["opponent_source"] != "delivered_opponent_snapshot":
            raise ValueError("Expected the production opponent observation path")
        tick = sample["server_tick"]
        frame = frames.get(tick)
        if frame is None or frame["monotonic_ns"] >= sample["monotonic_ns"]:
            raise ValueError(f"Missing tracker frame before decision at tick {tick}")
        observed = sample["observation"]
        position = [sample["self_position"][axis] + observed["relative_position"][axis] for axis in "xyz"]
        velocity = [observed["opponent_velocity"][axis] for axis in "xyz"]
        comparisons.append({"server_tick": tick, "snapshot_position": position, "tracker_position": frame["position"],
                            "snapshot_velocity": velocity, "tracker_velocity": frame["velocity"],
                            "position_error": math.dist(position, frame["position"]),
                            "velocity_error": math.dist(velocity, frame["velocity"]),
                            "snapshot_position_age_ns": observed["position_age_ns"],
                            "tracker_position_age_ns": frame.get("position_age_ns"),
                            "snapshot_velocity_age_ns": observed["velocity_age_ns"],
                            "tracker_velocity_age_ns": frame.get("velocity_age_ns")})
    summary = {}
    for channel in ("position", "velocity"):
        key = channel + "_error"
        differences = [row for row in comparisons if row[key] > 1e-6]
        summary[channel] = {"different_samples": len(differences), "first_difference": next(iter(differences), None),
                            "maximum_difference": max(comparisons, key=lambda row: row[key])}
    return {"schema": 1, "scope": "same-match-snapshot-vs-native-tracker-replica", "samples": len(comparisons),
            **summary, "comparisons": comparisons}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    report = compare(json.loads(args.capture.read_text()))
    report["capture_sha256"] = sha256(args.capture)
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
