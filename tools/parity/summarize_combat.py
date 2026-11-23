"""Summarize paired combat captures, including missing gates and the first difference."""
import argparse
import json
from pathlib import Path
import subprocess

from analyze_combat import compare_frames, compare_trials, equipment_delta, finite, validate
from analyze_bot_uplink import compare as compare_bot_uplink
from analyze_uplink import correlate
from extract_impulse import read_telemetry
from provenance import ROOT


def summarize(directory):
    summary = json.loads((directory / "summary.json").read_text())
    oracle = json.loads(subprocess.check_output([str(ROOT / "target/debug/examples/parity_attack.exe")], text=True))
    telemetry = {p.name: read_telemetry(p / "telemetry.ndjson") for p in (directory / "ParityA", directory / "ParityB")}
    groups = {}
    for row in summary["results"]:
        key = (row.get("scenario", "charged"), row["striker"], row["repeat"], row["client_uplink_ms"], row["client_downlink_ms"])
        group = groups.setdefault(key, {})
        if row["bot"] in group: raise ValueError(f"Duplicate trial for {key}")
        group[row["bot"]] = row["case"]
    results = []
    for (scenario, striker, repeat, up, down), group in groups.items():
        reference, bot = [json.loads((directory / group[side] / "server.json").read_text()) for side in (False, True)]
        result = {"scenario": scenario, "striker": striker, "repeat": repeat, "uplink_ms": up, "downlink_ms": down}
        if scenario == "charged":
            comparison = compare_trials([reference, bot], oracle)
            result.update({k: comparison[k] for k in ("measured_checks_passed", "attack_tick_matches", "damage_matches_rust",
                "health_matches_rust", "first_state_difference", "first_motion_difference", "final_position_difference",
                "equipment_initially_matches", "equipment_changes_match")})
            try:
                target = "ParityB" if striker == "ParityA" else "ParityA"
                uplink = correlate(reference, telemetry[target])
                first = uplink["first_changed_server_tick_end"]
                result["matched_uplink_packets"] = uplink["matched_packet_prefix"]
                result["movement_tick_end_after_damage_ticks"] = first - uplink["damage_server_tick"] if first is not None else None
            except ValueError as e:
                result["uplink_incomplete"] = str(e)
            if bot.get("adapter_version") == "experimental-motion-uplink-v1":
                try:
                    result["bot_uplink"] = compare_bot_uplink(reference, bot, telemetry[target])
                except ValueError as e:
                    result["bot_uplink_incomplete"] = str(e)
        else:
            result["observations"] = [scripted_trial(d) for d in (reference, bot)]
            states = [validate(d, scripted=True)[0] for d in (reference, bot)]
            frames = compare_frames(states, extended=True)
            result["state_comparison"] = {k: frames[k] for k in ("states_match", "first_state_difference",
                "first_motion_difference", "maximum_errors", "final_position_difference")}
            result["damage_sequence_matches"] = damage_sequence_matches(result["observations"])
        results.append(result)
    report = {"schema": 1, "scope": summary["scope"], "passed": False, "fps_limit": summary["fps_limit"],
              "bot_motion_downlink_ms": summary["motion_downlink_ms"], "trials": results}
    if (directory / "relay.ndjson").exists():
        report["relay"] = relay_summary(directory / "relay.ndjson")
    (directory / "suite-difference.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def damage_sequence_matches(measured):
    exact = ("tick", "cancelled", "attacker")
    for channel in ("damage", "fall_damage"):
        attack, bot = (trial[channel] for trial in measured)
        if len(attack) != len(bot):
            return False
        for a, b in zip(attack, bot):
            if any(a.get(k) != b.get(k) for k in exact) or any(abs(a[k]-b[k]) > 1e-6 for k in ("raw", "final_damage")):
                return False
    return True


def relay_summary(source):
    rows = [json.loads(line) for line in source.read_text().splitlines()]
    if len(rows) < 3 or rows[0].get("event") != "manifest" or rows[-1] != {
            "schema": 1, "event": "end", "forwarded": len(rows)-2}:
        raise ValueError("Incomplete TCP relay log")
    groups = {}
    for i, row in enumerate(rows[1:-1]):
        received, deadline, applied = (row[k] for k in ("received_ns", "deadline_ns", "forwarded_ns"))
        if row["sequence"] != i or not received <= deadline <= applied:
            raise ValueError("Invalid relay event timeline")
        group = groups.setdefault((row["direction"], deadline-received), {"count": 0, "maximum_lateness_ns": 0})
        group["count"] += 1
        group["maximum_lateness_ns"] = max(group["maximum_lateness_ns"], applied-deadline)
    return [{"direction": direction, "requested_ns": requested, **group} for (direction, requested), group in sorted(groups.items())]


def scripted_trial(data):
    states, _ = validate(data, scripted=True)
    tick = states[0]["server_tick"]
    return {"bot": data["bot"], "damage": [{"tick": r["server_tick"] - tick,
                                          **{k: r[k] for k in ("raw", "final_damage", "cancelled", "attacker")}}
                                          for r in data["events"] if r["event"] == "damage"],
            "fall_damage": [{"tick": r["server_tick"] - tick, **{k: r[k] for k in ("raw", "final_damage", "cancelled")}}
                             for r in data["events"] if r["event"] == "fall_damage" and r["server_tick"] >= tick],
            "bot_clicks": sum(r["event"] == "bot_click" for r in data["events"]),
            "attack_packets": {actor: sum(r["event"] == "attack_received" for r in c["events"])
                               for actor, c in data["player_connections"].items()},
            "equipment_changes": equipment_delta(states), "final_health": [states[-1][a]["health"] for a in ("human", "opponent")]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    result = summarize(parser.parse_args().directory)
    print(json.dumps({"paired_trials": len(result["trials"]), "passed": result["passed"]}))
    raise SystemExit(0 if result["passed"] else 1)
