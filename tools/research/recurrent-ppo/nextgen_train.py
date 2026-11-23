"""Next-generation PPO stage. Campaign orchestration lives in nextgen.py."""
import copy
import time
from collections import defaultdict
from pathlib import Path

import numpy as np
import torch

from league import League
from native import Batch, FIXED_EQUIPMENT_PROFILE
from policy import WIDTH, atomic_json, distribution, log_probability, verify_export
from train import Trainer


class NextTrainer(Trainer):
    def __init__(self, args, models, on_candidate=None):
        super().__init__(args)
        self.device = torch.device(getattr(args, "ppo_device", "cpu"))
        self.learner = self.policy if self.device.type == "cpu" else copy.deepcopy(self.policy).to(self.device)
        self.optimizer = torch.optim.Adam(self.learner.parameters(), lr=args.learning_rate, eps=1e-5,
                                          fused=True if self.device.type == "cuda" else None)
        self.league = League(models)
        self.on_candidate = on_candidate
        self.assignment = np.zeros((args.lanes, 2), dtype=np.int64)
        self.labels = np.zeros((args.rollout, args.lanes, 2), dtype=np.int64)
        self.prefix = [[] for _ in range(args.lanes)]
        self.phase = 1  # The next-generation objective includes the HP bonus from the start.

    def equipment_profile(self):
        return FIXED_EQUIPMENT_PROFILE if getattr(self.args, "fixed_equipment", False) else None

    def initialize(self, batch):
        self.batch = batch
        self.league.register(batch)
        self.memory = torch.zeros(self.args.lanes, WIDTH)
        for lane in range(self.args.lanes):
            self.reset(lane)

    def reset(self, lane):
        rival, net = self.league.choose(self.rng, self.args.focus, self.args.exploit,
                                        self.args.target)
        self.assignment[lane] = rival, net
        self.league.record(rival, net, start=True)
        self.batch.reset(lane, int(self.rng.integers(0, 2**63)), net, rival, self.args.max_steps)
        self.memory[lane].zero_()

    def record_step(self, tick):
        self.labels[tick] = self.assignment
        self.record_coverage(self.assignment, "steps")
        for lane in np.flatnonzero(self.batch.outcomes):
            rival, net = self.assignment[lane]
            self.league.record(int(rival), int(net), outcome=int(self.batch.outcomes[lane]))

    def record_coverage(self, labels, field, weights=None):
        rivals, nets = labels.T
        stride = int(nets.max()) + 1
        counts = np.bincount(rivals * stride + nets, weights=weights)
        for cell in np.flatnonzero(counts):
            rival, net = divmod(int(cell), stride)
            self.league.record(rival, net, **{field: int(counts[cell])})

    def init_checkpoint(self, path):
        state = torch.load(path, map_location="cpu", weights_only=False)
        if state["input"] != "full" or state["feedforward"]:
            raise ValueError("initial checkpoint must be full-input GRU")
        self.policy.load_state_dict(state["policy"])
        if self.learner is not self.policy:
            self.learner.load_state_dict(self.policy.state_dict())

    def checkpoint(self, name="latest.pt"):
        path = self.args.output / name
        tmp = path.with_suffix(".tmp")
        torch.save(dict(version=2, input="full", feedforward=False, seed=self.args.seed,
                        policy=self.policy.state_dict(), optimizer=self.optimizer.state_dict(),
                        rng=self.rng.bit_generator.state, torch_rng=torch.get_rng_state(),
                        update=self.update, elapsed=self.elapsed, total_steps=self.total_steps,
                        counters=self.counters, next_selection=self.next_selection,
                        league=self.league.state(), sequence=self.args.sequence, minibatch_tokens=self.args.minibatch_tokens,
                        burn_in=self.args.burn_in, focus=self.args.focus, exploit=self.args.exploit,
                        target=self.args.target, hp_beta=self.args.hp_beta,
                        learning_rate=self.args.learning_rate, match_profile=self.equipment_profile()), tmp)
        tmp.replace(path)

    def resume(self, path):
        state = torch.load(path, map_location="cpu", weights_only=False)
        if state.get("match_profile") != self.equipment_profile():
            raise ValueError("resume configuration mismatch: match_profile")
        if "minibatch_tokens" in state and state["minibatch_tokens"] != self.args.minibatch_tokens:
            raise ValueError("resume configuration mismatch: minibatch_tokens")
        for key in ("seed", "sequence", "burn_in", "focus", "exploit", "target", "hp_beta", "learning_rate"):
            if state.get(key) != getattr(self.args, key):
                raise ValueError(f"resume configuration mismatch: {key}")
        if state["version"] != 2:
            raise ValueError("use --init-checkpoint for the old trainer's checkpoint")
        self.restore_policy(state)
        if self.learner is not self.policy:
            self.learner.load_state_dict(self.policy.state_dict())
        self.rng.bit_generator.state = state["rng"]
        torch.set_rng_state(state["torch_rng"])
        self.league = League(self.league.models, state["league"])
        for key in ("update", "elapsed", "total_steps", "counters", "next_selection"):
            setattr(self, key, state[key])
        self.log({"event": "resume", "environment": "new episodes; partial rollout and recurrent state discarded"})

    def segments(self, rollout):
        obs, _, hidden, _, _, _, _, finished = rollout
        segments = []
        for lane in range(self.args.lanes):
            prefix = self.prefix[lane]
            prefix_n = len(prefix)
            all_obs = torch.cat([torch.stack([p[0] for p in prefix]), obs[:, lane]]) if prefix else obs[:, lane]
            all_h = torch.cat([torch.stack([p[1] for p in prefix]), hidden[:, lane]]) if prefix else hidden[:, lane]
            episode_start = 0
            start = 0
            while start < self.args.rollout:
                ends = np.flatnonzero(finished[start:, lane])
                end = min(start + self.args.sequence,
                          start + int(ends[0]) + 1 if len(ends) else self.args.rollout,
                          self.args.rollout)
                position = prefix_n + start
                burn_start = max(episode_start, position - self.args.burn_in)
                segments.append((lane, start, end, all_obs[burn_start:position], all_h[burn_start]))
                if finished[end - 1, lane]:
                    episode_start = prefix_n + end
                start = end
            tail_start = max(episode_start, len(all_obs) - self.args.burn_in)
            self.prefix[lane] = [(all_obs[i].clone(), all_h[i].clone()) for i in range(tail_start, len(all_obs))]
        return segments

    def optimize(self, rollout):
        if self.learner is self.policy:
            return self._optimize(rollout)
        previous = torch.backends.cudnn.enabled
        try:
            if torch.version.hip:
                # Use PyTorch's native GPU GRU on ROCm, including its backward pass.
                torch.backends.cudnn.enabled = False
            return self._optimize(rollout)
        finally:
            if torch.version.hip:
                torch.backends.cudnn.enabled = previous
            self.policy.load_state_dict(self.learner.state_dict())

    def _optimize(self, rollout):
        obs, masks, _, choices, logp, returns, advantages, _ = rollout
        segments = self.segments(rollout)
        obs, masks, choices, logp, returns, advantages = (
            x.flatten(0, 1).to(self.device) for x in (obs, masks, choices, logp, returns, advantages)
        )
        prefixes = torch.nn.utils.rnn.pad_sequence([s[3] for s in segments], batch_first=True).to(self.device)
        initial_memory = torch.stack([s[4] for s in segments]).to(self.device)
        totals = np.zeros(7)
        tokens = 0
        epochs = 0
        burn_tokens = 0
        for epoch in range(self.args.epochs):
            order = list(self.rng.permutation(len(segments)))
            batches, batch, count = [], [], 0
            for index in order:
                s = segments[index]
                size = s[2] - s[1]
                if batch and count + size > self.args.minibatch_tokens:
                    batches.append(batch)
                    batch, count = [], 0
                batch.append(index)
                count += size
            if batch:
                batches.append(batch)
            epoch_kl, epoch_tokens = 0., 0
            for ids in batches:
                items = [segments[i] for i in ids]
                lengths = np.array([s[2] - s[1] for s in items])
                lanes = np.array([s[0] for s in items])
                starts = np.array([s[1] for s in items])
                relative = np.arange(lengths.max())[:, None]
                ticks = starts + np.minimum(relative, lengths - 1)
                positions = torch.as_tensor(ticks * self.args.lanes + lanes, device=self.device)
                n = int(lengths.sum())
                valid = slice(None) if n == positions.numel() else torch.as_tensor(
                    np.flatnonzero(relative < lengths), device=self.device)
                samples = positions.flatten()[valid]
                segment_ids = torch.as_tensor(np.asarray(ids), device=self.device)
                memory = initial_memory.index_select(0, segment_ids)
                # Replay equally long prefixes together, starting at each segment's own state.
                with torch.no_grad():
                    groups = defaultdict(list)
                    for i, item in enumerate(items):
                        if len(item[3]):
                            groups[len(item[3])].append(i)
                    for length, active in groups.items():
                        indices = torch.as_tensor(active, device=self.device)
                        source = segment_ids.index_select(0, indices)
                        inputs = prefixes[:, :length].index_select(0, source).transpose(0, 1)
                        _, _, state = self.learner(inputs, memory.index_select(0, indices))
                        memory.index_copy_(0, indices, state)
                        burn_tokens += length * len(active)
                logits, value, _ = self.learner(obs[positions], memory)
                probs = distribution(logits.flatten(0, 1)[valid], masks[samples])
                new = log_probability(probs, choices[samples])
                log_ratio = new - logp[samples]
                ratio = log_ratio.exp()
                adv = advantages[samples]
                policy_loss = -torch.minimum(ratio * adv, ratio.clamp(.8, 1.2) * adv).mean()
                value_loss = (value.flatten()[valid] - returns[samples]).square().mean()
                entropy = -(probs.exp() * probs).sum((-2, -1)).mean()
                loss = policy_loss + .5 * value_loss - .01 * entropy
                if not torch.isfinite(loss):
                    raise RuntimeError("non-finite PPO loss")
                self.optimizer.zero_grad(set_to_none=True)
                loss.backward()
                grad = torch.nn.utils.clip_grad_norm_(self.learner.parameters(), .5, error_if_nonfinite=True)
                self.optimizer.step()
                stats = torch.stack((loss.detach(), value_loss.detach(), entropy.detach(),
                                     (ratio.detach() - 1 - log_ratio.detach()).mean(),
                                     ((ratio.detach() - 1).abs() > .2).float().mean(),
                                     grad.detach(), policy_loss.detach())).cpu().numpy().astype(np.float64)
                kl = stats[3]
                totals += n * stats
                tokens += n
                epoch_kl += n * kl
                epoch_tokens += n
                # A segment stays in one episode, so its first label covers all its valid tokens.
                self.record_coverage(self.labels[starts, lanes], "tokens", lengths)
            epochs += 1
            if epoch_kl / epoch_tokens > self.args.target_kl:
                break
        keys = ("loss", "value_loss", "entropy", "approx_kl", "clip_fraction", "gradient_norm", "policy_loss")
        return dict(zip(keys, totals / tokens), valid_tokens=tokens, burn_tokens=burn_tokens, epochs=epochs)

    def check_coverage(self):
        if self.args.exploit or self.elapsed < 1800:
            return
        missing = []
        for rival in (10, 1, 11, 8):
            for net in (5, 6):
                row = self.league.counts.get(self.league.key(rival, net), {})
                if any(row.get(k, 0) == 0 for k in ("starts", "steps", "tokens")):
                    missing.append([rival, net])
        if missing:
            raise RuntimeError(f"target cells have no training coverage after 30 minutes: {missing}")


