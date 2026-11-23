# Human vs Bot: native Rust duel window

**Deprecated.** Use the [Paper server](../../paper/RLBOT_SETUP.md) for human-vs-bot play in Minecraft. The native window and the commands below remain available for existing local workflows.

`pvp-play` is a first-person interface to `pvp-sim`. The human controls seat 0 and the selected opponent controls seat 1. The duel runs locally in Rust.

## Launch

From the workspace root in PowerShell:

```powershell
.\tools\play\start.bat
```

To play against the bundled Recurrent-PPO bot:

```powershell
.\tools\play\start.bat --opponent recurrent-ppo
```

Alternatively, build once and run the executable:

```powershell
.\tools\rust\cargo.bat build --release --locked -p pvp-play
.\target\release\pvp-play.exe
```

The app starts paused. Press **Enter** to start and capture the mouse. Press **Esc** to pause and release it. Both players face each other 20 blocks apart, measured between their centers at Z = −10 and +10. Defaults are 20 HP, full diamond Protection IV and a diamond sword. Human RTT defaults to 20 ms; bot RTT is drawn per match from Gamma(k=2, θ=35 ms) conditional on 5–150 ms. Spectating draws both bots independently. Restarts keep positions/equipment and the human RTT setting, but advance the seed and draw fresh bot timing. Use `--ping-profile fixed` for the old fixed-timing/restart behavior. See [ping controls and examples](ping.md).

Options accept values:

```powershell
.\target\release\pvp-play.exe --config configs/duel.json --seed 42 --record-dir recordings
```

Relative command-line paths use the current working directory. The default recording directory is resolved from the executable. Workspace builds use the project's `recordings/` folder. A standalone executable uses a sibling `recordings/` folder.

## Controls

| Input | Behavior |
| --- | --- |
| Mouse | Manual yaw/pitch; no automatic aim for the human |
| Left mouse press | One attack click, applied at the next decision tick |
| W / A / S / D | Move; opposite keys cancel |
| Space | Jump |
| Shift | Sneak |
| Left / right arrow on the ready screen | Select a practice opponent |
| Up / down arrow on the human ready screen | Change your RTT by 1 ms (Shift: 10 ms) |
| N on the ready screen | Draw fresh bot RTT and directional split |
| Enter | Start / resume |
| Esc or F1 | Pause and release the cursor |
| R | Save current match and return to the same starting setup |
| Q while paused | Save and quit |
| [ / ] | Adjust sensitivity by 0.025 degrees per logical mouse pixel |
| - / = | Adjust vertical field of view by 5 degrees |
| F2 | Toggle render-only movement interpolation |
| F11 | Toggle fullscreen / windowed mode |
| V on the ready screen | Switch between human play and bot-vs-bot spectating |
| A / D on the spectator ready screen | Select Bot A's style |
| Tab while spectating | Switch first-person viewpoint between Bot A and Bot B |

Both players start with the sprint key held. Forward movement requests sprint when eligible. To W-tap after a sprint hit, release W for a sampled movement tick and press it again. The usual sprint-state packets and network delays apply. A release and press entirely between two 50 ms input samples can be missed.

Holding the mouse button does not auto-click entities. Multiple clicks between two 50 ms decisions coalesce into one click. On resume, press movement keys again.

## Practice opponents

The window starts with **Critical** selected. Use the arrow keys before pressing Enter, or launch with `--opponent orbit`. R saves the current match and returns to opponent selection. The selection stays fixed until the match ends or restarts.

| Opponent | What to practice |
| --- | --- |
| Critical | Track the approach jump. It releases W near the apex to land a critical. If the opening takes too long, it resumes forward sprinting and waits until its client observes sprint active before attempting a charged sprint hit. |
| Critical+ | A parameter-tuned variant of Critical, selected by simulated duels. Select with `--opponent critical-plus`. See [parameters and match results](../research/bots/critical-plus.md). |
| Duel-RL | A linear policy trained against Critical+ using simulated match rewards. Select with `--opponent duel-rl`. See [training and evaluation](../research/bots/duel-rl.md). |
| League-RL (G4) | Trained with realistic RTT against scripted bots, Duel-RL and its own current and historical policies. Select with `--opponent league-rl`. See [league training and evaluation](../research/bots/league-rl.md). |
| League-G5 | Adds short-term observation history and trains against G4 and its opponents at both population and matched RTT. Select with `--opponent league-g5`. See [opponent and RTT breakdown](../research/bots/league-rl.md). |
| League-G6 | Extends ARS training to 0–150 ms and varies when attacks reach the server tick. Select with `--opponent league-g6`. See [training and timing evaluation](../research/bots/league-g6.md). |
| Recurrent-PPO | A GRU policy trained with PPO and a bonus for HP remaining at victory. Select with `--opponent recurrent-ppo`. See [training and evaluation](../research/bots/recurrent-ppo-plan.md). |
| Pressure | Recover from knockback against a bot that keeps advancing, attacks at strong charge, and releases W for one tick after each swing. |
| Orbit | Keep the target under the crosshair as it strafes, changes direction and reverses after taking damage. |
| Counter | Manage reach against an opponent that steps back during recovery and closes again as the sword recharges. |
| Baseline | Repeat the original straight approach and fully charged attack for comparison. |

