"""Strict, phase-aligned comparison. Missing evidence is a failure, never a pass."""
import argparse
import hashlib
import json
import math
from pathlib import Path

SCHEMA = 1
BOOLEAN_FIELDS = {"on_ground", "sprinting"}
POSES = ("standing", "crouching")
FIELDS = {
    "motion": {
        "position.x", "position.y", "position.z", "velocity.x", "velocity.y", "velocity.z",
        "yaw", "pitch", "on_ground", "sprinting", "pose", "fall_distance", "jump_delay",
    },
    "combat": {"health", "attack_charge", "attack_ticks", "hurt_ticks", "invulnerable_ticks",
               "sword", "armor.0", "armor.1", "armor.2", "armor.3"},
}


def flatten(value, prefix=""):
    if isinstance(value, dict):
        for key, child in value.items():
            yield from flatten(child, f"{prefix}.{key}" if prefix else key)
    elif isinstance(value, list):
        for key, child in enumerate(value):
            yield from flatten(child, f"{prefix}.{key}")
    else:
        yield prefix, value


def load(path):
    records = [json.loads(line) for line in Path(path).read_text(encoding="utf-8-sig").splitlines() if line.strip()]
    if len(records) < 3 or any(r.get("schema") != SCHEMA for r in records):
        raise ValueError("Missing records or unsupported schema")
    head, tail = records[0], records[-1]
    if head.get("event") != "manifest" or tail.get("event") != "end":
        raise ValueError("Missing manifest/end: incomplete capture")
    if tail.get("status") != "complete" or tail.get("dropped") != 0:
        raise ValueError("Incomplete or dropped capture")
    directory = Path(path).parent
    for filename, field in (("scenario.json", "scenario_sha256"), ("initial.json", "initial_sha256")):
        source = directory / filename
        if source.exists() and hashlib.sha256(source.read_bytes()).hexdigest() != head.get(field):
            raise ValueError(f"Capture input hash mismatch: {filename}")
    states = [r for r in records[1:-1] if r.get("event") == "state"]
    if len(states) != len(records) - 2:
        raise ValueError("Unsupported event type: use an event-specific comparator; events cannot be ignored")
    if not states or len(states) != tail.get("samples") or len(states) != head.get("expected_samples"):
        raise ValueError("Expected/sample/end counts differ")
    keys = [key(r) for r in states]
    if len(keys) != len(set(keys)):
        raise ValueError("Duplicate state keys")
    # A complete file cannot silently omit a whole tick.
    streams = {}
    for r in states:
        if type(r.get("logical_ns")) is not int:
            raise ValueError(f"Expected integer logical_ns at {key(r)}")
        streams.setdefault((r["actor"], r["channel"], r["phase"]), []).append(r["step"])
    for steps in streams.values():
        if steps != list(range(len(steps))):
            raise ValueError("Noncontiguous or unordered steps")
    return head, states


def key(record):
    return tuple(record[field] for field in ("step", "phase", "actor", "channel"))


def difference(name, a, b):
    if a is None or b is None:
        return False, None, 0
    if isinstance(a, bool) or isinstance(b, bool):
        return type(a) is type(b) and a == b, None, 0
    if isinstance(a, (float, int)) and isinstance(b, (float, int)):
        if not math.isfinite(a) or not math.isfinite(b):
            return False, None, 0
        error = abs(a - b)
        leaf = name.split(".")[-1]
        if leaf in ("yaw", "pitch"):
            error = abs((a - b + 180) % 360 - 180)
            tolerance = 1e-4
        elif name.startswith(("hidden.", "logits.")):
            tolerance = 1e-4
        elif name.startswith(("position.", "velocity.", "features.")) or leaf in (
                "health", "attack_charge", "damage", "fall_distance"):
            tolerance = 1e-6
        else:
            tolerance = 0
        return error <= tolerance, error, tolerance
    return type(a) is type(b) and a == b, None, 0


def compare(reference, candidate, profiles=("motion",)):
    result = {"schema": SCHEMA, "passed": False, "profiles": list(profiles),
              "reference": str(reference), "candidate": str(candidate),
              "failures": [], "max_error": {}, "first_difference": None}
    try:
        ah, aa = load(reference)
        bh, bb = load(candidate)
        for field in ("scenario", "scenario_sha256", "initial_sha256", "expected_samples"):
            if field not in ah or ah[field] != bh.get(field):
                raise ValueError(f"Manifest mismatch: {field}")
        required = set().union(*(FIELDS[p] for p in profiles))
        if not required:
            raise ValueError("No required coverage")
        if [key(r) for r in aa] != [key(r) for r in bb]:
            raise ValueError("Missing, extra, or reordered phase/actor/channel samples")
        count = 0
        for index, (a, b) in enumerate(zip(aa, bb)):
            av, bv = dict(flatten(a["state"])), dict(flatten(b["state"]))
            missing = sorted(required - av.keys() | required - bv.keys())
            if missing:
                raise ValueError(f"Unmeasured required fields at {key(a)}: {missing}")
            for side, values in (("reference", av), ("candidate", bv)):
                for field in sorted(required):
                    value = values[field]
                    if field in BOOLEAN_FIELDS:
                        valid = type(value) is bool
                    elif field == "pose":
                        valid = value in POSES
                    else:
                        valid = type(value) in (int, float) and math.isfinite(value)
                    if not valid:
                        raise ValueError(f"Invalid required field {field} in {side} at {key(a)}")
            # Compare ALL recorded fields, not only the minimum coverage.
            for field in sorted(av.keys() | bv.keys()):
                ok, error, tolerance = difference(field, av.get(field), bv.get(field))
                if error is not None:
                    result["max_error"][field] = max(result["max_error"].get(field, 0), error)
                if not ok:
                    count += 1
                    if result["first_difference"] is None:
                        result["first_difference"] = {
                            "key": key(a), "field": field, "reference": av.get(field),
                            "candidate": bv.get(field), "absolute_error": error, "tolerance": tolerance,
                            "context_reference": aa[max(0, index - 2):index + 3],
                            "context_candidate": bb[max(0, index - 2):index + 3],
                        }
            if a["logical_ns"] != b["logical_ns"]:
                raise ValueError(f"Logical time mismatch at {key(a)}")
        result["compared_samples"] = len(aa)
        result["different_values"] = count
        result["passed"] = count == 0
    except (ValueError, KeyError, TypeError, OverflowError, OSError) as error:
        result["failures"].append(str(error))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--profile", choices=FIELDS, action="append")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    report = compare(args.reference, args.candidate, args.profile or ["motion"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(json.dumps({k: report[k] for k in ("passed", "failures", "profiles")}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
