# Duel window validation

The original Windows renderer smoke run created a frame with the one-block ground grid, opponent hitbox, crosshair and health display. A human subsequently launched the executable and played a duel. The first reported Enter error predates persistent diagnostics. Its cause remains unknown.

The Rust tests exercise recording failures, pause and resume, fixed-step timing, and action replay. The sound tests cover accepted and rejected hits, stacked sprint and strong effects, criticals, the sweep speed boundary, and delayed client delivery. The audio test decodes the eight synthesized cues and checks that their PCM samples are distinct and audible.

```powershell
.\tools\rust\cargo.bat test --workspace --all-targets --locked
.\tools\rust\cargo.bat test --locked -p pvp-play synthesized_effects_are_distinct_audible_pcm
```

To compare sounds on an audio device, try a charged grounded stationary attack, a charged sprint attack and a charged falling attack after releasing W. Swing at empty air once to check for silence.
