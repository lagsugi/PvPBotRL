"""Summarize the saved Rust ping sweep with the Python standard library."""

import csv
import json
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
OUTPUT = ROOT / "build/benchmarks/ping-sweep"
PINGS = range(10, 151, 10)
STYLES = {"critical-plus": "Critical+", "duel-rl": "Duel-RL"}
CLUSTERS = 64
MATCHES_PER_CLUSTER = 4

REPORT = r"""# Measured effect of ping on match outcomes (November 6, 2026)

Critical+ mirror matches and bundled Duel-RL mirror matches totaled **112,128 games**, with zero timeouts. Ping means **RTT = upstream + downstream delay**. Baseline comparisons split RTT equally between the two directions.

The lower-ping player won **67.56%** of Critical+ matches and **74.27%** of Duel-RL matches on average. Each of the 105 unequal-ping pairs contributed 256 matches with equal weight. Both policies reached at least 50% on the lower-ping side in every pair, but win rate varied nonmonotonically with the gap. For example, Critical+ won 78.13% at 10 vs 100 ms and 74.22% at 10 vs 150 ms. These results apply to the tested simulator policies and conditions. Live Minecraft remains unverified.

## Representative matchups

Win rates are for the lower-ping player. Each row contains 256 matches per policy. The 95% intervals use 5,000 cluster bootstrap samples over 64 conditions. The four seat and processing-order variants within a condition belong to the same cluster.

|RTT (low vs high)|Critical+ wins–losses|Win rate / 95% interval|Duel-RL wins–losses|Win rate / 95% interval|
|---|---:|---:|---:|---:|
{representative}

## Upstream/downstream split at fixed RTT

At each RTT, vary the tested player's upstream share across 10, 25, 50, 75, and 90%, keeping the opponent at the same RTT with a 50:50 split. Total RTT stays constant. The results for all RTTs are in summary.csv.

At 100 ms, an additional sweep varies upstream delay from 5 to 95 ms in 5 ms increments. The opponent always has 50 ms each way. Reducing upstream delay increases downstream delay, so this comparison cannot isolate their independent causal effects.

|Tested upstream / downstream|Critical+ win rate|Duel-RL win rate|
|---|---:|---:|
{splits}

At 100 ms RTT, Critical+ ranged from 39.06% to 51.56% and Duel-RL from 42.19% to 51.56%. Changing the split by 5–10 ms affected performance. Shorter upstream or shorter downstream delay gave no consistent advantage. Small differences include finite-sample variation.

In the coarse sweep over all RTTs, Critical+ won 61.72% at 40 ms RTT (36 up/4 down) and 39.06% at 90 ms (81 up/9 down). Duel-RL won 63.28% at 70 ms (52.5 up/17.5 down) and 42.19% at 130 ms (97.5 up/32.5 down). Each opponent used an equal split at the same RTT. These are observed extremes among the tested conditions. The optimal split remains unknown.

## All pairings

Rows give the player's RTT, columns the opponent's RTT, and cells the row player's win percentage. Diagonal cells are 50% (128 wins, 128 losses). Transposed cells show the other side of the same matches and add no independent games.

{pairings}

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
"""


def rate(group, outcome="wins"):
    return 100 * group[outcome] / len(group["episodes"])


def rounded(value, places):
    # Preserve the archived report's rounding, including exact halfway values.
    return str(Decimal.from_float(value).quantize(Decimal(1).scaleb(-places), rounding=ROUND_HALF_UP))


def interval(group, state):
    wins = [0] * CLUSTERS
    for episode in group["episodes"]:
        wins[episode["variant"]] += episode["winner_role"] == 0
    samples = []
    for _ in range(5000):
        total = 0
        for _ in range(CLUSTERS):
            state = (state * 1664525 + 1013904223) & 0xFFFFFFFF
            total += wins[state * CLUSTERS >> 32]
        samples.append(100 * total / (CLUSTERS * MATCHES_PER_CLUSTER))
    samples.sort()
    return f"{rounded(samples[125], 1)}–{rounded(samples[4874], 1)}", state


