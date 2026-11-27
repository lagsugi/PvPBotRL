# Measured effect of ping on match outcomes (November 6, 2026)

Critical+ mirror matches and bundled Duel-RL mirror matches totaled **112,128 games**, with zero timeouts. Ping means **RTT = upstream + downstream delay**. Baseline comparisons split RTT equally between the two directions.

The lower-ping player won **67.56%** of Critical+ matches and **74.27%** of Duel-RL matches on average. Each of the 105 unequal-ping pairs contributed 256 matches with equal weight. Both policies reached at least 50% on the lower-ping side in every pair, but win rate varied nonmonotonically with the gap. For example, Critical+ won 78.13% at 10 vs 100 ms and 74.22% at 10 vs 150 ms. These results apply to the tested simulator policies and conditions. Live Minecraft remains unverified.

## Representative matchups

Win rates are for the lower-ping player. Each row contains 256 matches per policy. The 95% intervals use 5,000 cluster bootstrap samples over 64 conditions. The four seat and processing-order variants within a condition belong to the same cluster.

|RTT (low vs high)|Critical+ wins–losses|Win rate / 95% interval|Duel-RL wins–losses|Win rate / 95% interval|
|---|---:|---:|---:|---:|
|10 vs 50 ms|174–82|67.97% / 58.6–77.3%|150–106|58.59% / 50.0–67.2%|
|10 vs 100 ms|200–56|78.13% / 69.5–85.9%|180–76|70.31% / 62.5–78.1%|
|10 vs 150 ms|190–66|74.22% / 65.6–82.8%|228–28|89.06% / 82.8–94.5%|
|50 vs 100 ms|164–92|64.06% / 54.7–72.7%|208–48|81.25% / 73.4–87.5%|
|50 vs 150 ms|164–92|64.06% / 55.5–72.7%|244–12|95.31% / 91.4–98.4%|
|100 vs 150 ms|164–92|64.06% / 54.7–72.7%|152–104|59.38% / 50.0–68.8%|

## Upstream/downstream split at fixed RTT

At each RTT, vary the tested player's upstream share across 10, 25, 50, 75, and 90%, keeping the opponent at the same RTT with a 50:50 split. Total RTT stays constant. The results for all RTTs are in summary.csv.

At 100 ms, an additional sweep varies upstream delay from 5 to 95 ms in 5 ms increments. The opponent always has 50 ms each way. Reducing upstream delay increases downstream delay, so this comparison cannot isolate their independent causal effects.

|Tested upstream / downstream|Critical+ win rate|Duel-RL win rate|
|---|---:|---:|
|5 / 95 ms|51.56%|48.44%|
|10 / 90 ms|45.31%|42.19%|
|15 / 85 ms|49.22%|44.53%|
|20 / 80 ms|47.66%|44.53%|
|25 / 75 ms|49.22%|48.44%|
|30 / 70 ms|46.09%|47.66%|
|35 / 65 ms|47.66%|47.66%|
|40 / 60 ms|51.56%|51.56%|
|45 / 55 ms|48.44%|50.78%|
|50 / 50 ms|50.00%|50.00%|
|55 / 45 ms|48.44%|47.66%|
|60 / 40 ms|39.06%|43.75%|
|65 / 35 ms|43.75%|46.88%|
|70 / 30 ms|42.97%|46.88%|
|75 / 25 ms|43.75%|46.09%|
|80 / 20 ms|41.41%|44.53%|
|85 / 15 ms|42.19%|46.88%|
|90 / 10 ms|46.88%|50.00%|
|95 / 5 ms|45.31%|45.31%|

At 100 ms RTT, Critical+ ranged from 39.06% to 51.56% and Duel-RL from 42.19% to 51.56%. Changing the split by 5–10 ms affected performance. Shorter upstream or shorter downstream delay gave no consistent advantage. Small differences include finite-sample variation.

