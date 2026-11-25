# Paper / Rust differential validation

This implementation currently validates **isolated local movement**, **inference on shared observations**, and **idle physics after a measured received impulse**. Live single-hit trials expose a remaining knockback mismatch. Combat, network, and live-observation equivalence are not established. A passing component summary must not be interpreted as passing the complete parity plan.

## Reproduce

Run from the repository root with JDK 25, Python 3.12+ and Rust. `python` below can be replaced with `build/rl-venv/Scripts/python.exe` on this workspace.

```powershell
.\tools\paper\setup.bat -BuildOnly
.\fabric\gradlew.bat -p fabric --no-daemon --max-workers=1 build parityLaunch
.\tools\rust\cargo.bat build --locked -p pvp-sim --example parity_motion
```

Build the fixture from the pinned local JARs, an existing EULA acceptance, and the cached 26.1.2 Mojang server JAR. A first run of the [local server](../../paper/RLBOT_SETUP.md#start-and-stop-the-local-server) populates the cache.

```powershell
python tools/parity/build_fixture.py build/paper-parity-fixture --eula paper/run-rl-standalone/eula.txt --vanilla-cache paper/run-rl-standalone/cache/mojang_26.1.2.jar
python tools/parity/run_motion.py build/parity/new-run --bot --repetitions 5
```

The runners copy the fixture at `build/paper-parity-fixture/server` into a fresh output directory and install the pinned Citizens JAR and current PvPBotRL build. They use a loopback port, a flat overworld arena and synthetic player accounts. They stop their server and clients on completion. Set `PVP_PARITY_FIXTURE` to the `server/` path from another standalone fixture. Keep Windows output paths short.

The reference is a real Minecraft 26.1.2 client. Fabric supplies keyboard states at the start of `Minecraft.tick`, and records after `LocalPlayer.tick`. No reference movement implementation is substituted. Bot samples are taken at Paper tick end. The Rust example replays the captured initial state through the movement routine used by `Duel`; it does not exercise the entire `Duel` network/combat pipeline.

The 12 cases run with initial yaw 0 and 180 degrees. The `seat` label in these **single-body** movement cases denotes orientation, not a two-player match or swapped packet processing order. `--full` additionally generates 100 seeded 600-tick input scripts and a 6,000-tick idle case. `--case NAME`, `--seats 0`, and `--repetitions 1` restrict a diagnostic run. All training scripts hold the sprint key.

Horizontal collision invalidates a reference capture. Captures preserve warmup and initial state. Generated data remains under the ignored `build/` directory.

## Saved movement captures

Run `python tools/parity/verify_saved.py` to build the movement example, copy six committed real-client captures into a fresh directory under `build/parity/`, and compare Rust playback. Five captures must match directly. The `sprint_feedback` capture must reproduce the known X-position divergence without feedback and match when replayed with `measured-feedback.json`. The two runs write separate comparison reports. Git checks out the fixtures with LF because the manifests hash the committed input bytes.

## Inference corpus

```powershell
.\tools\rust\cargo.bat run --release --offline --locked -p pvp-policy --example parity_policy -- build/parity/policy.ndjson
.\paper\gradlew.bat -p paper/rlbot --offline --no-daemon --max-workers=1 -PparityFixture=C:/absolute/path/to/policy.ndjson test
```

The corpus covers the bundled recurrent-PPO, recurrent-G2, recurrent-G2+, and G3 weights across 100 seeds and both policy seats, cycling through 12 RTT values. It compares raw features (1e-6), recurrent hidden states and logits (1e-4), and exact selected actions. It resets memory at each episode. Supply a corpus to run the optional Java/Rust inference test.

To verify an external training model, pass its JSON path after the corpus path when generating the corpus. Pass the same JSON to Gradle with `-PparityTrainingModel=C:/absolute/path/to/model.json`. The resulting corpus uses that model for all 200 episodes and tests the same controller factory used by random Paper matches.

## Capture contract

Schema 1 movement captures contain a manifest, one `state` record per movement step, and an `end` record. The manifest fingerprints the exact scenario and measured initial state. State keys are `(step, phase, actor, channel)`. Wall-clock times are diagnostic; logical tick times must match. Position and velocity use blocks and blocks/tick, respectively.

`compare.py reference.ndjson candidate.ndjson --output difference.json` returns nonzero on failure. It rejects missing fields, nulls, NaN, missing or duplicate steps, changed input hashes, incomplete files, lost records, and unsupported event types. Discrete values are exact. Numeric tolerances are fixed in code; angle comparison wraps at 360 degrees. Reports contain the first difference, surrounding states, and maximum errors. It deliberately refuses to ignore unknown events; attack/packet event comparison still needs an event-specific implementation.

Rust's `Config.trace_states` adds opt-in privileged snapshots before/after client/server phases and after packet application. It defaults to false and is omitted from serialized defaults. It does not expose server state to actors. Existing packet trace capacity/drop accounting still applies. Legacy Paper motion TSV logs now report dropped records in a `trace-summary` line.

## Single-hit combat diagnostic

After building both Java projects as above:

```powershell
.\tools\rust\cargo.bat build --offline --locked -p pvp-sim --example parity_attack
python tools/parity/run_combat.py build/parity/new-combat
python tools/parity/analyze_combat.py build/parity/new-combat
```

The runner starts two real clients and asks the standalone Paper probe to stage a player-versus-player trial followed by a player-versus-Citizens-bot trial. The human clicks once with full charge, and the diagnostic bot remains idle. `server.json` records damage events and 81 tick-end snapshots. `analyze_combat.py` accepts both standalone captures and historical captures. It reports exact hit ticks, state flags, first motion divergence, surrounding records and maximum errors. Its `passed` remains false while full initialization, durability and network gates are unmeasured. `measured_checks_passed` describes the available checks. Nonzero exit is expected for the saved mismatch.

The client property `pvp.parity.telemetry` adds a separate, opt-in `telemetry.ndjson` stream with client tick, process monotonic time, epoch milliseconds and record sequence. It logs local physics and self motion packet application **after** vanilla main-thread handling. It does not timestamp Netty receipt. A `telemetry-stop.json` request closes the stream with an explicit end marker. Writes currently flush on every record and can affect timing; this is not a load/performance benchmark. Epoch milliseconds are diagnostic, not an assumed exact shared monotonic clock.

The server `velocity_event` records a Bukkit event and its round-trip through the vanilla LpVec3 codec. These bytes encode that event value, **not captured outbound wire bytes**. In the measured attack that event contains old server velocity, whereas the client receives the knockback velocity. Do not substitute it for a packet trace.

To isolate physics after the actual victim receives its impulse:

```powershell
python tools/parity/extract_impulse.py build/parity/new-combat/ParityB/telemetry.ndjson build/parity/new-combat/received-impulse
.\target\debug\examples\parity_motion.exe build/parity/new-combat/received-impulse
python tools/parity/compare.py build/parity/new-combat/received-impulse/client.ndjson build/parity/new-combat/received-impulse/rust.ndjson --output build/parity/new-combat/received-impulse/difference.json
python tools/parity/run_saved_bot.py build/parity/new-combat/received-impulse build/parity/new-impulse-bot
python -m unittest discover -s tools/parity -p 'test_*.py'
```

Extraction requires one horizontal impulse, an idle grounded victim, 35 consecutive subsequent physics samples, no additional motion packets during that window, and a complete telemetry stream. It preserves the original packet/tick/sequence provenance. The floor friction is explicitly assumed to be the fixture's smooth-stone value 0.6. This begins at measured packet application; it neither replays the original attack nor shifts a full-fight trace. `run_saved_bot.py` supports the central area of the same isolated arena at floor y=65. The impulse is included in the [saved movement captures](#saved-movement-captures).

## Packet processing and replay

```powershell
.\paper\gradlew.bat -p paper/rlbot --offline --no-daemon --max-workers=1 build parityAgentJar
.\tools\rust\cargo.bat build --offline --locked -p pvp-sim --example parity_server --example parity_replica --example parity_pick
python tools/parity/run_combat.py build/parity/paper-processing --packet-handlers --motion-transport
python tools/parity/analyze_uplink.py build/parity/paper-processing
python tools/parity/replay_server.py build/parity/paper-processing/charged_player/server.json build/parity/paper-processing/server-replay
python tools/parity/replay_replica.py build/parity/paper-processing build/parity/paper-processing/replica-replay
```

The separate Java 25 agent records the return from pinned Paper's normal move, attack, input, sprint-command and client-tick-end handlers. The isolated plugin correlates the same packet object with its earlier decoded Netty receipt and records monotonic times for the server tick boundaries. The comparison uses those timestamps and packet order. The `legacy` option replays the same inputs with the prior simulator rules. The default replay profile `paper_flat_v1` also records the settings version in the replay input and saves the input and reference SHA-256 hashes.

Replica replay supplies recorded position-sync packets, relative position and angle values, velocity and metadata, then advances on the observed remote player's tick sequence. Its scope covers the provided packets and the Rust interpolation state. The ordinary client's selected target and the bot's sampled and applied observations have their own channels.

```powershell
python tools/parity/run_combat.py build/parity/network --network-matrix --motion-transport
python tools/parity/summarize_combat.py build/parity/network
python tools/parity/run_combat.py build/parity/combat-cases --combat-suite --swap --repetitions 5 --packet-handlers
python tools/parity/summarize_combat.py build/parity/combat-cases
python tools/parity/run_combat.py build/parity/reach --case reach_inside --case reach_outside
python tools/parity/replay_pick.py build/parity/reach reach_outside_player
```

The loopback relay queues TCP bytes independently in both directions and appends receive, deadline and forwarding timestamps for each chunk to `relay.ndjson`. `--client-delay UP_MS DOWN_MS` selects one condition, and `--fps` selects a 30, 60 or 120 FPS limit. The bot's simulated motion delays are independent flags. `--swap` reverses the ordinary clients' identities and the arena orientation.

`--match-motion-delays --motion-transport` applies the relay profile's nominal delays to the bot's experimental idle movement queues. Its phases remain server-tick based. `--connect-b-first` varies connection order. `--start-offset-ms` requests a phase offset and stores the actual launcher clock in each trial. Packet application keeps its measured tick and time.

`--case overlap --packet-handlers` records the actual random draws used to generate the same-position knockback direction. Rust consumes those draws in order and flags any extra or missing draw. `--replica-downlink-ms 51 --packet-handlers` records a separate diagnostic observer driven by the native Paper entity tracker. Its packet publication, application and interpolation appear in `remote_replica` in the server capture.

In an ordinary bot capture, `production_replica` records the tracker packets delivered to the bot and the resulting frames. `--replica-observation` selects the separate diagnostic observer for experiments with independent timing. `check_replica_inputs.py SERVER_JSON` verifies the fields supplied to either bot path against its replica, including ages and recurrent opponent HP. Across different matches use `analyze_observation_ages.py` to compare the measured distributions.

With `--replica-observation --motion-transport`, `--attack-uplink-ms` enables a separate attack/swing queue into the normal Paper handlers. The bot picks from the received replica and logs the pick, send/deadline, handler return and server damage event. `check_attack_uplink.py SERVER_JSON 51` checks a single scripted hit queued with 51 ms uplink.

`--mixed-seed 13 --mixed-ticks 600 --packet-handlers` supplies two independent deterministic action streams to the ordinary clients and bot across paired trials. Use repeated `--mixed-seed` arguments to choose seeds. The scripts and capture length are retained with the trial. `--mixed-ticks 6000` expands the isolated floor and raises the time limit to 360 seconds. `--synchronized-start` schedules both scripts in advance and logs the actual first input time with its planned start.

For the local sprint feedback counterexample, replay `tests/fixtures/parity/motion/sprint_feedback` from a copy under `build/parity/`. Pass `measured-feedback.json` as the second argument to `parity_motion` to apply the captured fields. `run_saved_bot.py SOURCE OUTPUT --measured-feedback` replays the same input on an isolated, wall-cleared Citizens body. `analyze_motion_feedback.py RUN` extracts feedback for divergent motion cases captured with `run_motion.py --telemetry`.

### Live recurrent-policy diagnostics

```powershell
python tools/parity/run_combat.py build/parity/ppo-live --policy-model recurrent-ppo
.\tools\rust\cargo.bat build --offline --locked -p pvp-policy --example parity_live_policy
python tools/parity/replay_live_policy.py build/parity/ppo-live/policy_bot/server.json
```

For another bundled model, pass `--policy-model recurrent-g2` or `--policy-model recurrent-g2-plus` to the capture. Each frame records the model hash, raw observations, features, GRU state, logits and selected action. Rust starts with zero GRU state and replays the recorded observations in order.

## Experimental bot motion transport


```powershell
.\tools\rust\cargo.bat build --offline --locked -p pvp-sim --example parity_motion_codec
python tools/parity/run_combat.py build/parity/new-transport --motion-transport --downlink-ms 51
python tools/parity/analyze_transport.py build/parity/new-transport
python tools/parity/analyze_combat.py build/parity/new-transport
```

This is **isolated diagnostics only**, requiring both `pvp.parity.root` and `pvp.parity.motionTransport=true`. The normal adapter identifies itself as `citizens-player-input-v3`; the opt-in transport identifies itself as `experimental-motion-transport-v1`. It explicitly invokes pinned `ServerPlayer.hurtServer` via a special method handle to bypass Citizens' clearing/re-scheduling of `hurtMarked`. Paper then sends a real motion packet and restores the pre-hit server velocity. An `EmptyPacketListener` replacement consumes self motion packets through vanilla LpVec3 encoding/decoding, queues each deadline, and applies eligible packets before physics. Unexpected linkage, invalid clocks, off-thread access, or diagnostic overflow fail explicitly.

`--downlink-ms` controls only this experimental bot motion inbox; it does not configure latency on the ordinary clients or replace `BotPing`. It uses logical server time with a 50 ms tick and records the actual application tick separately. The stored epoch/monotonic clocks are separate diagnostic values. There is no arbitrary extra delay fitted to the measured human trace. Other packet types still follow Citizens' empty-listener behavior.

The 0 ms and 51 ms single-hit runs matched the human victim's decoded impulse and final position exactly. The Rust codec also matched captured Paper bytes exactly. At 51 ms the inbox applied the impulse two ticks after sending, 49 ms after its logical deadline. This does **not** establish full-fight timing equivalence: the motion uplink, independent client phase, and full server/predicted velocity separation remain absent. `analyze_transport.py` reports these component results but deliberately exits nonzero and keeps overall `passed=false`; the full combat analyzer also continues to report mismatches.

`--motion-uplink-ms` additionally arms independent predicted position and velocity, creates vanilla movement packets according to the reported-position threshold, and delivers eligible packets to Paper's normal movement handler before the server's travel step. `analyze_bot_uplink.py` checks the idle single-hit prediction prefix, application phase, deadlines and final position. Mixed trials also send physical input, sprint commands and client tick-end packets. `replay_bot_motion.py SERVER_JSON OUTPUT` checks the isolated moving predictor against Rust from the same inputs and received self-feedback. `replay_received_motion.py CLIENT_CASE OUTPUT` performs the same component check for an ordinary client in a recorded combat trial.

Saved `motion_transport_zero` and `motion_transport_51ms` combat fixtures include the paired server captures, jar hashes and component reports. The codec example's regression tests validate the actual captured bytes, including a deliberately corrupted byte. The Python checker rejects missing/reordered delivery records, early applications, changed phases and malformed velocities.

### Outstanding gates

The [verification report](../../docs/simulator/paper-parity.md#known-differences-and-open-verification-items) records the known differences, the pending moving-target and network-condition cases, and the required evidence for each.

- Verify all initial state and elapsed-time fields for each combat trial and resolve the remaining Rust replay differences.
- Verify all three observation channels from bootstrapping through live policy, including the bot's own prediction, independent client frame phases and serverbound acceptance.
- Close the four failed random-motion cases independently of measured feedback. Extend the repeated tests and seed corpus to mixed-input combat and long fights.
- Finish the three-way acceptance check after the outcome-changing differences pass. `coverage.py` currently reports available evidence and keeps the overall gate closed.

Model weights, live server configuration and deployed plugin JAR are unchanged. The shipped ladder template disables regeneration; applying it to an existing server is a separate deployment step after the full suite passes.
