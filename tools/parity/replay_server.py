"""Supply the measured Paper handler order and inputs to Rust's authoritative Duel."""
import argparse
import json
from pathlib import Path
import subprocess

from analyze_combat import validate
from provenance import ROOT, sha256

ACTORS = ("human", "opponent")
KEYS = dict.fromkeys(("forward", "backward", "left", "right", "jump", "sneak", "sprint"), False)
ITEMS = {"head": ("helmet", 363), "chest": ("chestplate", 528), "legs": ("leggings", 495),
         "feet": ("boots", 429), "main_hand": ("sword", 1561)}


def vec(values):
    return dict(zip("xyz", values, strict=True))


def equipment(state):
    remaining = {}
    for slot, (item, max_damage) in ITEMS.items():
        measured = state["equipment"][slot]
        if measured["material"] == "minecraft:air":
            remaining[slot] = 0
        elif measured["material"] == "minecraft:diamond_" + item and measured["count"] == 1:
            expected = {} if item == "sword" else {"minecraft:protection": 4}
            if measured["enchantments"] != expected or not 0 <= measured["damage"] < max_damage:
                raise ValueError(f"Unsupported diamond item in {slot}")
            remaining[slot] = max_damage - measured["damage"]
        else:
            raise ValueError(f"Unsupported equipment in {slot}")
    return {"sword": remaining.pop("main_hand"), "armor": list(remaining.values())}


def body(state):
    return {"position": vec(state["position"]), "velocity": vec(state["velocity"]),
            "yaw": state["rotation"][0], "pitch": state["rotation"][1], "pose": state["pose"],
            "on_ground": state["on_ground"], "sprinting": state["sprinting"], "attack_ticks": state["attack_ticks"],
            "fall_distance": state["fall_distance"], "cached_speed": state["attributes"]["movement_speed"]}


def initialize(state):
    attrs = state["attributes"]
    items = equipment(state)
    return {"body": body(state), "health": state["health"], "equipment": items,
            "armor_attribute": attrs["armor"], "toughness_attribute": attrs["armor_toughness"],
            "attack_damage": attrs["attack_damage"], "attack_speed": attrs["attack_speed"],
            "last_hurt": state["last_hurt"], "invulnerable_ticks": state["invulnerable_ticks"],
            "hurt_ticks": state["hurt_ticks"], "known_movement": vec(state["known_movement"]),
            "hurt_marked": False, "needs_sync": False, "metadata_dirty": False, "received_movement": False,
            "input": KEYS, "loaded": True, "first_good": vec(state["position"]), "last_good": vec(state["position"]),
            "move_packets": 0, "teleport_id": 0, "awaiting_teleport": None, "last_sword_present": items["sword"] > 0}


def move_state(value):
    return {**value, "last_move_event_position": vec(value["last_move_event_position"])}


def upstream(event, target_entity_id):
    payload = event["packet"]
    match event["event"]:
        case "movement_received":
            if payload["horizontal_collision"]: raise ValueError("Unexpected horizontal collision")
            return {"kind": "move", "position": vec(payload["position"]) if "position" in payload else None,
                    "rotation": payload.get("rotation"), "on_ground": payload["on_ground"]}
        case "attack_received":
            if payload["target_entity_id"] != target_entity_id: raise ValueError("Unexpected attack target")
            return {"kind": "attack"}
        case "input_received": return {"kind": "input", "keys": payload}
        case "swing_received" if payload["hand"] == "MAIN_HAND": return {"kind": "swing"}
        case "client_tick_end_received": return {"kind": "tick_end"}
        case "command_received" if payload["action"] in ("START_SPRINTING", "STOP_SPRINTING"):
            return {"kind": "sprint", "enabled": payload["action"] == "START_SPRINTING"}
        case _: raise ValueError(f"Unsupported packet {event}")


