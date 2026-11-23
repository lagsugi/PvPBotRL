# Modeling ping on real connections (November 6, 2026)

For settings see [ping in play and spectator mode](../../play/ping.md). For measured outcomes see [win rates by ping](ping-impact.md).

## RTT distribution

Use the user-supplied population distribution **Gamma(k=2, θ=35 ms)** conditioned on **5 ≤ RTT ≤ 150 ms**. Sample by inverting the truncated CDF so probability does not accumulate at the bounds. The original mean is 70 ms. Truncation changes the mean.

This distribution represents differences between players. Draw once per match and keep RTT constant for every packet and tick. Independent seeded streams per match and seat make the draw reproducible. Record actual delays, profile, and seed so replay can reproduce the conditions.

## How much do TCP upstream and downstream delays differ?

**RTT alone cannot identify one-way asymmetry, and no single difference in milliseconds is typical for all connections.** Routes and queues can differ by direction, so asymmetry depends on destination, ISP, and congestion. Accurate one-way measurements require synchronized endpoint clocks and an error estimate. [IETF RFC 7679](https://www.rfc-editor.org/rfc/rfc7679.html)

A 2008 study using 2007 PlanetLab measurements found one-way/RTT ratios below 0.4 and above 0.6 on paths including commercial networks. Under large route asymmetry it reported variation from 0.3 to 0.7. One example had 150 ms RTT, 60/90 ms one-way delays, and a 30 ms difference. These historical IP measurements do not directly characterize Japanese Minecraft players or TCP application delays in 2026. [A Measurement Study of Internet Delay Asymmetry, §3](https://web.eecs.umich.edu/~zmao/Papers/pam08_owd.pdf)

TCP supports bidirectional transfer without guaranteeing equal one-way delays. Retransmissions, send batching, and receiver scheduling can also affect application-visible delay. ACK wait time alone cannot measure one-way packet arrival delay. [TCP specification, RFC 9293](https://www.rfc-editor.org/rfc/rfc9293.html)

## Delay split model and limitations

The study informs this **configurable assumption**. The model has not been fitted to a measured population distribution.

- Upstream fraction `f = 0.5 + a × (U1−U2)`. U1 and U2 are independent uniform draws, producing a triangular distribution centered at 50%.
- The default `a=0.1` gives 40:60 to 60:40. These frequencies are modeling assumptions. `a=0` gives perfect symmetry and `a=0.2` gives 30:70 to 70:30. The allowed parameter range is 0–0.5.
- Upstream = `RTT×f`, downstream = `RTT − upstream`. Preserve total RTT after rounding to nanoseconds.
- Within the default range the largest directional difference is 20% of RTT: 4 ms at 20 ms, 14 ms at 70 ms, and 30 ms at 150 ms. The triangular distribution favors values near the center.

This model combines the supplied population distribution with configurable fixed one-way delays. It omits a TCP stack with loss, retransmission, head-of-line blocking, Nagle, time-varying jitter, and congestion correlations. Measurements by direction on a target server could replace the split assumption.

## Verification

Across 10,000 seeds, checked truncation bounds, agreement with the theoretical CDF at multiple points, split bounds, and reproducibility. Tested fixed human RTT, stability of the opponent draw after changing player settings, rematch draws, switching spectator mode, recording JSON round trips, and replay agreement. The existing physics and combat rules were preserved.

The workspace passed 80 tests and skipped one existing test requiring a Minecraft audio cache. Clippy for all targets and format checks passed. A release build ran 1,000 mirror matches each for Critical+ and Duel-RL with zero timeouts. The shared sample of 2,000 player RTTs ranged from 5.281 to 149.839 ms with mean 60.181 ms. Results are in `build/ping/critical-network.json` and `rl-network.json`. A UI smoke test with a fixed human RTT of 20 ms and a sampled opponent finished successfully and was visually checked. CLI recording and replay agreed on outcome, HP, position, and end time.
