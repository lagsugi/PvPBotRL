"""Extract a received impulse and subsequent idle client physics, without shifting any ticks.

This is a component fixture starting AFTER vanilla packet application, not a
claim about full network scheduling or the original attack's implementation.
"""
import argparse
import hashlib
import json
from pathlib import Path

from analyze_combat import finite

MOTION_FIELDS = ("position", "velocity", "yaw", "pitch", "on_ground", "sprinting", "pose", "fall_distance", "jump_delay")
LEGACY_SCOPE = "self-motion-application"
UPLINK_SCOPE = "self-motion-application-and-movement-submission"
OBSERVATION_SCOPE = "client-motion-and-observations"
OBSERVATION_EVENTS = {"remote_motion_applied", "remote_relative_applied", "remote_position_sync_applied",
                      "remote_metadata_applied", "remote_post_tick", "self_health_applied", "client_pick", "attack_input_pick"}
OBSERVATION_EVENTS.update(("self_attributes_applied", "self_metadata_applied", "local_pre_movement"))


def read_telemetry(source):
    rows = [json.loads(line) for line in source.read_text().splitlines()]
    for row in rows: finite(row)
    if len(rows) < 3: raise ValueError("Empty telemetry")
    scope = rows[0].get("scope")
    if rows[0].get("schema") != 1 or scope not in (LEGACY_SCOPE, UPLINK_SCOPE, OBSERVATION_SCOPE):
        raise ValueError("Unsupported telemetry manifest")
    if rows[-1].get("event") != "end" or rows[-1]["data"] != {"status": "complete", "dropped": 0}:
        raise ValueError("Incomplete telemetry")
    if sum(r.get("event") == "end" for r in rows) != 1:
        raise ValueError("Multiple telemetry end markers")
    if [r["sequence"] for r in rows[1:]] != list(range(len(rows)-1)):
        raise ValueError("Missing or reordered telemetry records")
    allowed = {"local_post_movement", "self_motion_applied", "end"}
    if scope != LEGACY_SCOPE: allowed.add("movement_submitted")
    if scope == OBSERVATION_SCOPE: allowed.update(OBSERVATION_EVENTS)
    if any(r["schema"] != 1 or type(r["sequence"]) is not int or type(r["client_tick"]) is not int
           or r["event"] not in allowed for r in rows[1:]):
        raise ValueError("Malformed telemetry event")
    if any(a["monotonic_ns"] > b["monotonic_ns"] or a["client_tick"] > b["client_tick"]
           for a, b in zip(rows[1:], rows[2:])):
        raise ValueError("Telemetry clock moved backwards")
    return rows


def extract(source, destination, ticks=35):
    if type(ticks) is not int or ticks < 1: raise ValueError("Expected positive tick count")
    rows = read_telemetry(source)
    impulses = [(i, row) for i, row in enumerate(rows) if row["event"] == "self_motion_applied"
                and abs(row["data"]["decoded_velocity"]["z"]) > 0.1]
    if len(impulses) != 1:
        raise ValueError("Expected exactly one horizontal impulse in this victim session")
    at, packet = impulses[0]
    initial = packet["data"]["state"]
    if initial["velocity"] != packet["data"]["decoded_velocity"]:
        raise ValueError("Packet did not set local velocity")
    if not initial["on_ground"] or initial["pose"] != "standing" or any(initial["input"].values()):
        raise ValueError("Expected stationary grounded input at impulse application")
    samples = []
    for row in rows[at+1:]:
        if row["event"] == "movement_submitted" or row["event"] in OBSERVATION_EVENTS: continue
        if row["event"] != "local_post_movement":
            raise ValueError("Additional packet or end before expected physics samples")
        if row["client_tick"] != packet["client_tick"] + len(samples):
            raise ValueError("Missing client physics tick")
        state = row["data"]["state"]
        if any(state["input"].values()) or state["yaw"] != initial["yaw"] or state["pitch"] != initial["pitch"]:
            raise ValueError("Victim input/aim changed")
        samples.append(row)
        if len(samples) == ticks: break
    if len(samples) != ticks: raise ValueError("Not enough physics samples")
    if abs(samples[-1]["data"]["state"]["position"]["y"] - initial["position"]["y"]) > 1e-6:
        raise ValueError("Victim did not return to flat floor")
    destination.mkdir(parents=True, exist_ok=False)
    scenario = {"schema": 1, "id": "received_knockback_idle", "floor_y": initial["position"]["y"],
                "floor_friction": 0.6, "actions": [{"keys": {}} for _ in samples]}
    serialized = {}
    for name, data in (("scenario", scenario), ("initial", initial)):
        text = json.dumps(data, separators=(",", ":"))
        (destination / (name + ".json")).write_text(text, encoding="utf-8")
        serialized[name] = hashlib.sha256(text.encode()).hexdigest()
    header = {"schema": 1, "event": "manifest", "engine": "minecraft-client-26.1.2",
              "scenario": scenario["id"], "scenario_sha256": serialized["scenario"],
              "initial_sha256": serialized["initial"], "expected_samples": len(samples)}
    with (destination / "client.ndjson").open("w", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(header) + "\n")
        for index, row in enumerate(samples):
            stream.write(json.dumps({"schema": 1, "event": "state", "step": index,
                "phase": "post_movement", "actor": 0, "channel": "local", "logical_ns": index * 50_000_000,
                "wall_ns": row["monotonic_ns"] - packet["monotonic_ns"],
                "state": {field: row["data"]["state"][field] for field in MOTION_FIELDS}}) + "\n")
        stream.write(json.dumps({"schema": 1, "event": "end", "status": "complete", "samples": len(samples), "dropped": 0}) + "\n")
    (destination / "provenance.json").write_text(json.dumps({"schema": 1,
        "scope": "physics-after-received-impulse-only", "source": str(source),
        "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "packet_application": packet,
        "source_sequences": [r["sequence"] for r in samples],
        "assumed_floor_friction": 0.6}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    extract(args.source, args.destination)
