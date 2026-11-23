"""Diagnose the experimental motion transport without certifying full-fight timing."""
import argparse
import json
from pathlib import Path
import subprocess

from analyze_combat import finite, validate
from extract_impulse import read_telemetry
from provenance import ROOT


def check_transport(events):
    finite(events)
    if any(e["event"] not in ("motion_send", "motion_apply") for e in events):
        raise ValueError("Unsupported transport event")
    if any(a["server_tick"] > b["server_tick"] for a, b in zip(events, events[1:])):
        raise ValueError("Transport clock moved backwards")
    sends = [e for e in events if e["event"] == "motion_send"]
    applications = [e for e in events if e["event"] == "motion_apply"]
    if not sends or [e["sequence"] for e in sends] != list(range(len(sends))):
        raise ValueError("Missing or reordered sends")
    if [e["sequence"] for e in applications] != [e["sequence"] for e in sends]:
        raise ValueError("Missing, duplicate or reordered applications")
    pending = set()
    for event in events:
        if type(event["sequence"]) is not int or type(event["server_tick"]) is not int:
            raise ValueError("Expected integer sequence and tick")
        if event["event"] == "motion_send": pending.add(event["sequence"])
        else:
            if event["sequence"] not in pending: raise ValueError("Application precedes send")
            pending.remove(event["sequence"])
    for sent, applied in zip(sends, applications):
        for vector in (sent["raw_velocity"], sent["decoded_velocity"], applied["velocity"], applied["before_velocity"]):
            if not isinstance(vector, list) or len(vector) != 3 or any(type(v) not in (float, int) for v in vector):
                raise ValueError("Expected three numeric velocity components")
        if any(type(e[k]) is not int for e, keys in ((sent, ("sent_ns", "ready_ns")),
               (applied, ("applied_ns", "ready_ns"))) for k in keys):
            raise ValueError("Expected integer logical timestamps")
        if sent["sent_ns"] != sent["server_tick"] * 50_000_000 or applied["applied_ns"] != applied["server_tick"] * 50_000_000:
            raise ValueError("Logical clock differs from declared server tick")
        if sent["ready_ns"] < sent["sent_ns"]: raise ValueError("Negative delay")
        if applied["ready_ns"] != sent["ready_ns"] or applied["applied_ns"] < sent["ready_ns"]:
            raise ValueError("Early application or changed deadline")
        if applied["velocity"] != sent["decoded_velocity"] or applied["phase"] != "pre_physics":
            raise ValueError("Packet application differs from decoded velocity")
    return sends, applications


def analyze(directory):
    player, bot = [json.loads((directory / name / "server.json").read_text())
                   for name in ("charged_player", "charged_bot")]
    player_states, _ = validate(player)
    bot_states, _ = validate(bot)
    if bot.get("adapter_version") != "experimental-motion-transport-v1":
        raise ValueError("Not an experimental transport capture")
    sends, applications = check_transport(bot["motion_transport"])
    impulses = [s for s in sends if abs(s["decoded_velocity"][2]) > 0.1]
    rows = read_telemetry(directory / "ParityB/telemetry.ndjson")
    received = [r for r in rows if r["event"] == "self_motion_applied"
                and abs(r["data"]["decoded_velocity"]["z"]) > 0.1]
    if len(impulses) != 1 or len(received) != 1: raise ValueError("Expected one impulse per victim")
    sent = impulses[0]
    applied = next(a for a in applications if a["sequence"] == sent["sequence"])
    decoded = [received[0]["data"]["decoded_velocity"][axis] for axis in ("x", "y", "z")]
    codec_run = subprocess.run([str(ROOT / "target/debug/examples/parity_motion_codec.exe"),
        str(directory / "charged_bot/server.json")], text=True, capture_output=True)
    if codec_run.returncode not in (0, 1) or not codec_run.stdout:
        raise ValueError("Rust codec runner failed: " + codec_run.stderr)
    codec = json.loads(codec_run.stdout)
    final_error = [abs(a-b) for a, b in zip(player_states[-1]["opponent"]["position"], bot_states[-1]["opponent"]["position"])]
    report = {"schema": 1, "scope": "experimental-motion-transport-components", "passed": False,
              "acceptance_status": "incomplete", "rust_codec": codec,
              "decoded_velocity_matches_client": sent["decoded_velocity"] == decoded,
              "final_position_matches": max(final_error) <= 1e-6, "final_position_absolute_error": final_error,
              "send": sent, "application": applied, "client_decoded_velocity": decoded,
              "deadline_wait_ns": sent["ready_ns"] - sent["sent_ns"],
              "physics_poll_lateness_ns": applied["applied_ns"] - sent["ready_ns"],
              "limitations": ["no movement uplink", "physics polls at server tick phase",
                              "full server/predicted velocity separation not implemented", "single stationary charged hit"]}
    (directory / "transport-difference.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    result = analyze(parser.parse_args().directory)
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["passed"] else 1)
