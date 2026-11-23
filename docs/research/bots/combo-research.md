# Combo investigation: 3-block first-hit-waiting Baseline

Target: Reach starts at 10 HP, Baseline at 20 HP. Baseline pursues normally
(forward beyond 2.4 center-distance, backward below 1.5), uses a 3.0 eye-to-box
attack threshold, and does not attack until its own received health first drops.
Jump inputs remain forbidden. Strafing is allowed. The ordinary Baseline and
production Reach policy were not changed during this investigation.

## Mechanism

The local 26.1.2 Player.java is the primary reference. Around lines 950–981,
attack strength scales damage and gates strong attacks; sprinting enables extra
knockback. Around lines 1113–1124, extra knockback slows the attacker's horizontal
velocity to 60% and cancels sprint. W/S release and renewed forward input must
therefore be timed around the hit and next attack. A fixed S delay alone does
not guarantee a combo. Spacing, relative motion, airborne state and attack
recovery matter. Horizontal movement can also alter spacing and aiming in play.

The simulator has a 3-block client selection check but a permissive server
distance check (6 blocks), and its bots retarget automatically each decision
tick. An accepted server attack does not recheck the aim ray. These boundaries
must be considered when translating human combo advice into this model; they
do not, by themselves, prove that the target is impossible. Mojang's historical
MC-11608 report also documents sprint canceling on a sprint-knockback attack:
https://bugs.mojang.com/browse/MC-11608

## Screening result

`reach_combo_tune` screens 47,520 combinations of chase/backoff distances,
charge thresholds, post-click stop/S durations, strafing patterns and airborne
attack gating. Policies use only their client observations. One seed (42),
Reach in seat A, and the default client order are used for candidate discovery;
this is not a multi-seed robustness test. No candidate won. The best final HP
difference was approximately -7.06. This does not constitute a proof of
impossibility or a successful new Reach version.

Reproduce with:

```powershell
.\tools\rust\cargo.bat run --release --locked -p pvp-rl --example reach_combo_tune
```

Report: `build/benchmarks/reach-combo-3block-screening.json`.
No new UI executable was published for these unsuccessful experimental policies.
