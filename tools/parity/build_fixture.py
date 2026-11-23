"""Create the isolated flat Paper fixture from pinned JARs and an existing EULA file."""
import argparse
import json
from pathlib import Path
import shutil

from provenance import ROOT, runtime_identity
from run_motion import CITIZENS_JAR, PAPER_JAR, RL_PLUGIN, Process, clear_motion_arena, loopback_properties


def build(root, eula, cache):
    if not any(line.strip() == "eula=true" for line in eula.read_text().splitlines()):
        raise ValueError("The supplied EULA file does not contain an existing acceptance")
    server = root / "server"
    server.mkdir(parents=True, exist_ok=False)
    cache_dir = server / "cache"
    cache_dir.mkdir()
    shutil.copy2(cache, cache_dir / "mojang_26.1.2.jar")
    shutil.copy2(eula, server / "eula.txt")
    pinned = ROOT / "paper/.gradle/pvpbot-dependencies"
    shutil.copy2(pinned / PAPER_JAR, server / PAPER_JAR)
    plugins = server / "plugins"
    plugins.mkdir()
    shutil.copy2(pinned / CITIZENS_JAR, plugins / CITIZENS_JAR)
    shutil.copy2(RL_PLUGIN, plugins / "PvPBotRL.jar")
    loopback_properties(server)
    process = Process(["java", "-Xms512M", "-Xmx2G", "-jar", PAPER_JAR, "--nogui"], server, root / "setup.log")
    try:
        process.wait_line("Done (", timeout=240)
        clear_motion_arena(process)
    finally:
        process.stop(server=True)
    (root / "runtime-sha256.json").write_text(json.dumps(runtime_identity(server), indent=2) + "\n")
    return server


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--eula", type=Path, required=True)
    parser.add_argument("--vanilla-cache", type=Path, required=True)
    args = parser.parse_args()
    print(build(args.output.resolve(), args.eula, args.vanilla_cache.resolve()))
