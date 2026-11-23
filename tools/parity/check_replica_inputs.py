"""Check the opponent fields consumed by the bot against the same tick's received replica."""

import argparse
from collections import deque
import json
import math
from pathlib import Path

from provenance import sha256


def check_deliveries(stream):
    pending = deque()
    sequence = applied = 0
    for row in stream:
        if row["phase"] == "remote_packet_sent":
            if row["sequence"] == 0:
                if row["packet_class"] != "ClientboundAddEntityPacket":
                    raise ValueError("A tracking session must start with pairing data")
                pending.clear()
                sequence = 0
            if row["sequence"] != sequence:
                raise ValueError("Tracker sent an incomplete or unordered sequence")
            pending.append(row)
            sequence += 1
        elif row["phase"] == "remote_packet_applied":
            if not pending:
                raise ValueError("Tracker applied a packet without a send record")
            sent = pending.popleft()
            if row["sequence"] != sent["sequence"] or row["applied_ns"] < sent["deadline_ns"]:
                raise ValueError("Tracker packet applied out of order or before its deadline")
            applied += 1
    if not applied:
        raise ValueError("No tracker packets were applied")
    return applied


def compare(capture):
    if capture["status"] != "complete" or not capture["bot"]:
        raise ValueError("Expected a complete bot trial")
    stream = capture.get("production_replica") or capture["remote_replica"]
    opponent_source = "tracker_packet_replica" if capture.get("production_replica") else "native_tracker_replica"
    applied = check_deliveries(stream) if opponent_source == "tracker_packet_replica" else None
    frames = {row["server_tick"]: row for row in stream if row["phase"] == "remote_replica_tick"}
    prediction = {row["server_tick"]: row for row in capture.get("motion_uplink", ())
                  if row["event"] == "client_post_movement"}
    samples = [row for row in capture["bot_observations"] if row["phase"] == "bot_sample"]
    if not samples or not frames:
        raise ValueError("Missing native tracker or observation samples")
    first = None
    pos_ages = set()
    self_samples = 0
    for sample in samples:
        tick = sample["server_tick"]
        frame, observed = frames[tick], sample["observation"]
        if frame["monotonic_ns"] >= sample["monotonic_ns"]:
            raise ValueError(f"Tracker frame follows its consumer at tick {tick}")
        relative = [p - sample["self_position"][a] for a, p in zip("xyz", frame["position"], strict=True)]
        displacement = frame["displacement"]
        expected = {
            "opponent_sprinting": frame["sprinting"], "opponent_sneaking": frame["sneaking"],
            "opponent_on_ground": frame["on_ground"],
            "opponent_walking": displacement[0] ** 2 + displacement[2] ** 2 >= 9e-6,
            "opponent_rising": not frame["on_ground"] and displacement[1] > 0,
            "opponent_falling": not frame["on_ground"] and displacement[1] < 0,
            "position_age_ns": frame["position_age_ns"], "metadata_age_ns": frame["metadata_age_ns"],
            "velocity_age_ns": frame.get("velocity_age_ns", frame["velocity_age_since_pairing_ns"]),
        }
        pos_ages.add(observed["position_age_ns"])
        comparisons = {name: (value, observed[name], 0) for name, value in expected.items()}
        comparisons["opponent_source"] = (opponent_source, sample["opponent_source"], 0)
        for name, vector in (("relative_position", relative), ("opponent_velocity", frame["velocity"]),
                             ("opponent_displacement", displacement)):
            for axis, value in zip("xyz", vector, strict=True):
                comparisons[f"{name}.{axis}"] = (value, observed[name][axis], 1e-6)
        if prediction:
            if sample.get("self_motion_source") != "client_prediction":
                raise ValueError("Expected the bot to observe its independent client motion")
            row = prediction[tick - 1]
            if row["monotonic_ns"] >= sample["monotonic_ns"]:
                raise ValueError("The bot consumed a local state before it was available")
            own = row["state"]
            self_samples += 1
            for axis, pos, velocity in zip("xyz", own["position"], own["velocity"], strict=True):
                comparisons[f"self_position.{axis}"] = (pos, sample["self_position"][axis], 1e-6)
                comparisons[f"velocity.{axis}"] = (velocity, observed["velocity"][axis], 1e-6)
            comparisons["on_ground"] = (own["on_ground"], observed["on_ground"], 0)
            comparisons["sprinting"] = (own["sprinting"], observed["sprinting"], 0)
            comparisons["sneaking"] = (own["pose"] == "crouching", observed["sneaking"], 0)
            if "health" in row:
                comparisons["health"] = (row["health"], observed["health"], 1e-6)
                comparisons["health_age_ns"] = (row["health_age_ns"] + 50_000_000, observed["health_age_ns"], 0)
                comparisons["attack_charge"] = (row["attack_charge"], observed["attack_charge"], 1e-6)
        if sample.get("recurrent_input") is not None:
            extra = sample["recurrent_input"]
            comparisons["opponent_health"] = (frame["health"], extra["opponent_health"], 1e-6)
            comparisons["opponent_health_age_ns"] = (frame["health_age_ns"], extra["opponent_health_age_ns"], 0)
        for name, (want, got, tolerance) in comparisons.items():
            equal = math.isclose(want, got, rel_tol=0, abs_tol=tolerance) if tolerance else want == got
            if not equal and first is None:
                first = {"server_tick": tick, "field": name, "replica": want, "observation": got, "tolerance": tolerance}
    return {"schema": 1, "scope": "same-tick-remote-fields-consumed-by-bot", "source": opponent_source, "passed": first is None,
            "samples": len(samples), "applied_packets": applied, "self_prediction_samples": self_samples,
            "first_difference": first, "position_ages_ns": sorted(pos_ages)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    args = parser.parse_args()
    report = compare(json.loads(args.capture.read_text()))
    report["capture_sha256"] = sha256(args.capture)
    args.capture.with_name("replica-input-difference.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))
    raise SystemExit(0 if report["passed"] else 1)
