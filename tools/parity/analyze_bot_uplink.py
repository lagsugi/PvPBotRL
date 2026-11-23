"""Compare the isolated idle victim's movement packets and their Paper application."""
import argparse
import json
from pathlib import Path

from analyze_combat import finite, validate
from analyze_uplink import correlate, unique
from extract_impulse import read_telemetry


def compare(reference, bot, client):
    human_uplink = correlate(reference, client)
    states, hits = validate(bot)
    if bot["adapter_version"] != "experimental-motion-uplink-v1" or not bot["bot"]:
        raise ValueError("Expected the isolated motion-uplink adapter")
    events = bot["motion_uplink"]
    finite(events)
    sends = [e for e in events if e["event"] == "movement_send"]
    applies = [e for e in events if e["event"] == "movement_apply"]
    if len(sends) + len(applies) != len(events) or not applies:
        raise ValueError("Missing or unsupported movement events")
    if any(row["sequence"] != i for stream in (sends, applies) for i, row in enumerate(stream)):
        raise ValueError("Lost or reordered movement events")
    first_send = unique(sends, lambda r: r["position"] == human_uplink["updates"][0]["received"]["packet"]["position"],
                        "bot movement following the impulse")
    window = len(human_uplink["updates"])
    start = first_send["sequence"]
    if len(applies) < start + window:
        raise ValueError("Incomplete movement application prefix")
    failures = []
    for measured, sent, applied in zip(human_uplink["updates"], sends[start:start+window], applies[start:start+window], strict=True):
        if (sent["phase"] != "after_client_prediction" or applied["phase"] != "before_server_travel" or
                sent["ready_ns"] != applied["ready_ns"] or applied["applied_ns"] < applied["ready_ns"] or
                sent["monotonic_ns"] >= applied["monotonic_ns"]):
            raise ValueError("Movement application phase or deadline invalid")
        packet = measured["received"]["packet"]
        if (not sent["has_position"] or packet["position"] != sent["position"] or packet["on_ground"] != sent["on_ground"] or
                sent["position"] != applied["position_after"] or sent["on_ground"] != applied["on_ground"]):
            failures.append({"reference_packet": packet, "bot_sent": sent, "bot_applied": applied})
    reference_end = reference["events"][-1]["opponent"]["position"]
    return {"schema": 1, "scope": "idle-bot-movement-uplink", "passed": False,
            "matched_packet_prefix": window - len(failures), "packet_prefix_length": window,
            "first_difference": failures[0] if failures else None,
            "human_damage_tick": human_uplink["damage_server_tick"], "bot_damage_tick": hits[0]["server_tick"],
            "human_movement_tick_end": human_uplink["first_changed_server_tick_end"],
            "bot_first_movement_handler_tick": applies[start]["server_tick"],
            "final_position_error": [abs(x-y) for x, y in zip(reference_end, states[-1]["opponent"]["position"], strict=True)]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--player-trial", default="charged_player")
    parser.add_argument("--bot-trial", default="charged_bot")
    args = parser.parse_args()
    directory = args.directory
    reference, bot = (json.loads((directory / trial / "server.json").read_text())
                      for trial in (args.player_trial, args.bot_trial))
    client = read_telemetry(directory / "ParityB/telemetry.ndjson")
    report = compare(reference, bot, client)
    (directory / "bot-uplink-difference.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report["passed"] else 1)