def schedule(data, server_rules="paper_flat_v1"):
    states, _ = validate(data, scripted=True)
    if data["bot"]: raise ValueError("A normal-client reference is required")
    floor_y = states[0]["human"]["position"][1]
    connection = next(r for r in data["events"] if r["event"] == "initial").get("connection_states", {})
    teleports = [e for e in data["events"] if e["event"] == "fall_teleport"]
    if teleports:
        if len(teleports) != 1: raise ValueError("Multiple external teleports in a fall capture")
        boundary = next((i for i, row in enumerate(states) if row["monotonic_ns"] > teleports[0]["monotonic_ns"]
                         and row["opponent"]["position"][1] > floor_y and "connection_states" in row), None)
        if boundary is None: raise ValueError("Missing airborne state after the setup teleport")
        states = states[boundary:]
        connection = states[0]["connection_states"]
    start = states[0]["monotonic_ns"]
    starts = [s for s in data["tick_starts"] if s["monotonic_ns"] > start]
    if [s["server_tick"] for s in starts] != [s["server_tick"] for s in states[1:]]:
        raise ValueError("Missing Paper tick start timestamps")
    events = [(s["monotonic_ns"], {"phase": "tick"}, {}) for s in starts]
    events.extend((s["monotonic_ns"], {"phase": "sample"}, {a: s[a] for a in ACTORS}) for s in states[1:])
    connections = data["player_connections"]
    if any(c["dropped"] or not c["installed"] for c in connections.values()):
        raise ValueError("Incomplete packet receipts")
    seen = set()
    for handler in data["packet_handlers"]:
        if handler["monotonic_ns"] < start: continue
        actor = handler["actor"]
        receipt = handler["receipt_sequence"]
        pair = (actor, receipt)
        if receipt < 0 or pair in seen:
            raise ValueError(f"Paper packet handler lacks a unique receipt {pair}")
        seen.add(pair)
        i = ACTORS.index(actor)
        packet = connections[actor]["events"][receipt]
        if packet["sequence"] != receipt or packet["monotonic_ns"] > handler["monotonic_ns"]:
            raise ValueError(f"Invalid receipt link {pair}")
        payload = upstream(packet, connections[ACTORS[1-i]]["entity_id"])
        expected = {actor: dict(handler["state"])} if "state" in handler else {}
        if expected and "has_moved" in handler.get("connection_state", {}):
            expected[actor]["connection_state"] = move_state(handler["connection_state"])
        if "target_state" in handler:
            expected[ACTORS[1-i]] = handler["target_state"]
        events.append((handler["monotonic_ns"], {"phase": "packet", "player": i, "packet": payload}, expected))
    if not seen:
        raise ValueError("The Paper packet handler trace is missing")
    events.sort(key=lambda event: event[0])
    if any(a[0] >= b[0] for a, b in zip(events, events[1:])) or events[0][0] < start:
        raise ValueError("Ambiguous Paper event order or inconsistent initial boundary")
    initial = [initialize(states[0][a]) for a in ACTORS]
    case = {"schema": 1, "seed": 0, "initial": initial,
            "config": {"floor_y": floor_y, "server_rules": server_rules,
                       "spawns": [{k: b["body"][k] for k in ("position", "yaw", "pitch")} for b in initial]},
            "events": [{"time_ns": ns-start, **event} for ns, event, _ in events]}
    if "unbreakable" in data:
        case["config"]["unbreakable"] = data["unbreakable"]
    if all("has_moved" in connection.get(a, {}) for a in ACTORS):
        case["paper_moves"] = [move_state(connection[a]) for a in ACTORS]
    if "knockback_random" in data:
        draws = data["knockback_random"]
        if any(d["sequence"] != index for index, d in enumerate(draws)):
            raise ValueError("Missing or reordered knockback draws")
        case["knockback_random"] = [d["value"] for d in draws]
    return case, [expected for _, _, expected in events]


def compare_case(case, expected, observed):
    first = None
    maximum = {}
    attacks = []
    for number, (event, actual, states) in enumerate(zip(case["events"], observed, expected, strict=True)):
        if actual["time_ns"] != event["time_ns"]:
            raise ValueError("Rust returned a different timeline")
        attacks.extend(t for t in actual["trace"] if t["event"]["event"] in ("attack", "attack_rejected"))
        for actor, state in states.items():
            index = ACTORS.index(actor)
            got = actual["server"][index]
            target = body(state)
            fields = [(key, target[key], got["body"][key], 0) for key in ("on_ground", "sprinting", "pose", "attack_ticks")]
            fields.extend((key, state[key], got[key], 0) for key in ("hurt_ticks", "invulnerable_ticks"))
            fields.append(("equipment", equipment(state), got["equipment"], 0))
            fields.append(("attack_charge", state["charge"], actual["attack_charge"][index], 1e-6))
            if "connection_state" in state and actual["paper_moves"] is not None:
                fields.append(("connection_state", state["connection_state"], actual["paper_moves"][index], 0))
            fields.extend((key, state["attributes"][source], got[key], 1e-6) for source, key in (
                ("armor", "armor_attribute"), ("armor_toughness", "toughness_attribute"),
                ("attack_damage", "attack_damage"), ("attack_speed", "attack_speed")))
            fields.extend((key, state[key], got[key], 1e-6) for key in ("health", "last_hurt"))
            fields.append(("fall_distance", target["fall_distance"], got["body"]["fall_distance"], 1e-6))
            for key in ("position", "velocity"):
                fields.extend((f"{key}.{axis}", target[key][axis], got["body"][key][axis], 1e-6) for axis in "xyz")
            fields.extend((f"known_movement.{axis}", want, got["known_movement"][axis], 1e-6)
                          for axis, want in zip("xyz", state["known_movement"], strict=True))
            fields.extend((key, target[key], got["body"][key], 1e-4) for key in ("yaw", "pitch"))
            for field, want, have, tolerance in fields:
                if tolerance:
                    error = abs((want-have+180) % 360-180) if field in ("yaw", "pitch") else abs(want-have)
                    maximum[f"{actor}.{field}"] = max(error, maximum.get(f"{actor}.{field}", 0))
                    different = error > tolerance
                else:
                    different = want != have
                if different and first is None:
                    first = {"event_index": number, "event": event, "actor": actor, "field": field, "paper": want, "rust": have}
    context = []
    if first:
        index = first["event_index"]
        context = [{"event": case["events"][i], "paper_states": expected[i], "rust_states": observed[i]["server"]}
                   for i in range(max(0, index-2), min(len(observed), index+3))]
    return {"schema": 1, "scope": "measured-server-handlers-only", "passed": False, "matched_states": first is None,
            "first_difference": first, "maximum_errors": maximum, "context": context, "rust_attacks": attacks, "events": len(observed)}


