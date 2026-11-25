import json

import numpy as np
import torch
from torch import nn

from native import HEADS, INPUTS

WIDTH = 64


def atomic_json(path, value):
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(value, ensure_ascii=False, allow_nan=False), encoding="utf-8")
    tmp.replace(path)


class Policy(nn.Module):
    def __init__(self, variant, feedforward):
        super().__init__()
        self.variant = variant
        self.feedforward = feedforward
        size = INPUTS[variant]
        self.register_buffer("mean", torch.zeros(size))
        self.register_buffer("scale", torch.ones(size))
        self.encoder = nn.Linear(size, WIDTH)
        self.core = nn.Linear(WIDTH, WIDTH) if feedforward else nn.GRU(WIDTH, WIDTH)
        self.actor = nn.Linear(WIDTH, sum(HEADS))
        self.critic = nn.Linear(WIDTH, 1)
        nn.init.orthogonal_(self.actor.weight, gain=0.01)
        nn.init.zeros_(self.actor.bias)
        nn.init.orthogonal_(self.critic.weight)
        nn.init.zeros_(self.critic.bias)

    def encode_sequence(self, observation, state):
        normalized = ((observation - self.mean) * self.scale).clamp(-8, 8)
        encoded = self.encoder(normalized).tanh()
        if self.feedforward:
            output = self.core(encoded).tanh()
            state = output[-1]
        else:
            output, state = self.core(encoded, state.unsqueeze(0))
            state = state.squeeze(0)
        return output, state

    def forward(self, observation, state):
        output, state = self.encode_sequence(observation, state)
        return self.actor(output), self.critic(output).squeeze(-1), state

    def export(self, path):
        def linear(layer):
            return tensors(layer.weight, layer.bias)

        def tensors(weight, bias):
            return {"weight": weight.detach().tolist(), "bias": bias.detach().tolist()}

        if self.feedforward:
            input_gate, hidden_gate = linear(self.core), None
        else:
            input_gate = tensors(self.core.weight_ih_l0, self.core.bias_ih_l0)
            hidden_gate = tensors(self.core.weight_hh_l0, self.core.bias_hh_l0)
        atomic_json(path, {
            "version": 1, "input": self.variant, "encoder": linear(self.encoder),
            "input_gate": input_gate, "hidden_gate": hidden_gate, "actor": linear(self.actor),
            "mean": self.mean.tolist(), "scale": self.scale.tolist(),
        })


def distribution(logits, masks):
    empty = torch.full_like(logits[..., :1], -1e9)
    jump = logits[..., 7:8].masked_fill(~masks[..., :1].bool(), -1e9)
    click = logits[..., 9:10].masked_fill(~masks[..., 1:2].bool(), -1e9)
    heads = torch.stack((
        logits[..., :3], logits[..., 3:6],
        torch.cat((logits[..., 6:7], jump, empty), -1),
        torch.cat((logits[..., 8:9], click, empty), -1),
    ), -2)
    return heads.log_softmax(-1)


def log_probability(log_probs, choices):
    return log_probs.gather(-1, choices.long().unsqueeze(-1)).squeeze(-1).sum(-1)


@torch.no_grad()
def verify_export(policy, path, batch, rng):
    batch.snapshot(0, path)
    memory = np.zeros(WIDTH, dtype=np.float32)
    expected_memory = torch.zeros(1, WIDTH)
    for _ in range(64):
        obs = rng.normal(size=INPUTS[policy.variant]).astype(np.float32)
        logits, _, expected_memory = policy(torch.from_numpy(obs)[None, None], expected_memory)
        actual, memory = batch.probe(0, obs, memory)
        np.testing.assert_allclose(actual, logits[0, 0].numpy(), atol=1e-5, rtol=1e-4)
        np.testing.assert_allclose(memory, expected_memory[0].numpy(), atol=1e-5, rtol=1e-4)
