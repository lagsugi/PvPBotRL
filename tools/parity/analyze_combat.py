"""Compare the captured single-hit trials without shifting the hit/motion timeline."""
import argparse
import json
import math
from pathlib import Path
import subprocess
from provenance import ROOT


def finite(value):
    if value is None or (isinstance(value, float) and not math.isfinite(value)):
        raise ValueError("Null or non-finite measurement")
    if isinstance(value, dict):
        for v in value.values(): finite(v)
    elif isinstance(value, list):
        for v in value: finite(v)


def numeric(value):
    if type(value) not in (int, float): raise ValueError("Expected numeric measurement")
    finite(value)


def equipment_delta(states):
    result = {}
    for actor in ("human", "opponent"):
        first = states[0][actor]["equipment"]
        last = states[-1][actor]["equipment"]
        result[actor] = {slot: {"before": item, "after": last[slot]}
                         for slot, item in first.items() if item != last[slot]}
    return result


def validate(data, scripted=False):
    finite(data)
    charged = {"paper-single-charged-hit", "zonepractice-single-charged-hit"}
    scripted_scopes = {"paper-scripted-combat", "zonepractice-scripted-combat"}
    scopes = charged | scripted_scopes if scripted else charged
    if data.get("schema") != 1 or data.get("status") != "complete" or data.get("scope") not in scopes:
        raise ValueError("Unsupported or incomplete capture")
    events = {"initial", "state", "damage", "velocity_event", "fall_damage"}
    if scripted: events.update(("bot_click", "fall_teleport"))
    for event in data["events"]:
        if event["event"] not in events:
            raise ValueError("Unsupported event")
        for actor in ("human", "opponent", "victim"):
            if actor not in event: continue
            state = event[actor]
            for field in ("position", "velocity"):
                if not isinstance(state[field], list) or len(state[field]) != 3:
                    raise ValueError("Expected three vector components")
                for v in state[field]: numeric(v)
            for field in ("health", "charge", "fall_distance"): numeric(state[field])
            for field in ("hurt_ticks", "invulnerable_ticks"):
                if type(state[field]) is not int: raise ValueError("Expected integer timer")
            for field in ("on_ground", "sprinting"):
                if type(state[field]) is not bool: raise ValueError("Expected boolean flag")
    states = [e for e in data["events"] if e["event"] == "state"]
    ticks = data.get("capture_ticks", 80)
    if type(ticks) is not int or not 60 <= ticks <= 6040:
        raise ValueError("Invalid capture length")
    if [e["tick"] for e in states] != list(range(ticks + 1)):
        raise ValueError("Incomplete state coverage")
    if any(type(e["tick"]) is not int or type(e["server_tick"]) is not int for e in states):
        raise ValueError("Expected integer tick")
    if [e["server_tick"] for e in states] != list(range(states[0]["server_tick"], states[0]["server_tick"] + ticks + 1)):
        raise ValueError("Missing server ticks")
    initials = [e for e in data["events"] if e["event"] == "initial"]
    if len(initials) != 1:
        raise ValueError("Exactly one initial state is required")
    if any(initials[0][actor] != states[0][actor] for actor in ("human", "opponent")):
        raise ValueError("Initial state differs from its tick-zero snapshot")
    hits = [e for e in data["events"] if e["event"] == "damage"]
    if data["scope"] in charged:
        if len(hits) != 1 or hits[0]["cancelled"] is not False or hits[0]["attacker"] != "human":
            raise ValueError("Exactly one accepted human attack is required")
    for hit in hits:
        for field in ("raw", "final_damage"): numeric(hit[field])
        if type(hit["tick"]) is not int or type(hit["server_tick"]) is not int or not (
                states[0]["server_tick"] <= hit["server_tick"] <= states[-1]["server_tick"]):
            raise ValueError("Attack outside capture")
    return states, hits


