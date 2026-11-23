"""Capture Paper combat against an ordinary player and a Citizens bot.

This first combat probe is evidence collection, not the full combat acceptance suite.
"""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import time
from provenance import ROOT, runtime_identity, sha256
from run_motion import PAPER_JAR, Process, clear_motion_arena, prepare, publish
from latency import LatencyRelay
from replay_live_policy import MODELS
from scenarios import combat_suite, knockback_suite, mixed_fight


RTTS = (0, 1, 20, 49, 50, 51, 99, 100, 101, 120, 150, 300)


def network_profiles():
    return [(rtt // 2, rtt - rtt // 2) for rtt in RTTS] + [(10, 90), (90, 10)]


def wait_file(path, processes, timeout=90, peer_failures=()):
    deadline = time.monotonic() + timeout
    while not path.exists():
        failed = path.parent / "failed.json"
        if failed.exists(): raise RuntimeError(failed.read_text())
        failed = path.parent / "telemetry-failed.json"
        if failed.exists(): raise RuntimeError(failed.read_text())
        failed = path.parent / "bot-failed.json"
        if failed.exists(): raise RuntimeError(failed.read_text())
        for failed in peer_failures:
            if failed.exists(): raise RuntimeError(f"{failed}: {failed.read_text()}")
        if any(p.process.poll() is not None for p in processes): raise RuntimeError("Capture process exited")
        if time.monotonic() > deadline: raise TimeoutError(str(path))
        time.sleep(0.1)


def main(output, motion_transport=False, downlink_ms=0, profiles=None, fps=60, repetitions=1, swap=False,
         cases=("charged",), packet_handlers=False, motion_uplink_ms=None, policy_model=None, match_motion_delays=False,
         start_offset_ms=None, connect_b_first=False, replica_downlink_ms=None, replica_observation=False,
         attack_uplink_ms=None, mixed_seeds=(), mixed_ticks=600, synced_start=False):
    output.mkdir(parents=True, exist_ok=False)
    serverdir, port = prepare(output)
    long_match = bool(mixed_seeds) and mixed_ticks + 40 >= 5600
    (output / "runtime-sha256.json").write_text(json.dumps(runtime_identity(serverdir), indent=2) + "\n")
    agent = ROOT / "paper/rlbot/build/libs/pvpbot-rl-0.1.0-parity-agent.jar"
    agent_args = [f"-javaagent:{agent}", "-Dpvp.parity.packetHandlers=true"] if packet_handlers else []
    if replica_downlink_ms is not None:
        agent_args.extend(["-Dpvp.parity.remoteReplica=true", f"-Dpvp.parity.replicaDownlinkMs={replica_downlink_ms}"])
    if replica_observation:
        agent_args.append("-Dpvp.parity.replicaObservation=true")
    if attack_uplink_ms is not None:
        agent_args.append(f"-Dpvp.parity.attackUplinkMs={attack_uplink_ms}")
    uplink_args = [f"-Dpvp.parity.motionUplinkMs={motion_uplink_ms}"] if motion_uplink_ms is not None else []
    server = Process(["java", "-Xms512M", "-Xmx2G", *agent_args, *uplink_args, f"-Dpvp.parity.root={output}",
                      f"-Dpvp.parity.motionTransport={str(motion_transport).lower()}",
                      f"-Dpvp.parity.motionDownlinkMs={downlink_ms}",
                      "-jar", PAPER_JAR, "--nogui"], serverdir, output / "server.log")
    clients = []
    resources = ExitStack()
    try:
        server.wait_line("Done (")
        clear_motion_arena(server, z_radius=192 if long_match else 64)
        relay = resources.enter_context(LatencyRelay(port, *profiles[0], output / "relay.ndjson")) if profiles else None
        launch = json.loads((ROOT / "fabric/build/parity-launch.json").read_text())
        client_order = ("ParityB", "ParityA") if connect_b_first else ("ParityA", "ParityB")
        for name in client_order:
            directory = output / name; directory.mkdir()
            (directory / "options.txt").write_text(f"pauseOnLostFocus:false\nenableVsync:false\nmaxFps:{fps}\n"
                "renderDistance:2\nsimulationDistance:5\nautoJump:false\nonboardAccessibility:false\n")
            args = ["java", "-Xmx1G", f"-Dpvp.parity.dir={directory}", f"-Dpvp.parity.telemetry={directory}",
                    *[a for a in launch["jvmArgs"] if not a.startswith("@")], "-cp", launch["classpath"],
                    launch["main"], *launch["args"], "--username", name, "--gameDir", str(directory),
                    "--width", "320", "--height", "240", "--quickPlayMultiplayer",
                    f"127.0.0.1:{relay.port if relay else port}"]
            client = Process(args, directory, output / (name + ".log"), dict(os.environ, **launch["environment"]))
            clients.append(client)
            server.wait_line("logged in with entity id", 180, client)
            server.command("op " + name)
        summaries = []
        suite = combat_suite() | knockback_suite()
        if mixed_seeds:
            suite.update({f"mixed_{seed:03}": mixed_fight(seed, mixed_ticks) for seed in mixed_seeds})
        if policy_model:
            suite["policy"] = ({"distance": 2.5, "policy_model": policy_model},
                               [{"keys": {}, "click": i in (10, 25)} for i in range(40)])
        trials = [(profile, striker, replicate, case, bot) for profile in profiles or [(0, 0)]
                  for striker in ("ParityA", "ParityB")[:2 if swap else 1]
                  for replicate in range(repetitions) for case in cases for bot in (False, True)]
        for (up, down), striker, replicate, case, bot in trials:
            setup, actions = suite[case]
            name = case + ("_bot" if bot else "_player")
            if profiles or swap or repetitions != 1:
                name += f"_{up}up_{down}down_{striker}_{replicate}"
            if relay:
                relay.set_delays(up, down)
            temporary = output / "fight-request.tmp"
            temporary.write_text(json.dumps({"id": name, "bot": bot, "striker": striker, "schema": 1,
                                             "client_uplink_ms": up, "client_downlink_ms": down, "fps_limit": fps,
                                             "scenario": case, "match_motion_delays": match_motion_delays,
                                             "script_start_delay_ticks": 8 if synced_start else 0,
                                             "start_offset_ms": start_offset_ms, **setup}))
            publish(temporary, output / "fight-request.json")
            ready_path = output / name / "ready.json"
            wait_file(ready_path, [server, *clients])
            if start_offset_ms is not None:
                ready = json.loads(ready_path.read_text())
                deadline = ready["epoch_ms"] + 250 + start_offset_ms
                time.sleep(max(0, (deadline * 1_000_000 - time.time_ns()) / 1_000_000_000))
                (output / name / "start-clock.json").write_text(json.dumps({"schema": 1, "known_ready": ready,
                    "planned_epoch_ms": deadline, "actual_epoch_ns": time.time_ns(), "process_monotonic_ns": time.monotonic_ns()}))
            request = {"schema": 1, "id": name, "warmup": 0, "fps": fps,
                       "floor_y": 65, "floor_friction": 0.6, "actions": actions}
            if synced_start:
                ready = json.loads(ready_path.read_text())
                request["start_epoch_ms"] = ready["script_start_epoch_ms"]
            temporary = output / striker / "request.tmp"
            temporary.write_text(json.dumps(request))
            publish(temporary, output / striker / "request.json")
            opponent = "ParityB" if striker == "ParityA" else "ParityA"
            opponent_actions = setup.get("opponent_actions", suite["charged"][1] if setup.get("opponent_click") else None)
            if opponent_actions is not None and not bot:
                temporary = output / opponent / "request.tmp"
                temporary.write_text(json.dumps({**request, "actions": opponent_actions}))
                publish(temporary, output / opponent / "request.json")
            scripted_clients = (striker, opponent) if opponent_actions is not None and not bot else (striker,)
            wait_file(output / name / "server.json", [server, *clients], timeout=max(90, len(actions) / 10 + 20),
                      peer_failures=[output / client / name / "failed.json" for client in scripted_clients])
            wait_file(output / striker / name / "complete.json", [server, *clients])
            if opponent_actions is not None and not bot:
                wait_file(output / opponent / name / "complete.json", [server, *clients])
            data = json.loads((output / name / "server.json").read_text())
            hits = [e for e in data["events"] if e["event"] == "damage"]
            summaries.append({"case": name, "scenario": case, "bot": bot, "striker": striker, "repeat": replicate,
                              "client_uplink_ms": up, "client_downlink_ms": down,
                              "hits": hits, "captured": data["status"] == "complete" and
                              (case != "charged" or len(hits) == 1 and not hits[0]["cancelled"])})
            print(json.dumps({"case": name, "accepted_hits": sum(not hit["cancelled"] for hit in hits),
                              "captured": summaries[-1]["captured"]}), flush=True)
            time.sleep(3)
        for name in ("ParityA", "ParityB"):
            directory = output / name
            (directory / "telemetry-stop.json").write_text("{}")
            wait_file(directory / "telemetry-complete.json", [server, *clients])
        result = {"schema": 1, "scope": "scripted-combat-capture-only", "results": summaries,
                  "experimental_motion_transport": motion_transport, "motion_downlink_ms": downlink_ms,
                  "motion_uplink_ms": motion_uplink_ms,
                  "match_motion_delays": match_motion_delays,
                  "connect_order": client_order, "start_offset_ms": start_offset_ms,
                  "replica_downlink_ms": replica_downlink_ms,
                  "replica_observation": replica_observation, "attack_uplink_ms": attack_uplink_ms,
                  "synchronized_script_start": synced_start,
                  "fps_limit": fps, "client_transport": "loopback-tcp-relay" if relay else "direct-loopback",
                  "packet_agent_sha256": sha256(agent) if packet_handlers else None,
                  "paper_sha256": sha256(serverdir / PAPER_JAR),
                  "client_launch_sha256": sha256(ROOT / "fabric/build/parity-launch.json"),
                  "jars": {p.name: sha256(p) for p in (serverdir / "plugins").glob("*.jar")}}
        (output / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
        return all(s["captured"] for s in summaries)
    finally:
        for client in clients: client.stop()
        try: server.stop(server=True)
        finally: resources.close()


if __name__ == "__main__":
    combat_cases = combat_suite()
    suite = combat_cases | knockback_suite()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--motion-transport", action="store_true", help="Experimental isolated bot motion packet path")
    parser.add_argument("--downlink-ms", type=int, default=0, choices=range(0, 10001), metavar="0..10000")
    network = parser.add_mutually_exclusive_group()
    network.add_argument("--network-matrix", action="store_true")
    network.add_argument("--client-delay", type=int, nargs=2, choices=range(301), metavar=("UP_MS", "DOWN_MS"))
    parser.add_argument("--fps", type=int, choices=(30, 60, 120), default=60)
    parser.add_argument("--repetitions", type=int, default=1)
    parser.add_argument("--swap", action="store_true", help="Repeat with the opposite ordinary client as striker")
    parser.add_argument("--case", action="append", choices=suite)
    parser.add_argument("--combat-suite", action="store_true")
    parser.add_argument("--packet-handlers", action="store_true", help="Record isolated Paper main-thread packet handler returns")
    parser.add_argument("--motion-uplink-ms", type=int, choices=range(0, 10001), metavar="0..10000")
    parser.add_argument("--match-motion-delays", action="store_true", help="Use each client's requested delays in the bot's motion queues")
    parser.add_argument("--policy-model", choices=tuple(MODELS))
    parser.add_argument("--start-offset-ms", type=int, choices=range(50), metavar="0..49")
    parser.add_argument("--connect-b-first", action="store_true")
    parser.add_argument("--replica-downlink-ms", type=int, choices=range(301), metavar="0..300")
    parser.add_argument("--replica-observation", action="store_true")
    parser.add_argument("--attack-uplink-ms", type=int, choices=range(301), metavar="0..300")
    parser.add_argument("--mixed-seed", type=int, action="append", choices=range(100), metavar="0..99")
    parser.add_argument("--mixed-ticks", type=int, choices=range(20, 6001), metavar="20..6000", default=600)
    parser.add_argument("--synchronized-start", action="store_true")
    args = parser.parse_args()
    if args.downlink_ms and not args.motion_transport: parser.error("--downlink-ms requires --motion-transport")
    if args.repetitions < 1: parser.error("--repetitions must be positive")
    if args.replica_downlink_ms is not None and not args.packet_handlers:
        parser.error("--replica-downlink-ms requires --packet-handlers")
    if args.replica_observation and args.replica_downlink_ms is None:
        parser.error("--replica-observation requires --replica-downlink-ms")
    if args.attack_uplink_ms is not None and not (args.replica_observation and args.motion_transport):
        parser.error("--attack-uplink-ms requires --replica-observation and --motion-transport")
    profiles = network_profiles() if args.network_matrix else [tuple(args.client_delay)] if args.client_delay else None
    cases = args.case or (tuple(combat_cases) if args.combat_suite else ("charged",))
    if args.policy_model:
        if args.case or args.combat_suite: parser.error("--policy-model selects its own scripted trial")
        cases = ("policy",)
    if args.mixed_seed:
        if args.policy_model or args.case or args.combat_suite:
            parser.error("--mixed-seed selects its own two-sided inputs")
        cases = tuple(f"mixed_{seed:03}" for seed in dict.fromkeys(args.mixed_seed))
    paired_scripts = args.mixed_seed or all(case in suite and "opponent_actions" in suite[case][0] for case in cases)
    if args.synchronized_start and (not paired_scripts or args.start_offset_ms is not None):
        parser.error("--synchronized-start requires action scripts for both sides and no start offset")
    if args.match_motion_delays:
        if not profiles or args.motion_uplink_ms is not None or args.downlink_ms:
            parser.error("--match-motion-delays requires a client latency profile and selects the bot motion delays")
        args.motion_uplink_ms = 0
    if args.motion_uplink_ms is not None and not (args.motion_transport and (args.mixed_seed or tuple(cases) == ("charged",))):
        parser.error("The motion uplink requires --motion-transport and a charged or mixed trial")
    raise SystemExit(0 if main(args.output.resolve(), args.motion_transport, args.downlink_ms, profiles, args.fps,
                               args.repetitions, args.swap, cases, args.packet_handlers, args.motion_uplink_ms,
                               args.policy_model, args.match_motion_delays, args.start_offset_ms, args.connect_b_first,
                                args.replica_downlink_ms, args.replica_observation, args.attack_uplink_ms,
                                tuple(dict.fromkeys(args.mixed_seed or ())), args.mixed_ticks, args.synchronized_start) else 1)
