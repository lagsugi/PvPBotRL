# Ping settings for play and spectating

In human matches, **your RTT defaults to 20 ms and the opponent draws a new RTT each match**. In spectator mode the two bots draw independently. The ready screen shows RTT and both one-way delays for each seat.

## Usage

```powershell
# Human match with 20 ms RTT and a gamma-distributed opponent RTT
.\tools\play\start.bat --opponent critical-plus
.\tools\play\start.bat --opponent duel-rl

# Set your RTT to 75 ms. Add --bot-ping 100 to fix the opponent's RTT too.
.\tools\play\start.bat --opponent duel-rl --human-ping 75

# Watch two bots with sampled RTTs. On the ready screen use A/D for Bot A and the arrow keys for Bot B.
.\tools\play\start.bat --spectate --opponent critical-plus

# Simulate bot matches in virtual time with independent RTT draws.
.\tools\rust\cargo.bat run --release --locked --offline -p pvp-cli -- batch --ping-profile gamma --bot-a critical-plus --bot-b critical-plus --matches 1000 --seed 42 --network-output build/ping/critical-network.json
.\tools\rust\cargo.bat run --release --locked --offline -p pvp-cli -- batch --ping-profile gamma --bot-a duel-rl --bot-b duel-rl --matches 1000 --seed 42

# Compare equal one-way delays (0 gives 50:50) with wider splits (0.2 gives 30:70 to 70:30).
.\tools\play\start.bat --ping-asymmetry 0
.\tools\play\start.bat --ping-asymmetry 0.2

# Legacy settings: 10+10 ms for both seats and the same seed for rematches.
.\tools\play\start.bat --ping-profile fixed
```

On the ready screen, **↑/↓** changes your RTT by 1 ms and **Shift+↑/↓** by 10 ms. Press **N** to draw again or **R** to save the match and prepare a rematch. In sampling mode both keys increment the seed by 1 and redraw the opponent's RTT and directional split. Your RTT setting persists. Settings lock when the match starts. **V** switches to spectating and samples Bot A too. Returning to human play restores your fixed RTT. The launch option `--human-ping` applies only to the human seat. Its range of 0–10,000 ms is independent of the bot distribution.

The app honors `--config`. Use `configs/human-ping.json` for a 20 ms human and a sampled bot, or `configs/realistic-ping.json` to sample both seats. The latter also samples the human in human matches. To fix the human at 20 ms, use human-ping.json or `--human-ping 20`. The simulator CLI defaults to legacy fixed delays. Enable sampling with `--ping-profile gamma` or a config file, and fix either seat's RTT with `--ping-a/--ping-b`. Explicit CLI options override the config.

See the [RTT and directional split model](../research/ping/realistic-ping.md) and [win rates by ping](../research/ping/ping-impact.md) for the network assumptions and their effect on matches.