def compare_frames(states, extended):
    if [frame["tick"] for frame in states[0]] != [frame["tick"] for frame in states[1]]:
        raise ValueError("Combat state ticks differ between trials")
    first = None
    first_state = None
    max_fields = {}
    maximum = 0.0
    exact_attack_timer = any("attack_ticks" in s[0]["human"] for s in states)
    for a, b in zip(*states):
        for actor in ("human", "opponent"):
            for field in ("health", "charge", "fall_distance", "on_ground", "sprinting", "hurt_ticks", "invulnerable_ticks"):
                x, y = a[actor][field], b[actor][field]
                exact = field in ("on_ground", "sprinting", "hurt_ticks", "invulnerable_ticks")
                if (x != y if exact else abs(x-y) > 1e-6) and first_state is None:
                    first_state = {"tick": a["tick"], "actor": actor, "field": field, "reference": x, "bot": y}
            if exact_attack_timer and a[actor]["attack_ticks"] != b[actor]["attack_ticks"] and first_state is None:
                first_state = {"tick": a["tick"], "actor": actor, "field": "attack_ticks",
                               "reference": a[actor]["attack_ticks"], "bot": b[actor]["attack_ticks"]}
            if extended:
                for field in ("equipment", "collidable", "rotation", "attributes", "food"):
                    x, y = a[actor][field], b[actor][field]
                    if field == "rotation":
                        differs = any(abs((p - q + 180) % 360 - 180) > 1e-4 for p, q in zip(x, y, strict=True))
                    elif field == "attributes":
                        differs = x.keys() != y.keys() or any(abs(x[k]-y[k]) > 1e-6 for k in x)
                    elif field == "food":
                        differs = x["level"] != y["level"] or any(abs(x[k]-y[k]) > 1e-6 for k in ("exhaustion", "saturation"))
                    else:
                        differs = x != y
                    if differs and first_state is None:
                        first_state = {"tick": a["tick"], "actor": actor, "field": field, "reference": x, "bot": y}
            for field in ("position", "velocity"):
                for axis, (x, y) in enumerate(zip(a[actor][field], b[actor][field], strict=True)):
                    error = abs(x - y)
                    maximum = max(maximum, error)
                    key = f"{actor}.{field}.{axis}"
                    max_fields[key] = max(max_fields.get(key, 0), error)
                    if error > 1e-6 and first is None:
                        first = {"tick": a["tick"], "actor": actor, "field": field, "axis": axis,
                                 "reference": x, "bot": y, "absolute_error": error}
    return {"states_match": first is None and first_state is None,
              "first_motion_difference": first, "maximum_motion_component_difference": maximum,
              "first_state_difference": first_state, "maximum_errors": max_fields,
              "context": [{"reference": a, "bot": b} for a, b in zip(*states)
                          if first is not None and abs(a["tick"] - first["tick"]) <= 2],
              "final_position_difference": [abs(a-b) for a,b in zip(states[0][-1]["opponent"]["position"],
                                                                   states[1][-1]["opponent"]["position"])]}


def compare_trials(data, oracle):
    finite(oracle)
    validated = [validate(d) for d in data]
    if [d.get("bot") for d in data] != [False, True]: raise ValueError("Expected player then bot trial")
    states = [v[0] for v in validated]
    hits = [v[1] for v in validated]
    expected = oracle["result"]["damage"]["final_damage"]
    damage_ok = all(abs(h[0]["final_damage"] - expected) <= 1e-6 and h[0]["raw"] == 7 for h in hits)
    hp_ok = all(abs(s[-1]["opponent"]["health"] - oracle["health_after"]) <= 1e-6 for s in states)
    hit_ticks = [h[0]["server_tick"] - s[0]["server_tick"] for s, h in zip(states, hits)]
    hit_tick_ok = hit_ticks[0] == hit_ticks[1]
    extended = any("player_connections" in trial for trial in data)
    unexpected_falls = [[e for e in trial["events"] if e["event"] == "fall_damage" and e["server_tick"] >= s[0]["server_tick"]]
                        for trial, s in zip(data, states, strict=True)]
    report = compare_frames(states, extended)
    report.update({"schema": 1, "scope": "single-charged-hit", "passed": False,
              "measured_checks_passed": damage_ok and hp_ok and report["states_match"] and hit_tick_ok and not any(unexpected_falls),
              "unexpected_fall_events": unexpected_falls,
              "acceptance_status": "incomplete", "attack_tick_matches": hit_tick_ok, "attack_ticks": hit_ticks,
              "damage_matches_rust": damage_ok, "health_matches_rust": hp_ok,
              "reference_damage": hits[0][0]["final_damage"], "rust_damage": expected,
              "limitations": ["one repetition", "human attacks only", "network arrivals not controlled",
                              "Rust arithmetic component only; no full Duel replay",
                              "full initialization and timers not measured",
                              "server player velocity is not client prediction velocity"]})
    if extended:
        report["equipment_changes"] = {"reference": equipment_delta(states[0]), "bot": equipment_delta(states[1])}
        report["equipment_initially_matches"] = all(states[0][0][actor]["equipment"] == states[1][0][actor]["equipment"]
                                                    for actor in ("human", "opponent"))
        report["equipment_changes_match"] = report["equipment_changes"]["reference"] == report["equipment_changes"]["bot"]
    else:
        report["limitations"].append("equipment durability not measured")
    return report


def analyze(directory):
    data = [json.loads((directory / case / "server.json").read_text())
            for case in ("charged_player", "charged_bot")]
    oracle = json.loads(subprocess.check_output([str(ROOT / "target/debug/examples/parity_attack.exe")], text=True))
    report = compare_trials(data, oracle)
    (directory / "difference.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("directory", type=Path)
    a = p.parse_args()
    result = analyze(a.directory)
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["passed"] else 1)
