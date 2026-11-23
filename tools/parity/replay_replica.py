"""Replay ordinary-client replica packets on the recorded client clock."""
import argparse
import json
from pathlib import Path
import subprocess

from analyze_combat import validate
from extract_impulse import OBSERVATION_SCOPE, read_telemetry
from provenance import ROOT


def downstream(row):
    data, event = row["data"], row["event"]
    fields = data["packet"]
    match event:
        case "remote_relative_applied":
            return {"kind": "relative_move", "entity": 1, "delta": fields.get("delta"),
                    "rotation": fields.get("rotation"), "on_ground": fields["on_ground"]}
        case "remote_position_sync_applied":
            return {"kind": "position_sync", "entity": 1, **fields}
        case "remote_metadata_applied":
            return {"kind": "metadata", "entity": 1, "pose": data["state"]["pose"],
                    "sprinting": data["state"]["sprinting"]}
        case "remote_motion_applied":
            return {"kind": "motion", "entity": 1, "velocity": fields["decoded_velocity"]}
        case "remote_post_tick": return None
        case _: raise ValueError(f"Unsupported remote event {event}")


def schedule(server, telemetry, target="opponent"):
    states, hits = validate(server, scripted=True)
    if telemetry[0]["scope"] != OBSERVATION_SCOPE or server["bot"]:
        raise ValueError("Expected a two-player trial with replica telemetry")
    start, end = states[0]["epoch_ms"], states[-1]["epoch_ms"]
    entity = server["player_connections"][target]["entity_id"]
    rows = [r for r in telemetry[1:] if r["event"].startswith("remote_") and r["data"]["entity_id"] == entity
            and start <= r["epoch_ms"] <= end]
    combat_start = next((hit["epoch_ms"] for hit in hits if not hit["cancelled"]), end)
    initial = next((r for r in rows if r["event"] == "remote_post_tick" and r["epoch_ms"] < combat_start and
                    r["data"]["packet"] == {"interpolating_position": False, "interpolating_velocity": False}), None)
    if initial is None:
        raise ValueError("No settled reference replica before combat")
    body = {k: v for k, v in initial["data"]["state"].items() if k != "jump_delay"}
    body.update(attack_ticks=0, cached_speed=0.1)
    rows = [r for r in rows if r["sequence"] > initial["sequence"]]
    events = [{"time_ns": r["monotonic_ns"] - initial["monotonic_ns"], "packet": downstream(r)} for r in rows]
    if len([e for e in events if e["packet"] is None]) < 30 or not any(
            e["packet"] and e["packet"]["kind"] in ("position_sync", "relative_move") for e in events):
        raise ValueError("Incomplete replica movement coverage")
    return {"schema": 1, "initial": body, "position_base": initial["data"]["position_codec_base"], "events": events}, rows


def compare(case, rows, samples):
    first = None
    maximum = {}
    ages = {}
    previous = case["initial"]["position"]
    for event, row, sample in zip(case["events"], rows, samples, strict=True):
        measured = row["data"]["state"]
        if packet := event["packet"]:
            channel = {"motion": "velocity", "relative_move": "position", "position_sync": "position", "metadata": "metadata"}[packet["kind"]]
            ages[channel] = event["time_ns"]
        for channel, when in ages.items():
            if sample[channel + "_age_ns"] != event["time_ns"] - when:
                raise ValueError(f"Rust replica {channel} age differs at sequence {row['sequence']}")
        fields = [(k, measured[k], sample["body"][k], 0) for k in ("sprinting", "on_ground", "pose")]
        fields.extend((k, measured[k], sample["body"][k], 1e-4) for k in ("yaw", "pitch"))
        for key in ("position", "velocity"):
            fields.extend((f"{key}.{axis}", measured[key][axis], sample["body"][key][axis], 1e-6) for axis in "xyz")
        fields.extend((f"base.{axis}", row["data"]["position_codec_base"][axis], sample["position_base"][axis], 0)
                      for axis in "xyz")
        if event["packet"] is None:
            fields.extend((f"displacement.{axis}", measured["position"][axis] - previous[axis],
                           sample["observed_displacement"][axis], 1e-6) for axis in "xyz")
            previous = measured["position"]
        for field, want, have, tolerance in fields:
            if tolerance:
                error = abs((want-have+180) % 360-180) if field in ("yaw", "pitch") else abs(want-have)
                maximum[field] = max(maximum.get(field, 0), error)
                different = error > tolerance
            else:
                different = want != have
            if different and first is None:
                first = {"sequence": row["sequence"], "event": row["event"], "field": field, "minecraft": want, "rust": have}
    return {"schema": 1, "scope": "same-received-replica-packets", "passed": False, "matched_states": first is None,
            "first_difference": first, "maximum_errors": maximum, "events": len(samples),
            "measured_age_channels": sorted(ages)}


def replay(directory, output, trial="charged_player", observer_seat="striker"):
    server = json.loads((directory / trial / "server.json").read_text())
    request = json.loads((directory / trial / "request.json").read_text())
    striker = request.get("striker", "ParityA")
    if observer_seat == "opponent":
        observer = "ParityB" if striker == "ParityA" else "ParityA"
        target = "human"
    else:
        observer, target = striker, "opponent"
    client = read_telemetry(directory / observer / "telemetry.ndjson")
    case, rows = schedule(server, client, target)
    output.mkdir(exist_ok=False)
    path = output / "schedule.json"
    path.write_text(json.dumps(case, indent=2) + "\n")
    result = subprocess.check_output([str(ROOT / "target/debug/examples/parity_replica.exe"), str(path)])
    (output / "rust.json").write_bytes(result)
    report = compare(case, rows, json.loads(result))
    (output / "difference.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--trial", default="charged_player")
    parser.add_argument("--observer-seat", choices=("striker", "opponent"), default="striker")
    args = parser.parse_args()
    result = replay(args.directory, args.output, args.trial, args.observer_seat)
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["passed"] else 1)
