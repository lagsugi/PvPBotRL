"""Preserve the pre-change runtime and fingerprint the exact comparison inputs."""
import argparse
import hashlib
import json
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def sha256(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def runtime_identity(server):
    paths = ("paper-26.1.2-74.jar", "versions/26.1.2/paper-26.1.2.jar", "server.properties",
             "bukkit.yml", "spigot.yml", "config/*.yml", "plugins/PvPBotRL/config.yml", "plugins/*.jar")
    return {p.relative_to(server).as_posix(): sha256(p) for pattern in paths for p in sorted(server.glob(pattern))}


def capture(destination):
    destination.mkdir(parents=True, exist_ok=False)
    files = {}
    run = "paper/run-rl-standalone"
    patterns = (f"{run}/*.jar", f"{run}/plugins/*.jar", "models/bundled/*.json",
                f"{run}/plugins/PvPBotRL/config.yml", f"{run}/config/*.yml",
                f"{run}/bukkit.yml", f"{run}/spigot.yml", "Cargo.lock", "fabric/gradle.properties")
    for pattern in patterns:
        for source in sorted(ROOT.glob(pattern)):
            relative = source.relative_to(ROOT)
            files[relative.as_posix()] = sha256(source)
            target = destination / "files" / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
    manifest = {
        "schema": 1,
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "tracked_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "HEAD"], cwd=ROOT)).hexdigest(),
        "java": subprocess.check_output(["java", "--version"], text=True).strip(),
        "files": files,
        "claim": "Artifact provenance only; this is not evidence of behavioral equivalence.",
    }
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    print(json.dumps({"files": len(capture(args.destination)["files"]), "path": str(args.destination)}))
