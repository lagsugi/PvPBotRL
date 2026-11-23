"""Synchronous, batched binding to the simulator's dedicated training DLL."""
import ctypes as C
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[3]
BUILD = ROOT / "target" / "release"
INPUTS = {"full": 44, "legacy": 48, "no_opponent_hp": 42, "no_action": 40}
STRIDE = 48
HEADS = (3, 3, 2, 2)
FIXED_EQUIPMENT_PROFILE = "sword_pvp_fixed_equipment_v1"


class Batch:
    def __init__(self, lanes, threads, variant, fixed_equipment=False):
        self.lib = C.CDLL(str(BUILD / "pvp_rl.dll"))
        pointer = C.c_void_p
        size = C.c_size_t
        create = "create_fixed_equipment" if fixed_equipment else "create"
        signatures = {
            "error": (C.c_char_p, []),
            create: (pointer, [size, size, C.c_uint8]),
            "free": (None, [pointer]),
            "snapshot": (C.c_int, [pointer, size, C.c_char_p]),
            "reset": (C.c_int, [pointer, size, C.c_uint64, size, size, C.c_uint64, pointer, pointer]),
            "step": (C.c_int, [pointer, pointer, C.c_float, pointer, pointer, pointer, pointer, pointer]),
            "probe": (C.c_int, [pointer, size, pointer, pointer, pointer]),
        }
        for name, (result, args) in signatures.items():
            method = getattr(self.lib, f"rl_{name}")
            method.restype = result
            method.argtypes = args
        self.handle = getattr(self.lib, f"rl_{create}")(lanes, threads, list(INPUTS).index(variant))
        if not self.handle:
            self.check(-1)
        self.features = np.zeros((lanes, STRIDE), dtype=np.float32)
        self.observations = self.features[:, :INPUTS[variant]]
        self.masks = np.zeros((lanes, 2), dtype=np.uint8)
        self.rewards = np.zeros(lanes, dtype=np.float32)
        self.outcomes = np.zeros(lanes, dtype=np.uint8)
        self.hp = np.zeros(lanes, dtype=np.float32)

    def check(self, status):
        if status:
            raise RuntimeError(self.lib.rl_error().decode("utf-8", errors="replace"))

    def snapshot(self, slot, path):
        self.check(self.lib.rl_snapshot(self.handle, slot, str(path.resolve()).encode("utf-8")))

    def reset(self, lane, seed, network, opponent, max_steps):
        self.check(self.lib.rl_reset(
            self.handle, lane, seed, network, opponent, max_steps,
            self.features[lane].ctypes.data, self.masks[lane].ctypes.data,
        ))

    def step(self, choices, beta):
        choices = np.ascontiguousarray(choices, dtype=np.uint8)
        if choices.shape != (len(self.rewards), 4):
            raise ValueError("invalid action matrix")
        self.check(self.lib.rl_step(
            self.handle, choices.ctypes.data, beta, self.features.ctypes.data,
            self.masks.ctypes.data, self.rewards.ctypes.data, self.outcomes.ctypes.data,
            self.hp.ctypes.data,
        ))

    def probe(self, slot, features, memory):
        values = np.zeros(STRIDE, dtype=np.float32)
        values[:len(features)] = features
        memory = np.array(memory, dtype=np.float32, copy=True, order="C")
        if memory.shape != (64,):
            raise ValueError("invalid GRU memory shape")
        logits = np.zeros(sum(HEADS), dtype=np.float32)
        self.check(self.lib.rl_probe(self.handle, slot, values.ctypes.data, memory.ctypes.data, logits.ctypes.data))
        return logits, memory

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.lib.rl_free(self.handle)
        self.handle = None
