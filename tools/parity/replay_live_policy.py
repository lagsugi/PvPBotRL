"""Verify the frozen model hash, then replay real Paper decisions through Rust."""
import argparse
import json
from pathlib import Path
import subprocess

from provenance import ROOT, sha256

MODELS = {
    "recurrent-ppo": "recurrent_ppo.json",
    "recurrent-g2": "recurrent_g2.json",
    "recurrent-g2-plus": "recurrent_g2_plus.json",
}


def replay(source):
    data = json.loads(source.read_text())
    frames = data["policy_frames"]
    if data["status"] != "complete" or not frames:
        raise ValueError("Missing complete live decisions")
    model = frames[0]["model"]
    path = MODELS[model]
    weights = ROOT / "models/bundled" / path
    digest = sha256(weights)
    if any(frame["model_sha256"] != digest or frame["model"] != model for frame in frames):
        raise ValueError("Live capture used different weights")
    process = subprocess.run([str(ROOT / "target/debug/examples/parity_live_policy.exe"), str(source), str(weights)],
                             capture_output=True, text=True)
    if not process.stdout:
        raise RuntimeError(process.stderr.strip())
    result = json.loads(process.stdout)
    if result["passed"] != (process.returncode == 0):
        raise RuntimeError("Inconsistent Rust inference exit status")
    result.update(model_sha256=digest, reference_sha256=sha256(source))
    source.with_name("policy-difference.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def training_impact(source):
    verified = replay(source)
    if not verified["passed"]:
        raise ValueError("Fix the reference inference mismatch before measuring sensitivity")
    results = {}
    for name, filename in MODELS.items():
        weights = ROOT / "models/bundled" / filename
        report = json.loads(subprocess.check_output([
            str(ROOT / "target/debug/examples/parity_live_policy.exe"), str(source), str(weights), "--training-impact"
        ], text=True))
        results[name] = {**report, "model_sha256": sha256(weights)}
    result = {"schema": 1, "scope": "fixed-observation-trajectory-sensitivity", "source_sha256": sha256(source),
              "reference_frames": verified["frames"], "models": results}
    source.with_name("training-impact.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--training-impact", action="store_true")
    args = parser.parse_args()
    if args.training_impact:
        print(json.dumps(training_impact(args.source), indent=2))
        raise SystemExit(0)
    report = replay(args.source)
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report["passed"] else 1)
