"""Replay a saved local-motion fixture on an isolated Citizens body."""
import argparse
import json
from pathlib import Path
import shutil

from compare import compare
from provenance import sha256
from run_combat import wait_file
from run_motion import PAPER_JAR, RL_PLUGIN, Process, clear_motion_arena, prepare, publish


def main(source, output, measured_feedback=False):
    output.mkdir(parents=True, exist_ok=False)
    serverdir, _ = prepare(output)
    target = output / "fixture"
    target.mkdir()
    files = ["scenario.json", "initial.json", "client.ndjson"]
    if measured_feedback:
        files.append("measured-feedback.json")
    for name in files:
        shutil.copy2(source / name, target / name)
    server = Process(["java", "-Xms512M", "-Xmx2G", f"-Dpvp.parity.root={output}",
                      "-jar", PAPER_JAR, "--nogui"], serverdir, output / "server.log")
    try:
        server.wait_line("Done (")
        clear_motion_arena(server)
        # The shipped test arena has a flat smooth-stone floor at y=64.
        initial = json.loads((target / "initial.json").read_text())
        scenario = json.loads((target / "scenario.json").read_text())
        if scenario["floor_y"] != 65 or scenario["floor_friction"] != 0.6:
            raise ValueError("This isolated arena supports only floor y=65, friction=0.6")
        if abs(initial["position"]["x"]) > 10 or abs(initial["position"]["z"]) > 10:
            raise ValueError("Fixture outside supported central arena")
        temporary = output / "bot-request.tmp"
        temporary.write_text(json.dumps({"directory": "fixture", "measured_feedback": measured_feedback}))
        publish(temporary, output / "bot-request.json")
        wait_file(target / "bot-complete.json", [server])
        report = compare(target / "client.ndjson", target / "bot.ndjson")
        (output / "difference.json").write_text(json.dumps(report, indent=2) + "\n")
        (output / "provenance.json").write_text(json.dumps({"schema": 1, "source": str(source),
            "scope": "saved-local-motion-with-measured-feedback" if measured_feedback else "saved-local-motion-only",
            "plugin_sha256": sha256(RL_PLUGIN), "source_sha256": {name: sha256(source / name) for name in files}}, indent=2))
        print(json.dumps({"passed": report["passed"], "output": str(output)}))
        return report["passed"]
    finally: server.stop(server=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--measured-feedback", action="store_true")
    args = parser.parse_args()
    raise SystemExit(0 if main(args.source.resolve(), args.output.resolve(), args.measured_feedback) else 1)
