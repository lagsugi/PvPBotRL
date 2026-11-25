# PvP Bot model data

Copyright © 2026 lagsugi

The JSON model weights, training history and model provenance files in `models/bundled/` and `models/archive/` are made available under [Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/), to the extent of the rights held by lagsugi. Commercial reuse and adaptation are permitted under those terms. When sharing the data, credit `lagsugi` for the `PvP Bot model weights`, link to the license, and indicate changes where applicable. The terms apply when the model files are embedded in a PvP Bot executable or packaged in a plugin JAR.

| Bundled weights | Training or selection record |
| --- | --- |
| `duel_rl.json` | [Duel-RL](../docs/research/bots/duel-rl.md) |
| `league_rl.json`, `league_g5.json` | [League RL](../docs/research/bots/league-rl.md) |
| `league_g6.json` | [League G6](../docs/research/bots/league-g6.md) |
| `recurrent_ppo.json` | [Recurrent PPO](../docs/research/bots/recurrent-ppo-plan.md) |
| `recurrent_g2.json` | [Run notes](../docs/research/bots/next-gen-gru-run.md) and [checkpoint provenance](bundled/recurrent_g2.source.json) |
| `recurrent_g2_plus.json` | [Run plan](../docs/research/bots/recurrent-g2-plus-plan.md) and [checkpoint provenance](bundled/recurrent_g2_plus.source.json) |
| `g3_prototype.json` | [G3 experiment](../docs/research/bots/g3.md) and [checkpoint provenance](bundled/g3_prototype.source.json) |

The archived files contain historical league weights and training history. [SHA-256 checksums](SHA256SUMS) identify the bytes of the JSON files in this directory. See the [GPLv3 additional permission for model bundling](../LICENSE-EXCEPTIONS.md#model-bundling) for the code that embeds or packages these files.
