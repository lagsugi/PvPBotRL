"""Check a scripted bot click through the local target selection and Paper handlers."""

import argparse
import json
from pathlib import Path

from provenance import sha256


def compare(capture, delay_ms, expect_hit):
    if capture["status"] != "complete" or not capture["bot"]:
        raise ValueError("Expected a complete bot trial")
    trace = capture["attack_uplink"]
    picks = [r for r in trace if r["phase"] == "client_pick"]
    sends = [r for r in trace if r["phase"] == "attack_uplink_sent"]
    returns = [r for r in trace if r["phase"] == "attack_uplink_handler_return"]
    hits = [r for r in capture["events"] if r["event"] == "damage" and r["attacker"] == "opponent"]
    packets = ["ServerboundAttackPacket", "ServerboundSwingPacket"] if expect_hit else ["ServerboundSwingPacket"]
    failures = []
    if len(picks) != 1 or picks[0]["selected"] is not expect_hit:
        failures.append("client_pick")
    if [r["packet"] for r in sends] != packets or [r["packet"] for r in returns] != packets:
        failures.append("packet_order_or_coverage")
    if len(sends) == len(returns):
        for seq, (sent, returned) in enumerate(zip(sends, returns, strict=True)):
            if (sent["sequence"] != seq or returned["sequence"] != seq or
                    sent["deadline_ns"] - sent["send_ns"] != delay_ms * 1_000_000 or
                    returned["applied_ns"] < sent["deadline_ns"] or
                    returned["monotonic_ns"] < sent["monotonic_ns"]):
                failures.append(f"packet_timing.{seq}")
    if len(hits) != int(expect_hit) or (expect_hit and (hits[0]["cancelled"] or not sends or not returns or
            hits[0]["server_tick"] != returns[0]["server_tick"] or
            not sends[0]["monotonic_ns"] <= hits[0]["monotonic_ns"] <= returns[0]["monotonic_ns"])):
        failures.append("paper_damage_acceptance")
    return {"schema": 1, "scope": "single-bot-click-attack-uplink", "passed": not failures,
            "failures": failures, "picks": picks, "sends": sends, "handler_returns": returns, "damage": hits}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    parser.add_argument("delay_ms", type=int)
    parser.add_argument("--miss", action="store_true")
    args = parser.parse_args()
    result = compare(json.loads(args.capture.read_text()), args.delay_ms, not args.miss)
    result["capture_sha256"] = sha256(args.capture)
    args.capture.with_name("attack-uplink-difference.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": result["passed"], "failures": result["failures"]}))
    raise SystemExit(0 if result["passed"] else 1)
