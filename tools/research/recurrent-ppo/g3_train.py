"""Cold-start recurrent risk prediction and league training on the local simulator."""
import argparse
import json
import time
from pathlib import Path

import numpy as np
import torch
from torch import nn
from torch.nn import functional as F

from g3_budget import remaining
from league import digest
from native import Batch, ROOT
from nextgen_train import NextTrainer
from policy import Policy, WIDTH, atomic_json, verify_export


BASELINES = tuple(ROOT / "models/bundled" / f"recurrent_{name}.json"
                  for name in ("ppo", "g2", "g2_plus"))
# Reach, Orbit/Pressure and the G4 challenger are reserved for evaluation.
TRAIN_RIVALS = (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 17)
HOLDOUT = (14, 15, 16)
HORIZON = 8


class G3Policy(Policy):
    def __init__(self, variant, feedforward):
        super().__init__(variant, feedforward)
        self.forecast = nn.Linear(WIDTH, 2)
        # Exploration prior for cold starts on widely spaced spawns.
        with torch.no_grad():
            self.actor.bias[2] = 1.2
            self.actor.bias[9] = .6


def forecast_labels(obs, finished):
    """Identify a delivered HP drop over the next eight decisions, within one episode."""
    targets = torch.zeros((*obs.shape[:2], 2))
    valid = torch.zeros(obs.shape[:2], dtype=torch.bool)
    if len(obs) > HORIZON:
        hp = obs[:, :, :2]
        future_min = hp[1:].unfold(0, HORIZON, 1).amin(-1)
        targets[:-HORIZON] = future_min < hp[:-HORIZON] - 1e-5
        valid[:-HORIZON] = ~torch.as_tensor(finished[:-1]).unfold(0, HORIZON, 1).any(-1)
    return targets.flatten(0, 1), valid.flatten()


class G3Trainer(NextTrainer):
    policy_class = G3Policy

    def reset(self, lane):
        net = int(self.rng.integers(13)) if self.rng.random() < .75 else 0
        if self.update < 20:
            rival = int(self.rng.choice((1, 2, 3, 4, 5, 6)))
        else:
            group = self.rng.random()
            if group < .48:
                pool = list(TRAIN_RIVALS)
            elif group < .85 or len(self.league.models) == 3:
                pool = [18, 19, 20]
            else:
                pool = list(range(18 + max(3, len(self.league.models) - 5), 18 + len(self.league.models)))
            scores = np.array([self.league.score(r, net) for r in pool])
            # Emphasize unsolved but learnable matches. Uniform mass retains coverage.
            weight = .15 + scores * (1 - scores)
            rival = int(self.rng.choice(pool, p=weight / weight.sum()))
        self.assignment[lane] = rival, net
        self.league.record(rival, net, start=True)
        self.batch.reset(lane, int(self.rng.integers(2**63)), net, rival, self.args.max_steps)
        self.memory[lane].zero_()

    def optimize(self, rollout):
        labels, mask = forecast_labels(rollout[0], rollout[-1])
        self.targets = labels.to(self.device)
        self.known = mask.to(self.device)
        return super().optimize(rollout)

    def training_outputs(self, observation, memory, samples, valid):
        latent, _ = self.learner.encode_sequence(observation, memory)
        pred = self.learner.forecast(latent.flatten(0, 1)[valid])
        mask = self.known[samples]
        extra = F.binary_cross_entropy_with_logits(pred, self.targets[samples], reduction="none").mean(-1)
        loss = .15 * (extra * mask).sum() / mask.sum().clamp_min(1)
        logits = self.learner.actor(latent)
        # A decaying, observation-only timing prior makes contact and charged swings
        # discoverable when random play almost never wins against any trained rival.
        if self.args.guided and self.update < 400:
            obs = observation.flatten(0, 1)[valid]
            heads = logits.flatten(0, 1)[valid]
            ready = obs[:, 2] > .91
            approach = (obs[:, 44] < -.5) | (ready & (obs[:, 44] < 1.4))
            retreat = ~ready & (obs[:, 44] > .7)
            movement = 1 + approach.long() - retreat.long()
            movement_loss = F.cross_entropy(heads[:, :3], movement)
            in_reach = obs[:, 44] > 0
            swing_loss = F.cross_entropy(heads[:, 8:10], ready.long(), reduction="none")
            guide = movement_loss + (swing_loss * in_reach).sum() / in_reach.sum().clamp_min(1)
            loss = loss + .45 * (1 - self.update / 400) * guide
        return logits, self.learner.critic(latent).squeeze(-1), loss

    def record_step(self, tick):
        super().record_step(tick)
        if self.args.guided:
            self.batch.rewards -= .00006
            self.batch.rewards[self.batch.outcomes == 3] -= .65


