"""Replay the committed real-client movement captures without launching Minecraft."""
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from compare import compare
from provenance import ROOT

REQUIRED_SCENARIOS = {
    "backward_seat0_0", "diagonal_seat0_0", "held_jump_seat0_0",
    "received_knockback_idle", "sneak_transition_seat0_0", "sprint_feedback",
}


def verify_case(target, *, with_feedback=False, expected_x_error=None):
    runner = ROOT / "target/debug/examples/parity_motion.exe"
    args = [str(runner), str(target)]
    if with_feedback:
        args.append(str(target / "measured-feedback.json"))
    subprocess.run(args, check=True)
    suffix = "-with-feedback" if with_feedback else ""
    result = compare(target / "client.ndjson", target / f"rust{suffix}.ndjson")
    (target / f"difference{suffix}.json").write_text(
        json.dumps(result, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    if expected_x_error is None:
        passed = result["passed"]
    else:
        first = result["first_difference"]
        passed = (not result["passed"] and not result["failures"] and first is not None
                  and first["field"] == "position.x" and first["absolute_error"] is not None
                  and math.isclose(first["absolute_error"], expected_x_error, rel_tol=0, abs_tol=1e-6))
    return {"scenario": target.name, "with_feedback": with_feedback,
            "expected": "match" if expected_x_error is None else "position.x divergence",
            "comparison_passed": result["passed"], "passed": passed}


def main():
    fixture_root = ROOT / "tests/fixtures/parity/motion"
    sources = sorted(path for path in fixture_root.glob("*") if path.is_dir())
    missing = sorted(REQUIRED_SCENARIOS.difference(source.name for source in sources))
    if missing:
        print(json.dumps({"missing_scenarios": missing}), file=sys.stderr)
        return 1

    subprocess.run([str(ROOT / "tools/rust/cargo.bat"), "build", "--locked", "--offline",
                    "-p", "pvp-sim", "--example", "parity_motion"], cwd=ROOT, check=True)
    output_root = ROOT / "build/parity"
    output_root.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="saved-", dir=output_root))
    results = []
    for source in sources:
        target = output / source.name
        shutil.copytree(source, target)
        if source.name == "sprint_feedback":
            results.append(verify_case(target, expected_x_error=0.0017639999189078548))
            results.append(verify_case(target, with_feedback=True))
        else:
            results.append(verify_case(target))
    print(json.dumps({"results": results, "output": str(output)}, indent=2))
    return 0 if results and all(r["passed"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
