# Duel window validation

The original Windows renderer smoke run created a frame with the one-block ground grid, opponent hitbox, crosshair and health display. A human subsequently launched the executable and played a duel. The first reported Enter error predates persistent diagnostics. Its cause remains unknown.

The Rust tests exercise recording failures, pause and resume, fixed-step timing, and action replay. The sound tests cover accepted and rejected hits, stacked sprint and strong effects, criticals, the sweep speed boundary, delayed client delivery and synthesized audio. The installed-cache test separately decoded all variants across the eight combat events from the local 26.1.2 asset index.

```powershell
.\tools\rust\cargo.bat test --workspace --all-targets --locked
.\tools\rust\cargo.bat test --locked -p pvp-play installed_combat_clips_decode -- --ignored
```

To compare sounds on an audio device, try a charged grounded stationary attack, a charged sprint attack and a charged falling attack after releasing W. Swing at empty air once to check for silence.