def row(path, group):
    checksum = digest(path)
    return dict(id=checksum, sha256=checksum, path=str(path.resolve()), group=group)


def train(args):
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=True)
    args.output = root
    models = [row(p, "anchor") for p in BASELINES]
    settings = dict(seed=args.seed, lanes=args.lanes, rollout=args.rollout, threads=args.threads,
                    torch_threads=args.torch_threads, sequence=args.sequence, burn_in=args.burn_in,
                    minibatch_tokens=args.minibatch_tokens, learning_rate=args.learning_rate, epochs=args.epochs,
                    device=args.ppo_device, fixed_equipment=True, input=args.input, hp_beta=args.hp_beta,
                    max_steps=args.max_steps, target_kl=args.target_kl, selection_interval=args.selection_interval,
                    held_out=HOLDOUT, baselines=models, start="random", torch=torch.__version__)
    if args.guided:
        settings["timing_curriculum"] = "charge_and_spacing_v1"
    manifest = root / "settings.json"
    if args.resume:
        if json.loads(manifest.read_text()) != json.loads(json.dumps(settings)):
            raise ValueError("training settings or baseline weights changed")
    elif any(root.iterdir()):
        raise ValueError("choose an empty experiment directory or resume the existing run")
    else:
        atomic_json(manifest, settings)
    torch.set_num_threads(args.torch_threads)
    torch.manual_seed(args.seed)
    trainer = G3Trainer(args, models)
    if args.resume:
        latest = root / "latest.pt"
        if latest.exists():
            trainer.resume(latest)
        elif any(root.glob("candidate-*.pt")) or (root / "last.json").exists():
            raise ValueError("latest.pt is missing from a previously trained run")
    started, prev_elapsed = time.monotonic(), trainer.elapsed
    with Batch(args.lanes, args.threads, "g3", fixed_equipment=True) as batch:
        trainer.initialize(batch)
        last_save = time.monotonic()
        while (trainer.update < args.updates and remaining(args.until) > 30
               and trainer.elapsed < args.seconds and not (root / "finish-training").exists()):
            stats = trainer.optimize(trainer.collect())
            trainer.update += 1
            trainer.elapsed = prev_elapsed + time.monotonic() - started
            if trainer.update <= 2 or trainer.update % 10 == 0:
                trainer.log(dict(event="update", **stats))
            if trainer.elapsed >= trainer.next_selection:
                path = root / f"candidate-{trainer.update:07}.json"
                trainer.export_training_model(path)
                trainer.next_selection += args.selection_interval
                slot = trainer.league.add(row(path, "self"))
                batch.snapshot(slot, path)
                trainer.checkpoint(path.with_suffix(".pt").name)
            if time.monotonic() - last_save >= 300 or trainer.update == 1:
                trainer.checkpoint()
                last_save = time.monotonic()
        if trainer.update == 0:
            raise RuntimeError("training stopped before the first update")
        trainer.checkpoint()
        trainer.export_training_model(root / "last.json")
    with Batch(1, 1, "g3") as probe:
        verify_export(trainer.policy, root / "last.json", probe, np.random.default_rng(913))
    atomic_json(root / "training-summary.json", dict(updates=trainer.update, steps=trainer.total_steps,
                seconds=trainer.elapsed, steps_per_second=trainer.total_steps / max(trainer.elapsed, .001),
                sha256=digest(root / "last.json"), rust_export=True))


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=Path)
    p.add_argument("--seed", type=int, default=83001)
    p.add_argument("--ppo-device", choices=["cpu", "cuda"], default="cuda")
    p.add_argument("--seconds", type=float, default=20 * 3600)
    p.add_argument("--until", type=float, required=True, help="absolute UTC Unix stop time")
    p.add_argument("--updates", type=int, default=10**8)
    p.add_argument("--lanes", type=int, default=384)
    p.add_argument("--threads", type=int, default=8)
    p.add_argument("--torch-threads", type=int, default=2)
    p.add_argument("--rollout", type=int, default=128)
    p.add_argument("--minibatch-tokens", type=int, default=4096)
    p.add_argument("--selection-interval", type=float, default=1800)
    p.add_argument("--resume", action="store_true")
    p.add_argument("--guided", action="store_true", help="anneal a spacing and charged-swing prior over 400 updates")
    p.set_defaults(input="g3", feedforward=False, fixed_equipment=True, sequence=32,
                   learning_rate=3e-4, burn_in=8, hp_beta=.65, max_steps=6000,
                   focus=0., exploit=False, target=None, epochs=3, target_kl=.018)
    return p


if __name__ == "__main__":
    train(parser().parse_args())