In the coarse sweep over all RTTs, Critical+ won 61.72% at 40 ms RTT (36 up/4 down) and 39.06% at 90 ms (81 up/9 down). Duel-RL won 63.28% at 70 ms (52.5 up/17.5 down) and 42.19% at 130 ms (97.5 up/32.5 down). Each opponent used an equal split at the same RTT. These are observed extremes among the tested conditions. The optimal split remains unknown.

## All pairings

Rows give the player's RTT, columns the opponent's RTT, and cells the row player's win percentage. Diagonal cells are 50% (128 wins, 128 losses). Transposed cells show the other side of the same matches and add no independent games.

### Critical+ mirror matches

|Row / column RTT (ms)|10|20|30|40|50|60|70|80|90|100|110|120|130|140|150|
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
|10|50.0|54.7|66.4|63.3|68.0|70.3|73.4|75.8|75.8|78.1|78.9|82.0|78.1|74.2|74.2|
|20|45.3|50.0|57.8|60.2|57.8|64.1|68.8|70.3|73.4|74.2|76.6|80.5|75.8|71.1|71.9|
|30|33.6|42.2|50.0|53.1|59.4|60.9|65.6|67.2|68.8|71.9|73.4|78.1|73.4|71.1|68.0|
|40|36.7|39.8|46.9|50.0|52.3|62.5|60.9|63.3|66.4|66.4|71.1|72.7|69.5|69.5|70.3|
|50|32.0|42.2|40.6|47.7|50.0|60.2|61.7|60.9|60.9|64.1|64.1|68.8|65.6|64.1|64.1|
|60|29.7|35.9|39.1|37.5|39.8|50.0|55.5|60.2|62.5|66.4|71.1|70.3|70.3|67.2|68.8|
|70|26.6|31.3|34.4|39.1|38.3|44.5|50.0|53.9|62.5|67.2|71.9|75.8|75.0|71.1|70.3|
|80|24.2|29.7|32.8|36.7|39.1|39.8|46.1|50.0|53.9|64.1|69.5|72.7|76.6|72.7|71.1|
|90|24.2|26.6|31.3|33.6|39.1|37.5|37.5|46.1|50.0|57.0|71.9|76.6|77.3|74.2|69.5|
|100|21.9|25.8|28.1|33.6|35.9|33.6|32.8|35.9|43.0|50.0|57.8|71.9|70.3|68.8|64.1|
|110|21.1|23.4|26.6|28.9|35.9|28.9|28.1|30.5|28.1|42.2|50.0|58.6|69.5|66.4|68.0|
|120|18.0|19.5|21.9|27.3|31.3|29.7|24.2|27.3|23.4|28.1|41.4|50.0|56.3|64.8|64.8|
|130|21.9|24.2|26.6|30.5|34.4|29.7|25.0|23.4|22.7|29.7|30.5|43.8|50.0|57.0|66.4|
|140|25.8|28.9|28.9|30.5|35.9|32.8|28.9|27.3|25.8|31.3|33.6|35.2|43.0|50.0|56.3|
|150|25.8|28.1|32.0|29.7|35.9|31.3|29.7|28.9|30.5|35.9|32.0|35.2|33.6|43.8|50.0|

### Duel-RL mirror matches

