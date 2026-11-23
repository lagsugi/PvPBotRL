"""Run genuine Minecraft client movement against an isolated pinned Paper stack.

This runner certifies only local motion. Combat, bot, and network gates remain separate.
"""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import queue
import shutil
import socket
import subprocess
import threading
import time

from compare import compare
from latency import LatencyRelay
from provenance import ROOT, runtime_identity, sha256
from scenarios import movement_suite, scenario

PAPER_JAR = "paper-26.1.2-74.jar"
CITIZENS_JAR = "Citizens-2.0.44-b4270.jar"
RL_PLUGIN = ROOT / "paper/rlbot/build/libs/pvpbot-rl-0.1.0.jar"


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def publish(temporary, destination):
    # Java's short read of the control file can deny rename sharing on Windows.
    until = time.monotonic() + 5
    while True:
        try:
            temporary.replace(destination)
            return
        except PermissionError:
            if time.monotonic() >= until: raise
            time.sleep(0.02)


class Process:
    def __init__(self, args, cwd, logfile, env=None):
        self.lines = queue.Queue()
        self.process = subprocess.Popen(args, cwd=cwd, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace",
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        def drain():
            with Path(logfile).open("w", encoding="utf-8") as log:
                for line in self.process.stdout:
                    log.write(line); log.flush(); self.lines.put(line)
        threading.Thread(target=drain, daemon=True).start()

    def command(self, text):
        self.process.stdin.write(text + "\n"); self.process.stdin.flush()

    def wait_line(self, match, timeout=150, peer=None):
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            if self.process.poll() is not None:
                raise RuntimeError(f"Process exited: {self.process.returncode}")
            if peer is not None and peer.process.poll() is not None:
                raise RuntimeError(f"Peer process exited: {peer.process.returncode}")
            try:
                line = self.lines.get(timeout=1)
                if match in line: return
            except queue.Empty: pass
        raise TimeoutError(match)

    def stop(self, server=False):
        if self.process.poll() is None:
            if server:
                self.command("stop")
                try: self.process.wait(45)
                except subprocess.TimeoutExpired: self.process.terminate()
            else: self.process.terminate()
            self.process.wait(30)


def loopback_properties(server):
    port = free_port()
    (server / "server.properties").write_text(
        f"server-ip=127.0.0.1\nserver-port={port}\nonline-mode=false\nenforce-secure-profile=false\n"
        "view-distance=3\nsimulation-distance=3\nspawn-protection=0\nallow-flight=true\n"
        "pause-when-empty-seconds=-1\nnetwork-compression-threshold=-1\n"
        "level-type=minecraft:flat\ngenerate-structures=false\nspawn-monsters=false\nspawn-animals=false\n",
        encoding="ascii")
    return port


def clear_motion_arena(server, z_radius=64):
    for z in range(-z_radius, z_radius, 128):
        end_z = min(z + 127, z_radius - 1)
        server.command(f"execute in minecraft:overworld run forceload add -64 {z} 63 {end_z}")
        server.wait_line("force load")
        server.command(f"execute in minecraft:overworld run fill -64 64 {z} 63 64 {end_z} minecraft:smooth_stone")
        for y in (65, 67, 69):
            server.command(f"execute in minecraft:overworld run fill -64 {y} {z} 63 {y + 1} {end_z} minecraft:air")
    server.command("execute in minecraft:overworld run gamerule minecraft:natural_health_regeneration false")
    server.command("say parity-motion-arena-prepared")
    server.wait_line("[Server] parity-motion-arena-prepared")


def prepare(run):
    template = Path(os.environ.get("PVP_PARITY_FIXTURE", ROOT / "build/paper-parity-fixture/server")).resolve()
    server = run / "server"
    server.mkdir()
    # Copy only the existing isolated test fixture, never the running server's world.
    for name in ("world", "config", "libraries", "versions", "cache"):
        if (template / name).exists(): shutil.copytree(template / name, server / name)
    for name in ("eula.txt", PAPER_JAR):
        shutil.copy2(template / name, server / name)
    for name in ("bukkit.yml", "spigot.yml"):
        if (template / name).is_file(): shutil.copy2(template / name, server / name)
    if "eula=true" not in (server / "eula.txt").read_text():
        raise RuntimeError("The isolated fixture has no existing EULA acceptance")
    plugins = server / "plugins"
    plugins.mkdir()
    shutil.copy2(ROOT / "paper/.gradle/pvpbot-dependencies" / CITIZENS_JAR, plugins / CITIZENS_JAR)
    shutil.copy2(RL_PLUGIN, plugins / "PvPBotRL.jar")
    return server, loopback_properties(server)


def run_suite(run, names, full=False, repetitions=1, bot=False, seats=(0, 1), random_only=False, telemetry=False,
              client_delay=None):
    run.mkdir(parents=True, exist_ok=False)
    serverdir, port = prepare(run)
    (run / "runtime-sha256.json").write_text(json.dumps(runtime_identity(serverdir), indent=2) + "\n")
    server = Process(["java", "-Xms512M", "-Xmx2G", f"-Dpvp.parity.root={run}", "-jar", "paper-26.1.2-74.jar", "--nogui"],
                     serverdir, run / "server.log")
    client = None
    resources = ExitStack()
    reports = []
    try:
        server.wait_line("Done (")
        client_port = port if client_delay is None else resources.enter_context(
            LatencyRelay(port, *client_delay, run / "relay.ndjson")).port
        launch = json.loads((ROOT / "fabric/build/parity-launch.json").read_text())
        clientdir = run / "client"
        clientdir.mkdir()
        (clientdir / "options.txt").write_text("pauseOnLostFocus:false\nenableVsync:false\nmaxFps:60\n"
            "renderDistance:2\nsimulationDistance:5\nautoJump:false\nonboardAccessibility:false\n", encoding="utf-8")
        env = dict(os.environ, **launch["environment"])
        # Loom normally writes the classpath argfile in runClient.doFirst; we supply -cp directly.
        jvm_args = [arg for arg in launch["jvmArgs"] if not arg.startswith("@")]
        telemetry_args = [f"-Dpvp.parity.telemetry={clientdir}"] if telemetry else []
        args = ["java", "-Xmx1G", f"-Dpvp.parity.dir={clientdir}", *telemetry_args, *jvm_args,
                "-cp", launch["classpath"], launch["main"], *launch["args"],
                "--username", "ParityA", "--gameDir", str(clientdir), "--width", "320", "--height", "240",
                "--quickPlayMultiplayer", f"127.0.0.1:{client_port}"]
        client = Process(args, clientdir, run / "client.log", env)
        server.wait_line("logged in with entity id", 180, peer=client)
        server.command("op ParityA")
        clear_motion_arena(server)
        server.command("gamemode survival ParityA")
        server.command("effect clear ParityA")
        server.command("team add parity")
        server.command("team modify parity collisionRule never")
        server.command("team join parity ParityA")
        time.sleep(3)
        cases = movement_suite(full)
        if random_only: cases = {n: a for n, a in cases.items() if n.startswith("random_") or n == "long_idle"}
        if names: cases = {n: cases[n] for n in names}
        cases = {f"{name}_seat{seat}": (actions, seat) for name, actions in cases.items() for seat in seats}
        for name, (actions, seat) in cases.items():
            for repeat in range(repetitions):
                id_ = f"{name}_{repeat}"
                server.command(f"execute in minecraft:overworld run tp ParityA 0.5 65 0.5 {seat * 180} 0")
                time.sleep(2)
                request = json.dumps(scenario(id_, actions), separators=(",", ":"))
                temp = clientdir / "request.tmp"
                temp.write_text(request, encoding="utf-8")
                publish(temp, clientdir / "request.json")
                output = clientdir / id_
                until = time.monotonic() + len(actions) / 20 + 45
                while not (output / "complete.json").exists():
                    if client.process.poll() is not None: raise RuntimeError("Reference client exited")
                    if (output / "failed.json").exists(): raise RuntimeError((output / "failed.json").read_text())
                    if time.monotonic() > until: raise TimeoutError(id_)
                    time.sleep(0.25)
                subprocess.run([str(ROOT / "target/debug/examples/parity_motion.exe"), str(output)], check=True, cwd=ROOT)
                report = compare(output / "client.ndjson", output / "rust.ndjson")
                (output / "difference.json").write_text(json.dumps(report, indent=2) + "\n")
                reports.append({"scenario": id_, "passed": report["passed"], "output": str(output)})
                print(json.dumps(reports[-1]), flush=True)
                if bot:
                    # Keep the human far away while the bot replays its captured initial state.
                    server.command("execute in minecraft:overworld run tp ParityA 50 65 50")
                    temp = run / "bot-request.tmp"
                    temp.write_text(json.dumps({"directory": str(output.relative_to(run))}), encoding="utf-8")
                    publish(temp, run / "bot-request.json")
                    until = time.monotonic() + len(actions) / 20 + 45
                    while not (output / "bot-complete.json").exists():
                        if (output / "bot-failed.json").exists(): raise RuntimeError((output / "bot-failed.json").read_text())
                        if time.monotonic() > until: raise TimeoutError("bot " + id_)
                        time.sleep(0.25)
                    report = compare(output / "client.ndjson", output / "bot.ndjson")
                    (output / "bot-difference.json").write_text(json.dumps(report, indent=2) + "\n")
                    reports.append({"scenario": id_, "engine": "bot", "passed": report["passed"], "output": str(output)})
                    print(json.dumps(reports[-1]), flush=True)
        if telemetry:
            (clientdir / "telemetry-stop.json").write_text("{}")
            deadline = time.monotonic() + 10
            while not (clientdir / "telemetry-complete.json").exists():
                if (clientdir / "telemetry-failed.json").exists() or time.monotonic() > deadline:
                    raise RuntimeError("Incomplete client telemetry")
                time.sleep(0.1)
        result = {"schema": 1, "scope": "genuine-client-local-motion-only", "results": reports,
                  "passed": bool(reports) and all(r["passed"] for r in reports),
                  "client_delay_ms": client_delay,
                  "not_validated": ([] if bot else ["bot"]) + ["combat", "network", "observations", "inference"],
                  "paper_sha256": sha256(serverdir / "paper-26.1.2-74.jar"),
                  "jars": {p.name: sha256(p) for p in (serverdir / "plugins").glob("*.jar")}}
        (run / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
        return result
    finally:
        try:
            if client: client.stop()
            server.stop(server=True)
        finally:
            resources.close()


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=Path)
    p.add_argument("--case", action="append")
    p.add_argument("--full", action="store_true")
    p.add_argument("--random-only", action="store_true", help="Run only the 100 seeded and 6000-tick movement cases")
    p.add_argument("--bot", action="store_true")
    p.add_argument("--telemetry", action="store_true")
    p.add_argument("--client-delay", type=int, nargs=2, choices=range(301), metavar=("UP_MS", "DOWN_MS"))
    p.add_argument("--seats", nargs="+", type=int, choices=(0, 1), default=[0, 1])
    p.add_argument("--repetitions", type=int, default=5)
    a = p.parse_args()
    if a.repetitions < 1: p.error("repetitions must be positive")
    result = run_suite(a.output.resolve(), a.case, a.full or a.random_only, a.repetitions, a.bot, a.seats, a.random_only,
                       a.telemetry, a.client_delay)
    raise SystemExit(0 if result["passed"] else 1)