def replay(source, destination, server_rules="paper_flat_v1"):
    data = json.loads(source.read_text())
    case, expected = schedule(data, server_rules)
    destination.mkdir(exist_ok=False)
    path = destination / "schedule.json"
    path.write_text(json.dumps(case, indent=2) + "\n")
    output = subprocess.check_output([str(ROOT / "target/debug/examples/parity_server.exe"), str(path)])
    (destination / "rust.json").write_bytes(output)
    rows = json.loads(output)
    report = compare_case(case, expected, rows)
    report["schedule_sha256"] = sha256(path)
    report["reference_sha256"] = sha256(source)
    expected_hits = [e for e in data["events"] if e["event"] == "damage" and not e["cancelled"]]
    hits = [e["event"] for e in report["rust_attacks"] if e["event"].get("result", {}).get("damage", {}).get("accepted")]
    report["damage_matches"] = len(hits) == len(expected_hits) and all(
        ACTORS[got["attacker"]] == want["attacker"] and
        abs(got["result"]["damage"]["raw"] - want["raw"]) <= 1e-6 and
        abs(got["result"]["damage"]["final_damage"] - want["final_damage"]) <= 1e-6
        for got, want in zip(hits, expected_hits))
    first = next(r for r in data["events"] if r["event"] == "state")
    expected_velocities = [[vec(e["packet"]["velocity"]) for e in data["player_connections"][actor]["events"]
                           if e["event"] == "self_motion_outbound" and e["monotonic_ns"] >= first["monotonic_ns"]]
                           for actor in ACTORS]
    actual_velocities = [[], []]
    for row in rows:
        for trace in row["trace"]:
            event = trace["event"]
            if event["event"] == "send_down":
                packet = event["envelope"]["packet"]
                if packet["kind"] == "motion" and packet["entity"] == event["player"]:
                    actual_velocities[event["player"]].append(packet["velocity"])
    report["decoded_impulse_matches"] = expected_velocities == actual_velocities
    expected_fall = [e for e in data["events"] if e["event"] == "fall_damage" and not e["cancelled"]
                     and e["monotonic_ns"] >= first["monotonic_ns"]]
    actual_fall = [trace["event"] for row in rows for trace in row["trace"] if trace["event"]["event"] == "fall_damage"]
    report["fall_damage_matches"] = len(expected_fall) == len(actual_fall) and all(
        want["raw"] == got["damage"]["raw"] and got["damage"]["accepted"] and
        abs(want["final_damage"] - got["damage"]["final_damage"]) <= 1e-6
        for want, got in zip(expected_fall, actual_fall))
    (destination / "difference.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def replay_suite(source, destination, server_rules="paper_flat_v1"):
    destination.mkdir(exist_ok=False)
    trials = []
    for case in sorted(source.glob("*/server.json")):
        data = json.loads(case.read_text())
        if data["bot"]: continue
        output = destination / case.parent.name
        summary = {"trial": case.parent.name}
        try:
            result = replay(case, output, server_rules)
            summary.update({k: result[k] for k in ("matched_states", "damage_matches", "fall_damage_matches", "decoded_impulse_matches",
                                                    "first_difference", "events")})
        except (KeyError, ValueError, subprocess.CalledProcessError) as error:
            summary["incomplete"] = str(error)
        trials.append(summary)
    report = {"schema": 1, "scope": "measured-server-handlers-suite", "passed": False, "trials": trials}
    (destination / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--server-rules", choices=("legacy", "paper_swing", "paper_flat_v1", "paper_flat_v2"), default="paper_flat_v1")
    options = parser.parse_args()
    report = (replay_suite if options.server.is_dir() else replay)(options.server, options.output, options.server_rules)
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report["passed"] else 1)
