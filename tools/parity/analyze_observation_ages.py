"""Measure normal-client packet ages and the bot observations supplied to its policy."""
import argparse
import json
from pathlib import Path

from analyze_combat import validate
from extract_impulse import OBSERVATION_SCOPE, read_telemetry


def frames(data, client):
    states, _ = validate(data)
    if client[0]["scope"] != OBSERVATION_SCOPE:
        raise ValueError("Missing observation telemetry")
    # The ordinary victim is the observer corresponding to the experimental bot.
    target = data["player_connections"]["human"]["entity_id"]
    start, end = states[0]["epoch_ms"], states[-1]["epoch_ms"]
    packet_channels = {"remote_relative_applied": "position", "remote_position_sync_applied": "position",
                       "remote_motion_applied": "velocity", "remote_metadata_applied": "metadata",
                       "self_health_applied": "health"}
    seen = {}
    observations = []
    for row in client[1:-1]:
        event = row["event"]
        if event == "self_health_applied" or (event.startswith("remote_") and row["data"]["entity_id"] == target):
            if event in packet_channels:
                seen[packet_channels[event]] = row["monotonic_ns"]
            if event == "remote_post_tick" and start <= row["epoch_ms"] <= end:
                observations.append({"sequence": row["sequence"], "client_tick": row["client_tick"],
                                     "ages_ns": {k: row["monotonic_ns"]-v for k, v in seen.items()}})
    if not observations:
        raise ValueError("Missing normal client observation samples")
    return observations


def describe(human, bot):
    fields = {}
    for subject in ("position", "metadata", "velocity", "health"):
        ages_ref = [r["ages_ns"][subject] for r in human if subject in r["ages_ns"]]
        ages_bot = [r["observation"][subject + "_age_ns"] for r in bot if r["phase"] == "bot_action_applied"]
        fields[subject] = {"client_samples": len(ages_ref), "bot_decisions": len(ages_bot),
                           "client_ns": [min(ages_ref), max(ages_ref)] if ages_ref else None,
                           "bot_ns": [min(ages_bot), max(ages_bot)] if ages_bot else None}
    return fields


def analyze(root, player, bot, observer):
    reference = json.loads((root / player / "server.json").read_text())
    candidate = json.loads((root / bot / "server.json").read_text())
    client = read_telemetry(root / observer / "telemetry.ndjson")
    human = frames(reference, client)
    validate(candidate)
    report = {"schema": 1, "scope": "live-observation-age-coverage", "passed": False,
              "age_ranges": describe(human, candidate["bot_observations"]), "ordinary_client_samples": human}
    (root / "observation-difference.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--player-trial", default="charged_player")
    parser.add_argument("--bot-trial", default="charged_bot")
    parser.add_argument("--observer", default="ParityB")
    args = parser.parse_args()
    result = analyze(args.root, args.player_trial, args.bot_trial, args.observer)
    print(json.dumps(result["age_ranges"], indent=2))
    raise SystemExit(0 if result["passed"] else 1)