|Row / column RTT (ms)|10|20|30|40|50|60|70|80|90|100|110|120|130|140|150|
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
|10|50.0|58.6|64.8|60.2|58.6|68.0|70.3|72.7|71.9|70.3|69.5|71.1|73.4|84.4|89.1|
|20|41.4|50.0|54.7|60.2|55.5|69.5|77.3|79.7|81.3|79.7|83.6|86.7|85.9|88.3|93.0|
|30|35.2|45.3|50.0|54.7|60.2|68.8|79.7|83.6|82.0|80.5|84.4|87.5|88.3|92.2|93.8|
|40|39.8|39.8|45.3|50.0|53.9|71.1|75.0|82.8|83.6|80.5|87.5|91.4|93.0|97.7|97.7|
|50|41.4|44.5|39.8|46.1|50.0|63.3|78.1|80.5|81.3|81.3|87.5|91.4|92.2|95.3|95.3|
|60|32.0|30.5|31.3|28.9|36.7|50.0|60.9|72.7|74.2|75.8|81.3|85.9|88.3|89.8|91.4|
|70|29.7|22.7|20.3|25.0|21.9|39.1|50.0|58.6|71.9|71.9|78.1|82.0|82.8|82.0|81.3|
|80|27.3|20.3|16.4|17.2|19.5|27.3|41.4|50.0|58.6|71.1|73.4|75.8|76.6|74.2|74.2|
|90|28.1|18.8|18.0|16.4|18.8|25.8|28.1|41.4|50.0|57.0|70.3|71.9|74.2|68.8|64.8|
|100|29.7|20.3|19.5|19.5|18.8|24.2|28.1|28.9|43.0|50.0|58.6|66.4|62.5|60.9|59.4|
|110|30.5|16.4|15.6|12.5|12.5|18.8|21.9|26.6|29.7|41.4|50.0|57.8|63.3|56.3|57.8|
|120|28.9|13.3|12.5|8.6|8.6|14.1|18.0|24.2|28.1|33.6|42.2|50.0|50.8|56.3|51.6|
|130|26.6|14.1|11.7|7.0|7.8|11.7|17.2|23.4|25.8|37.5|36.7|49.2|50.0|53.1|57.0|
|140|15.6|11.7|7.8|2.3|4.7|10.2|18.0|25.8|31.3|39.1|43.8|43.8|46.9|50.0|53.1|
|150|10.9|7.0|6.3|2.3|4.7|8.6|18.8|25.8|35.2|40.6|42.2|48.4|43.0|46.9|50.0|

## In-match behavior and interpretation

The 2,560 traced matches were additional diagnostic reruns. All outcomes, HP values, and end times matched the corresponding untraced matches. At 10 vs 150 ms, mean successful attacks were 20.70 vs 19.81 for Critical+ and 24.80 vs 23.95 for Duel-RL. Mean critical hits were 15.54 vs 15.68 for Critical+ and 11.76 vs 11.09 for Duel-RL. Lower-ping Critical+ won more often despite landing fewer critical hits, so critical-hit count alone cannot explain the win rate.

Upstream delay postpones server receipt of attacks and movement. Downstream delay postpones client receipt of opponent information and other updates. Both interact with 50 ms ticks, roughly 16.67 ms receive frames, position interpolation, knockback, and attack charge. The nonmonotonic results are consistent with timing effects. This experiment did not estimate individual contributions. Diagnostic conditions recorded zero server attack rejections for range or similar checks and zero damage-invulnerability rejections, giving no evidence for increased rejections as the cause.

## Conditions and reproduction

- 64 shared conditions × two seats × two processing orders for simultaneous inputs = 256 matches per condition. Reuse the same 64 starting conditions across all pings and splits.
- Starting distance 3.5–32 blocks, lateral offset −2 to 2 blocks. A fixed seed generates server and both client tick phases in [0, 50 ms) and receive-frame phases within one frame. Both clients use 60 FPS.
- Start at 20 HP with ordinary gear and finite durability, maximum 300 seconds. Policies and models were unchanged throughout. Network delays are constant with no jitter or packet loss.
- Per policy: 120 RTT grid conditions, 75 fixed-RTT splits, 19 detailed splits at 100 ms, and five diagnostic conditions = 219. Both policies total 438 conditions and 112,128 matches, including reruns. The main grid comparison covers 61,440 matches.
- The 64 conditions sample the simulator. Intervals omit error relative to live Minecraft and untested conditions. The reported extremes have no multiple-comparison adjustment.

```powershell
.\tools\rust\cargo.bat run --release --locked --offline -p pvp-rl --example ping_sweep
python tools/research/ping/analyze_ping.py
.\tools\research\ping\plot-ping.bat
```

Raw data and individual matches: `build/benchmarks/ping-sweep/results.json`. The 64 reproducible conditions: `contexts.json`. All-condition summary: `summary.csv`. Episode hits, crits, damage, and similar counters were measured only in diagnostics. Zeros elsewhere indicate missing measurements.