See the [match results for the five scripted practice bots](../research/bots/practice-opponents.md).

Reach experiments are archived and cannot be selected in the game or by CLI name. See [combo-research.md](../research/bots/combo-research.md) for the investigation.

## Spectating and keystrokes

Open the executable normally, press **V** on the ready screen to select spectator
mode, then **Enter** to start. Both seats are controlled by independent bots; the
default pairing is Critical vs Critical. Before starting, **A/D** selects Bot A
and **Left/Right** selects Bot B. **Tab** switches between their first-person
views, including while paused. **Esc** pauses, **Enter** resumes, and **R** returns
to setup while keeping the pairing, mode and selected viewpoint. The mouse is
not captured and manual movement/attack input does not control either bot.

The two keystroke panels show each bot's actual most recent 20 Hz policy input:
W/A/S/D, SPACE, SHIFT, CTRL and LMB. LMB clicks pulse for 120 ms for visibility.
CTRL reflects the simulator's always-held sprint key; **SPRINT: ON/OFF** shows
the resulting client sprint state, so a W-tap or coasting interval is visible.
Paused panels retain the last sampled input. These are bot decisions, not a
record of the spectator's physical keyboard. The yellow panel title identifies
the currently watched bot.

The camera, health HUD, hand item and effects use the selected bot's local and
received opponent view. Sound source IDs are mapped relative to that viewpoint;
switching discards pending old-listener audio and transient effects. Viewpoint
changes never modify either bot's observations or simulation state. Spectator
recordings store both bots' actual actions, `mode: "spectator"`, `human_seat:
null`, and Bot A's style, while retaining the existing opponent field for Bot B.

To start directly in spectator setup from a command line:

```powershell
.\tools\play\start.bat --bot-a league-g5 --opponent league-rl
```

## Display

- First-person camera at the local player's standing/crouching eye height.
- World FOV defaults to 100 degrees; the held-item camera retains its vanilla 70-degree projection.
- Opponent skin flashes red for half a second when its delayed hurt/death sound event arrives. This uses the existing client-visible event stream, independently of sound asset availability. Local health loss briefly adds a red screen tint.
- Green ground with a line at every integer block boundary; stronger lines every five blocks.
- The opponent is a navy training automaton with amber eyes, a copper scarf and turquoise armor. The helmet opening shows the eyes through both layers. The skin and equipment use original pixel art embedded in the executable. [Duelist and armor textures](../../crates/pvp-play/assets/player/README.md)
- Head pitch, facing direction, crouching and alternating walking limbs follow received opponent movement. Arms have gentle idle motion and a raised right-hand sword pose. The right-hand sword uses `ItemInHandLayer` and `handheld.json`'s third-person transform; attack sounds trigger a short approximate arm swing. The crosshair ready mark still indicates a reachable target.
- The opponent hitbox remains visible as an amber wireframe over the model, turning lime when the displayed ray can reach it. Eye-height and facing-direction lines are also drawn using the received pose.
- A three-dimensional pixel diamond sword appears at the lower right, with a six-tick attack swing based on local attack state. It uses the same original artwork as the hotbar. A separate 70-degree item camera and depth pass keep it visible at close range and across world FOV settings. If received equipment has no sword, a bare arm is displayed.

The sword's model coordinates and resting pose follow local Minecraft 26.1.2 references: `ItemModelGenerator` uses X/Y 0–16 and Z 7.5–8.5; `ItemTransform.apply` centers the model after applying `handheld.json`'s right-hand translation `[1.13, 3.2, 1.13] / 16`, XYZ rotation `[0, -90, 25]` degrees and scale `0.68`. The hand offset is `[0.56, -0.52, -0.72]`. Attack transforms preserve `ItemInHandRenderer`'s full Y/Z/X/Y rotation sequence, including its +45/-45 degree rotations. Numeric tests verify the resting center and orientation. View bobbing, camera-turn lag and item equip-height animation are not implemented; the sword is rendered at its fully raised equip height.
- A nine-slot hotbar sits at bottom center. Hearts and armor are above its left edge and full food icons are above its right edge. The experience bar stays empty in this duel profile. Health and equipment use received local state.
- The four-bracket sight has a segmented charge meter below it. A ready mark appears when the sword is charged and the opponent is in reach. The inversion shader keeps the sight visible against the arena. [Duel HUD sprites](../../crates/pvp-play/assets/hud/README.md)
- Critical hits from either player emit copper and gold sparks around the victim. The effect follows the delayed critical-attack event and approximates `TrackingEmitter`'s three-tick burst and `CritParticle`'s gravity, friction, color and lifetime. Effects freeze on pause, clear on restart and use a separate visual RNG.
- No authoritative opponent HP or current server position is shown. The renderer uses `Duel::client_view(0)`, which exposes local and received opponent bodies only.

The simulation hitbox still follows the received pose independently of the visual mesh. Opponent armor and sword appearance are cosmetic representations of the default diamond loadout; this client view does not expose remote equipment updates, so breakage is not reflected visually. Enchantment glint is not rendered. Remote attack animation is approximate and cannot show silent misses. No extra authoritative opponent state is read for visuals. Player contact pushing remains disabled. The ground has no destructible block data; its grid is visual. Rendering follows the camera over a finite patch of the infinite simulation plane.

## Combat sounds

Combat audio follows the server's attack decision for both players. Listen for these combinations:

| Action or result | Sound |
| --- | --- |
| Hit at 90% charge or less | Weak attack |
| Charged hit while falling and eligible for a critical | Critical attack |
| Charged sprint hit | Knockback, then strong attack |
| Charged grounded sword hit below the sweep speed threshold | Sweep |
| Other charged hit | Strong attack |
| Target rejects the damage during invulnerability | No-damage cue, preceded by knockback if it was a charged sprint attack |
| Full hit on the victim | Hurt or death, alongside the attacker's cue |
| Miss or server range rejection | Silence |

The app synthesizes an original sound for each of the eight combat cues at startup.

Both players' sounds reach the human after the configured downstream delay, rounded to the next simulated client frame. Playback starts on a render frame once the corresponding server decision has been simulated. A lethal strike's pending sounds can finish on the result screen. Distance reduces the opponent's volume.

Both seats use the same sounds. Gain is `0.7 * 0.75 * clamp(1 - distance / 16, 0, 1)`. Local sounds use zero distance, and remote sounds use the current received separation. A hit mixes the attacker's cue with the victim's hurt or death cue.

## Timing

Physics remains fixed at 20 ticks/second. A monotonic wall clock feeds a 50 ms accumulator; the simulator is not changed to variable-time physics. Mouse orientation updates at render rate and is sampled into actions at decision boundaries. The internal simulated client receive frames still follow the configured FPS and phases, independently of actual graphics FPS.

Movement interpolation smooths the previous/current local and remote positions. It adds up to one decision interval of visual lag but does not change combat, networking or replay. F2 disables it to inspect exact discrete positions. Crosshair highlighting describes the displayed geometry; attack acceptance uses the next logical tick and its client/server states, so a highlight does not guarantee a hit.

Pause stops both players and all simulated network time. On Windows, loss of foreground focus automatically pauses. A render stall longer than 250 ms also pauses instead of fast-forwarding through missed human input. Normal short frame delays retain their accumulator remainder. The Windows focus check uses read-only Win32 calls; automatic focus-loss pausing on other platforms is not implemented.

This is a playable simulator frontend, useful for judging feel and collecting human interaction. It is not itself a Minecraft equivalence test. Input sampling, render interpolation, camera smoothing, FOV behavior, animations and missing game effects differ from the actual game. Existing simulation boundaries still apply; see [Phase 2](../simulator/implementation.md).

## Local recordings

The first resume creates a unique directory under `recordings/` by default. If recording fails, the match continues and the HUD displays **REC OFF**. Press F1 to see the diagnostic log path. The app attempts to write `errors.log` in the recording directory, then in the system temporary directory under `pvp-play/`. Restarting the match retries recording from its first action.

| File | Contents |
| --- | --- |
| `metadata.json` | Format version, configuration, seed, human seat, opponent identity and input rate |
| `transitions.ndjson` | Both client observations before each decision, six-key policy actions, and the resulting transition |
| `replay.json` | Existing versioned simulator action replay; written on completion, restart or clean exit |
| `result.json` | Exit reason and privileged final diagnostic snapshot |

Transitions flush every 20 decisions and on clean exit. An abrupt process termination can lose buffered data; the final replay/result may not yet exist. Pausing alone does not end the recording. The final snapshot contains privileged state and is for diagnostics, not actor input.

Replay a completed or cleanly interrupted recording with:

```powershell
.\tools\rust\cargo.bat run --release --locked -p pvp-cli -- replay --input recordings/<session>/replay.json
```

The files provide a starting dataset for later human-in-the-loop training. This app does not update a policy during a match or implement an RLHF trainer.

## Implementation and checks

Graphics/input use [Macroquad 0.4.16](https://docs.rs/macroquad/0.4.16/macroquad/); combat and networking stay in `pvp-sim`. The workspace's default Cargo member is `pvp-cli`, so headless simulation commands do not start a window.

The Windows GNU build forwards Miniquad's `imm32` import-library name to the equivalent library already shipped by its WinAPI dependency. This supports the workspace-local minimal Rust/MinGW toolchain without a system SDK installation.

```powershell
.\tools\rust\check.bat
# Automated renderer smoke run: opens a window, records synthetic controls,
# writes a PNG after 120 frames, then exits.
.\tools\play\start.bat --opponent league-g5 --smoke-test build/play/smoke.png --record-dir build/play/smoke-recordings
```

Rust tests cover real-time accumulator behavior at multiple render rates, pause/stall behavior, mouse angle handling, local view access, and exact replay of recorded actions. Native window and input verification is recorded in [validation.md](validation.md).