def validate(groups):
    count = CLUSTERS * MATCHES_PER_CLUSTER
    for group in groups:
        condition = group["condition"]
        reported = group["wins"] + group["losses"] + group["timeouts"]
        if len(group["episodes"]) != count or reported != count:
            raise ValueError(f"Invalid match counts for {condition}")
        if condition["family"] == "rtt-grid" and condition["up_ms"][0] == condition["up_ms"][1]:
            if group["wins"] != count // 2:
                raise ValueError(f"Asymmetric control for {condition}")
        if not condition["diagnostics"]:
            continue
        for control in groups:
            other = control["condition"]
            if other["diagnostics"] or other["style"] != condition["style"]:
                continue
            delays = zip(other["up_ms"] + other["down_ms"], condition["up_ms"] + condition["down_ms"], strict=True)
            if all(abs(a - b) < 1e-7 for a, b in delays):
                break
        else:
            raise ValueError(f"Missing untraced control for {condition}")

        for traced, untraced in zip(group["episodes"], control["episodes"], strict=True):
            if any(traced[field] != untraced[field] for field in ("winner_role", "hp", "seconds")):
                raise ValueError(f"Trace altered outcomes for {condition}")


def write_summary(groups):
    with (OUTPUT / "summary.csv").open("w", encoding="utf-8", newline="") as stream:
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow(("style", "family", "role0_up_ms", "role0_down_ms", "role1_up_ms", "role1_down_ms",
                         "wins", "losses", "timeouts", "win_pct", "mean_hp_difference", "mean_seconds"))
        for group in groups:
            condition = group["condition"]
            up_a, up_b = condition["up_ms"]
            down_a, down_b = condition["down_ms"]
            row = (condition["style"], condition["family"], up_a, down_a, up_b, down_b, group["wins"],
                   group["losses"], group["timeouts"], rate(group), group["mean_hp_difference"], group["mean_seconds"])
            writer.writerow(int(value) if isinstance(value, float) and value.is_integer() else value for value in row)


def render_report(groups):
    grid = {}
    fine_splits = {}
    for group in groups:
        condition = group["condition"]
        style = condition["style"]
        a, b = condition["up_ms"]
        if condition["family"] == "rtt-grid":
            grid[style, a * 2, b * 2] = group
        elif condition["family"] == "100ms-fine-offset":
            fine_splits[style, a] = group

    state = 20261106
    representative = []
    for a, b in ((10, 50), (10, 100), (10, 150), (50, 100), (50, 150), (100, 150)):
        cells = [f"{a} vs {b} ms"]
        for style in STYLES:
            group = grid[style, a, b]
            bounds, state = interval(group, state)
            cells.extend((f"{group['wins']}–{group['losses']}", f"{rounded(rate(group), 2)}% / {bounds}%"))
        representative.append("|" + "|".join(cells) + "|")

    splits = []
    for upstream in range(5, 96, 5):
        cells = [f"{upstream} / {100 - upstream} ms"]
        cells.extend(f"{rounded(rate(fine_splits[style, upstream]), 2)}%" for style in STYLES)
        splits.append("|" + "|".join(cells) + "|")

    pairings = []
    for style, name in STYLES.items():
        columns = "|".join(map(str, PINGS))
        pairings.extend((f"### {name} mirror matches", "", f"|Row / column RTT (ms)|{columns}|",
                         "|---|" + "---:|" * len(PINGS)))
        for a in PINGS:
            cells = [str(a)]
            for b in PINGS:
                group = grid[style, min(a, b), max(a, b)]
                cells.append(rounded(rate(group, "wins" if a <= b else "losses"), 1))
            pairings.append("|" + "|".join(cells) + "|")
        pairings.append("")

    return REPORT.format(representative="\n".join(representative), splits="\n".join(splits),
                         pairings="\n".join(pairings).rstrip("\n"))


def main():
    groups = json.loads((OUTPUT / "results.json").read_text(encoding="utf-8"))
    validate(groups)
    report = render_report(groups)
    write_summary(groups)
    for path in (ROOT / "docs/research/ping/ping-impact.md", OUTPUT / "report.md"):
        path.write_text(report, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
