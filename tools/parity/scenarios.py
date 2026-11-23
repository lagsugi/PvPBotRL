"""Deterministic physical-key scripts; no model inference or adaptive tracking."""
import random


def action(**keys):
    # The training action always holds the sprint key.
    keys.setdefault("sprint", True)
    return {"keys": keys}


def movement_suite(full=False):
    cases = {
        "idle": [action()] * 60,
        "forward": [action(forward=True, sprint=True)] * 60 + [action()] * 20,
        "backward": [action(backward=True, sprint=True)] * 60 + [action()] * 20,
        "left": [action(left=True)] * 60 + [action()] * 20,
        "right": [action(right=True)] * 60 + [action()] * 20,
        "diagonal": [action(forward=True, left=True, sprint=True)] * 60 + [action()] * 20,
        "opposing": [action(forward=True, backward=True, left=True, right=True, sprint=True)] * 40,
        "sneak_diagonal": [action(forward=True, left=True, sneak=True, sprint=True)] * 40 + [action()] * 20,
        "sneak_transition": [action(forward=True, sprint=True)] * 10 +
            [action(forward=True, sneak=True, sprint=True)] * 10 + [action(forward=True, sprint=True)] * 20,
        "jump": [action(jump=True)] + [action()] * 39,
        "held_jump": [action(forward=True, jump=True, sprint=True)] * 60,
        "sprint_reset": [action(forward=True, sprint=True)] * 20 + [action()] +
            [action(forward=True, sprint=True)] * 20,
    }
    if full:
        for seed in range(100):
            rng = random.Random(seed)
            cases[f"random_{seed:03}"] = [action(**dict(zip(
                ("forward", "backward", "left", "right", "jump", "sneak"),
                (bool(rng.getrandbits(1)) for _ in range(6))))) for _ in range(600)]
        cases["long_idle"] = [action()] * 6000
    return cases


def scenario(name, actions, fps=60):
    return {"schema": 1, "id": name, "floor_y": 65.0, "floor_friction": 0.6,
            "fps": fps, "warmup": 40, "actions": actions}


def mixed_fight(seed, ticks):
    """Two independently seeded key sequences with a charged opening attack."""
    sides = []
    for side in range(2):
        rng = random.Random(seed * 2 + side)
        opening = [{**action(), "click": side == 0 and i == 10} for i in range(20)]
        actions = []
        for _ in range(ticks - len(opening)):
            keys = {key: bool(rng.getrandbits(1)) for key in ("forward", "backward", "left", "right", "jump", "sneak")}
            actions.append({**action(**keys), "click": rng.randrange(12) == 0})
        sides.append(opening + actions)
    return {"distance": 2.5, "seed": seed, "capture_ticks": ticks + 40, "opponent_actions": sides[1]}, sides[0]


def clicks(*ticks, keys=None, pitch=None):
    return [{"keys": (keys or {}).get(i, {}), "click": i in ticks, **({"pitch": pitch} if pitch is not None else {})}
            for i in range(40)]


def combat_suite():
    return {
        "charged": ({"distance": 2.5}, clicks(10)),
        "critical": ({"distance": 2.0, "pitch": 40.0}, clicks(15, keys={5: {"jump": True}}, pitch=40.0)),
        "sprint": ({"distance": 2.5}, clicks(10, keys={i: {"forward": True, "sprint": True} for i in range(6, 11)})),
        "reach_inside": ({"distance": 3.29}, clicks(10)),
        "reach_outside": ({"distance": 3.31}, clicks(10)),
        "rapid": ({"distance": 1.5}, clicks(10, 11, 13, 16, 20, 22, 23, 24)),
        "break": ({"distance": 2.5, "one_durability": True}, clicks(10)),
        "opponent_attack": ({"distance": 2.5, "opponent_click": True}, clicks()),
        "simultaneous": ({"distance": 2.5, "opponent_click": True}, clicks(10)),
        "fall": ({"distance": 2.5, "fall_height": 8.0}, clicks()),
        "overlap": ({"distance": 0.0}, clicks(10)),
    }


def knockback_suite():
    victim_jump = clicks(keys={10: {"jump": True}})
    sprint = {i: {"forward": True, "sprint": True} for i in range(6, 14)}
    approach = clicks(keys={i: {"forward": True} for i in (9, 10, 11)})
    retreat = clicks(keys={i: {"backward": True} for i in range(7, 14)})
    sidestep = {i: {"left": True} for i in range(7, 11)} | {i: {"right": True} for i in range(12, 20)}
    sprint_jump = {i: {"forward": True, "sprint": True, "jump": i == 10} for i in range(8, 12)}
    held_sprint_jump = {i: {"forward": True, "sprint": True, "jump": i == 10} for i in range(8, 18)}
    return {
        "victim_jump_control": ({"distance": 2.5, "opponent_actions": victim_jump}, clicks()),
        "victim_takeoff": ({"distance": 2.5, "opponent_actions": victim_jump}, clicks(10)),
        "victim_rising": ({"distance": 2.5, "opponent_actions": victim_jump}, clicks(13)),
        "victim_falling": ({"distance": 2.5, "opponent_actions": victim_jump}, clicks(18)),
        "victim_rising_sprint_hit": ({"distance": 2.5, "opponent_actions": victim_jump}, clicks(13, keys=sprint)),
        "victim_ground_moving": ({"distance": 2.0, "opponent_actions": clicks(
            keys={i: {"left": True} for i in (10, 11, 12)})}, clicks(12)),
        "reach_approach_retry": ({"distance": 3.45, "opponent_actions": approach}, clicks(8, 14, 17)),
        "reach_retreat_followup": ({"distance": 3.10, "opponent_actions": retreat}, clicks(7, 11, 15)),
        "bot_reach_approach": ({"distance": 3.45, "opponent_actions": clicks(8, 14, 17)}, approach),
        "bot_reach_retreat": ({"distance": 3.10, "opponent_actions": clicks(7, 11, 15)}, retreat),
        "bot_reach_strafe": ({"distance": 2.50, "opponent_actions": clicks(10, 16, 20)}, clicks(keys=sidestep)),
        "bot_self_after_hit": ({"distance": 2.50, "opponent_actions": clicks(18, keys=sprint_jump)}, clicks(13)),
        "bot_self_after_hit_held_sprint": ({"distance": 3.00, "opponent_actions": clicks(18, keys=held_sprint_jump)}, clicks(13)),
    }
