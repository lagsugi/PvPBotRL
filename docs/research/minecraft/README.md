# Phase 1 — Minecraft Java Edition 26.1.2 sword PvP specification

Analysis date: November 5, 2026 (Asia/Tokyo)

Scope: Java Edition **26.1.2 official client and dedicated server**, Survival, diamond sword, full Protection IV diamond armor, 1v1.

## Findings

A standalone Rust simulator is feasible. It needs **server state for both players, each client's local player and replica of the opponent, and bidirectional packet queues**. Clients compute player movement. The server validates and applies movement packets and processes attacks. The victim's client incorporates knockback into movement after receiving a velocity packet.

Phase 1 examined generated 26.1.2 sources, identified official JARs, and documented call order, formulas, boundaries, permissible omissions, and a verification plan for the port. It also added executable numerical probes. **Live Minecraft match trace comparisons, Rust implementation, training, and benchmarking were outside Phase 1.** Simulation fidelity had yet to be demonstrated.

## Documents

| Document | Contents |
| --- | --- |
| [combat.md](combat.md) | Equipment, cooldowns, critical hits, invulnerability, mitigation, knockback, and durability |
| [movement.md](movement.md) | WASD, sprint, jump, sneak, floor, and numeric types |
| [network-and-ticks.md](network-and-ticks.md) | Tick order, reach, packets, 20 ms ping, and opponent interpolation |
| [implementation-contract.md](implementation-contract.md) | Minimum state, actions and observations, proposed settings, open questions, and differential tests |
| [source-index.md](source-index.md) | Supporting classes, methods, line numbers, and reproduction steps |
| [evidence/provenance.json](evidence/provenance.json) | Target JAR, source JAR, and bundled library hashes and versions |
| [evidence/probe-results.json](evidence/probe-results.json) | Numerical results from calling official classes and scope of the probes |

## Findings that affect implementation

1. **Attacks use `ServerboundAttackPacket`.** Porting the older Interact ATTACK branch would use the wrong protocol. The attack packet contains a target ID and omits aim direction and attack time.
2. **Client selection and server acceptance have different ranges.** For a sword, the distance along the client's view ray to its intersection with the opponent AABB must be strictly below 3. The server allows an eye-to-AABB minimum distance up to 6 inclusive. Use the client limit for legal bot actions.
3. **Ping affects more than attacks.** Delay also affects movement, view rotation, sprinting, received knockback, and opponent updates. Model server receive boundaries, client receive frames, the nominal two-tick tracker interval, and three-step interpolation.
4. **The specified armor gives variable mitigation.** A fully charged normal hit costs approximately **0.6804 HP** and a critical hit **1.1529 HP**. Retain fractional HP.
5. **A stronger hit during invulnerability mitigates the difference in raw damage.** Subtracting the two independently mitigated values gives a different result.
6. **A sprint hit applies normal knockback of 0.4 and then an extra 0.5.** Each call halves the previous horizontal velocity. Simply adding 0.9 misses this effect.
7. **A miss can desynchronize the two cooldowns.** The client resets its timer. Receiving Swing alone leaves the server attack timer unchanged.
8. **Velocity packets use `LpVec3`.** The legacy three-short encoding with scale 8,000 and ±3.9 limit does not describe 26.1.2.
9. **Pin small-velocity cutoffs, diagonal-input correction, and fall damage to this version.** Older releases use different constants and formulas.

The linked documents identify the source for each finding.

## Target version and evidence

“1.9+” identifies a combat style. Physics and networking vary across those releases. The initial conformance target is **vanilla 26.1.2**. Paper, Spigot, PvP plugins, anti-cheat, and custom knockback require separate profiles. Record disabled player pushing, fixed hunger, and disabled natural regeneration as user-requested changes.

Evidence labels used in these documents:

- **Source-verified.** Observed in local generated 26.1.2 sources or data from the official JAR.
- **Numerically verified.** Obtained by invoking selected official bytecode functions under Java 25. These probes cover individual functions.
- **Proposed design.** Project choices such as virtual time, initial state, and observations.
- **Requires trace verification.** Runtime conditions to verify with real client/server event recordings.

Distribution verification used the [official 26.1.2 release post](https://www.minecraft.net/en-us/article/minecraft-java-edition-26-1-2) and its [official server JAR](https://piston-data.mojang.com/v1/objects/97ccd4c0ed3f81bbb7bfacddd1090b0c56f9bc51/server.jar). Version metadata came from the existing Loom cache. DNS/web restrictions prevented downloading that JSON again during the investigation. The local client/server JAR SHA-1 hashes match the recorded distribution hashes, and the server hash also matches the official post's link. The combat specification uses primary sources throughout.
