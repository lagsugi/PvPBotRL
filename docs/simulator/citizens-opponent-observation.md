# Citizens opponent position and velocity

Paths under `build/` refer to generated local artifacts. They are relative to the repository root.

The current `citizens-tracker-opponent-v5` match adapter uses the same native Paper packet format as the diagnostic receiver below. It wraps the pinned Paper 26.1.2 build 74 `ServerEntity` synchronizer during the match, delegates sends to the original synchronizer, and queues copies for the bot. Pairing establishes the position base. Subsequent updates apply at tick deadlines using half the configured RTT. Closing the observation feed restores the original synchronizer when its last subscriber leaves. [Subscription lifecycle](../../paper/rlbot/src/main/java/dev/sugi/pvpbot/rl/OpponentTracking.java)

The isolated v5 run completed eight ZonePractice matches on JDK 25 without the diagnostic Java agent. It exercised active PPO in both orientations and a separate 120-step mixed input script with seed 13 in both orientations. In four bot matches, 369 tracker packets applied in order and at or after their deadlines. All 480 sampled opponent observations matched the received replica, including recurrent opponent HP when present. Rust reproduced the model action for all 332 live PPO frames. The two ordinary-client mixed input replays matched Rust position and velocity over 619 events given the measured deliveries. At the end of the run both test players returned to their recorded lobby positions. Production validation (`build/parity/tracker-opponent-live-20261121/production-checks.json`) · Runtime hashes (`build/parity/tracker-opponent-live-20261121/runtime-sha256.json`)

The Gradle build passed 15 tests with the optional inference-corpus test skipped. The Python parity suite passed 26 tests using the committed LF fixture bytes. The tested scope covers opponent observation delivery and inference with those inputs. Paper services the bot's queues on server ticks, so live match timing and win-rate parity remain separate checks.

## Historical snapshot comparison

Measured on November 21, 2026 with Minecraft 26.1.2, Paper build 74, Citizens b4270, and the production `citizens-delayed-opponent-v4` observation path. The isolated fixture used a 20 ms configured bot RTT and a 10 ms diagnostic tracker downlink. Two 120-step input scripts (seeds 0 and 13) ran in both seat orientations. Each script produced a Citizens match and a two-client reference match. Runtime hashes (`build/parity/obs-shadow-20261121/runtime-sha256.json`)

## Compared stages

In each bot match the diagnostic tracker ran alongside the production bot. The comparison pairs the tracker frame at tick start with the production `bot_sample` from the same server tick. It reconstructs the absolute opponent position by adding the observation's relative position to the bot's sampled position. Velocity is compared directly in blocks per tick. The tolerance is `1e-6` for the Euclidean difference in each vector. The tracker frames come from Paper's actual outgoing tracker packets with the configured delay and interpolation.

The v4 production path used a tick-sampled Paper position and `getDeltaMovement()` velocity. Its ages measured elapsed time since the most recent delivered change in each field. The tracker path decodes position and motion packets and interpolates them, and its ages measure elapsed time since packet application. Both clocks can report an age of zero while the spatial values differ. An absent tracker velocity age in these captures indicates that the diagnostic observer had yet to apply a separate velocity update. [Current tracker receiver](../../paper/rlbot/src/main/java/dev/sugi/pvpbot/rl/OpponentReplica.java)

In the seed 13 ParityA trial, the first decision already differed on velocity. Both positions were `(0.5, 65, -9.5)`, with measured Y velocities of `-0.078400` in the snapshot and `0` in the tracker replica. The first position divergence was at server tick 1522, when the opponent jumped. The bot saw Y `65.420000` and the replica was at `65.140000`. Both position ages were zero. This identifies the delivered snapshot versus interpolation stage as a source of the difference. First difference and decision samples (`build/parity/obs-shadow-20261121/mixed_013_bot_0up_0down_ParityA_0/snapshot-difference.json`)

The second comparison feeds the ordinary opponent client's recorded packets and logical tick order to Rust. This checks the Rust replica against the position and velocity produced by a real Minecraft client under the same deliveries. The ordinary and bot matches have independent game clocks and packet delivery schedules.

## Results

| Striker and seed | Bot decisions with different position | Maximum position difference (blocks) | Bot decisions with different velocity | Maximum velocity difference (blocks/tick) |
| --- | ---: | ---: | ---: | ---: |
| ParityA, 0 | 131/160 | 0.574 | 160/160 | 0.807 |
| ParityB, 0 | 131/160 | 0.579 | 160/160 | 0.807 |
| ParityA, 13 | 130/160 | 0.685 | 160/160 | 0.865 |
| ParityB, 13 | 131/160 | 0.707 | 160/160 | 0.807 |

The four ordinary-client replays matched Rust across 1,348 packet and tick events. Maximum observed position and velocity errors were zero when the two replicas consumed the same packets at the same steps. Position, velocity, and metadata ages agreed after updates from their respective channels. The isolated comparison establishes a difference in the opponent inputs supplied to Citizens. The effect on policy decisions and match outcomes remains unmeasured. The Python parity unit suite passed all 26 tests using the committed LF fixture bytes. Aggregate comparison (`build/parity/obs-shadow-20261121/observation-summary.json`)

## Reproduce the historical comparison

The snapshot comparison uses a v4 capture. The saved evidence below can be compared directly. To generate a fresh snapshot capture, use the v4 plugin and follow the [packet processing prerequisites](../../tools/parity/README.md#packet-processing-and-replay).

```powershell
python tools/parity/run_combat.py build/parity/observation-new --mixed-seed 13 --mixed-seed 0 `
  --mixed-ticks 120 --synchronized-start --packet-handlers --replica-downlink-ms 10 --swap
python tools/parity/compare_opponent_snapshot.py `
  build/parity/observation-new/mixed_013_bot_0up_0down_ParityA_0/server.json `
  build/parity/observation-new/snapshot-difference.json
python tools/parity/replay_replica.py build/parity/observation-new build/parity/observation-rust `
  --trial mixed_013_player_0up_0down_ParityA_0 --observer-seat opponent
```
