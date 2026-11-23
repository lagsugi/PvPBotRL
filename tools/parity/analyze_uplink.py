"""Correlate real local movement, decoded TCP input, and authoritative tick-end states."""
import argparse
import json
from pathlib import Path

from analyze_combat import validate
from extract_impulse import OBSERVATION_SCOPE, UPLINK_SCOPE, read_telemetry


def unique(rows, predicate, description):
    matches = [row for row in rows if predicate(row)]
    if len(matches) != 1:
        raise ValueError(f"Expected one {description}, found {len(matches)}")
    return matches[0]


def vector(value):
    return [value[axis] for axis in "xyz"]


def correlate(server, client, count=8):
    states, hits = validate(server)
    if server["bot"] or client[0]["scope"] not in (UPLINK_SCOPE, OBSERVATION_SCOPE):
        raise ValueError("Expected a human/human trial with movement submissions")
    connections = server["player_connections"]
    for connection in connections.values():
        if (connection["schema"], connection["phase"], connection["installed"], connection["dropped"]) != (
                1, "server-netty-decoded", True, 0):
            raise ValueError("Incomplete connection capture")
        events = connection["events"]
        if any(r["sequence"] != i for i, r in enumerate(events)) or any(
                a["monotonic_ns"] > b["monotonic_ns"] for a, b in zip(events, events[1:])):
            raise ValueError("Missing or reordered connection records")
    attack = unique(connections["human"]["events"], lambda r: r["event"] == "attack_received", "attack packet")
    victim = connections["opponent"]
    if attack["packet"]["target_entity_id"] != victim["entity_id"]:
        raise ValueError("The declared victim differs from the packet target")
    # The two JVMs share the host's wall clock. Use it only to select a multi-second
    # trial window. Packet ordering and timing within each side use its monotonic clock.
    trial_start, trial_end = states[0]["epoch_ms"], states[-1]["epoch_ms"]
    impulse = unique(client, lambda r: r["event"] == "self_motion_applied" and
                     trial_start <= r["epoch_ms"] <= trial_end and
                     abs(r["data"]["decoded_velocity"]["z"]) > 0.1, "applied impulse")
    outbound = unique(victim["events"], lambda r: r["event"] == "self_motion_outbound" and
                      abs(r["packet"]["velocity"][2]) > 0.1, "outbound impulse")
    if impulse["data"]["entity_id"] != victim["entity_id"] or \
            vector(impulse["data"]["decoded_velocity"]) != outbound["packet"]["velocity"]:
        raise ValueError("Outbound and applied impulse differ")
    if not attack["monotonic_ns"] <= hits[0]["monotonic_ns"] <= outbound["monotonic_ns"]:
        raise ValueError("Packet and damage order differs on the server clock")
    local = {r["client_tick"]: r for r in client if r["event"] == "local_post_movement"}
    submitted = [r for r in client if r["event"] == "movement_submitted" and r["sequence"] > impulse["sequence"]
                 and r["data"]["entity_id"] == victim["entity_id"]][:count]
    if len(submitted) != count or any("position" not in r["data"]["packet"] for r in submitted):
        raise ValueError("Missing position updates following the impulse")
    received = [r for r in victim["events"] if r["event"] == "movement_received"]
    start = unique(list(enumerate(received)), lambda pair: pair[1]["packet"] == submitted[0]["data"]["packet"],
                   "receipt of the first position update")[0]
    received = received[start:start + count]
    if len(received) != count:
        raise ValueError("Incomplete received packet prefix")
    updates = []
    for send, receive in zip(submitted, received):
        packet = send["data"]["packet"]
        post = local[send["client_tick"]]
        state = post["data"]["state"]
        if (send["sequence"] >= post["sequence"] or receive["packet"] != packet or
                receive["monotonic_ns"] <= outbound["monotonic_ns"] or
                packet["position"] != vector(state["position"]) or packet["on_ground"] != state["on_ground"]):
            raise ValueError("Packet contents or ordering differ along the observed path")
        visible = next((r for r in states if r["monotonic_ns"] >= receive["monotonic_ns"] and
                        r["opponent"]["position"] == packet["position"] and
                        r["opponent"]["on_ground"] == packet["on_ground"]), None)
        applied = [r for r in server.get("packet_handlers", ()) if r["actor"] == "opponent" and
                   r["receipt_sequence"] == receive["sequence"]]
        if len(applied) > 1 or (applied and (applied[0]["state"]["position"] != packet["position"] or
                                            applied[0]["state"]["on_ground"] != packet["on_ground"])):
            raise ValueError("Packet handler state differs from submitted movement")
        updates.append({"submitted": send, "received": receive,
                        "local_state_sequence": post["sequence"], "handler_return": applied[0] if applied else None,
                        "first_matching_tick_end": visible})
    first_visible = updates[0]["first_matching_tick_end"]
    first_changed = next((r for r in states if r["monotonic_ns"] >= outbound["monotonic_ns"] and
                          r["opponent"]["position"] != states[0]["opponent"]["position"]), None)
    return {"schema": 1, "scope": "single-charged-hit-measured-uplink", "passed": False,
            "acceptance_status": "incomplete", "matched_packet_prefix": len(updates),
            "damage_server_tick": hits[0]["server_tick"],
            "first_moved_server_tick_end": first_visible["server_tick"] if first_visible else None,
            "first_changed_server_tick_end": first_changed["server_tick"] if first_changed else None,
            "attack_received": attack, "impulse_outbound": outbound, "impulse_applied": impulse, "updates": updates}


def analyze(directory):
    server = json.loads((directory / "charged_player/server.json").read_text())
    client = read_telemetry(directory / "ParityB/telemetry.ndjson")
    result = correlate(server, client)
    (directory / "uplink-difference.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    result = analyze(args.directory)
    print(json.dumps({key: result[key] for key in ("matched_packet_prefix", "damage_server_tick",
                                                    "first_moved_server_tick_end", "passed")}, indent=2))
    raise SystemExit(0 if result["passed"] else 1)