def run_stage(args, models, on_candidate=None, deadline=None):
    args.output = Path(args.output)
    args.output.mkdir(parents=True, exist_ok=True)
    latest = args.output / "latest.pt"
    if latest.exists() and not args.resume:
        raise ValueError("stage exists; explicit resume required")
    started = time.monotonic()
    torch.set_num_threads(args.torch_threads)
    torch.manual_seed(args.seed)
    trainer = NextTrainer(args, models, on_candidate)
    if args.resume:
        trainer.resume(args.resume)
    else:
        trainer.init_checkpoint(args.init_checkpoint)
    initial_elapsed = trainer.elapsed
    def elapsed():
        trainer.elapsed = initial_elapsed + time.monotonic() - started
    atomic_json(args.output / "settings.json", {k: str(v) if isinstance(v, Path) else v for k, v in vars(args).items()})
    with Batch(args.lanes, args.threads, "full", fixed_equipment=bool(trainer.equipment_profile())) as batch:
        trainer.initialize(batch)
        try:
            while trainer.elapsed < args.seconds and (deadline is None or time.monotonic() < deadline):
                if args.updates is not None and trainer.update >= args.updates:
                    break
                rollout = trainer.collect()
                stats = trainer.optimize(rollout)
                trainer.update += 1
                elapsed()
                trainer.check_coverage()
                if trainer.update == 1 or trainer.update % 10 == 0:
                    trainer.log({"event": "update", **stats})
                if trainer.elapsed >= trainer.next_selection:
                    trainer.next_selection += args.selection_interval
                    candidate(trainer, on_candidate)
                    elapsed()
                    trainer.checkpoint()
                    atomic_json(args.output / "coverage.json", trainer.league.state())
        finally:
            elapsed()
            trainer.checkpoint()
            trainer.policy.export(args.output / "last.json")
            atomic_json(args.output / "coverage.json", trainer.league.state())
        with Batch(1, 1, "full") as probe:
            verify_export(trainer.policy, args.output / "last.json", probe, np.random.default_rng(12345))
        candidate(trainer, on_candidate)
        elapsed()
        trainer.checkpoint()
    report = dict(elapsed=trainer.elapsed, updates=trainer.update, steps=trainer.total_steps,
                  steps_per_second=trainer.total_steps / max(trainer.elapsed, 1e-6),
                  model=str((args.output / "last.json").resolve()), checkpoint=str(latest.resolve()),
                  rust_torch_parity="passed")
    atomic_json(args.output / "training-summary.json", report)
    trainer.log({"event": "complete", "rust_torch_parity": "passed"})
    return report


def candidate(trainer, callback):
    path = trainer.args.output / f"candidate-{trainer.update:07d}.json"
    trainer.export_training_model(path)
    trainer.checkpoint(path.with_suffix(".pt").name)
    if callback:
        callback(path, trainer)
