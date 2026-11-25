"""Account for awake time only while the G3 campaign process is running."""
import ctypes
import json
import subprocess
import sys
import threading
import time
from pathlib import Path


def awake():
    ticks = ctypes.c_ulonglong()
    if not ctypes.windll.kernel32.QueryUnbiasedInterruptTime(ctypes.byref(ticks)):
        raise ctypes.WinError()
    return ticks.value / 10_000_000


def remaining(limit):
    return limit() if callable(limit) else limit - time.time()


def run(root, hours, quick, build=True):
    root = Path(root).resolve()
    training = root / "training"
    ledger = root / "active-budget.json"
    handoff = root / "migrate-active-budget"
    if ledger.exists():
        state = json.loads(ledger.read_text())
    else:
        initial = 0.
        phase = "training"
        if handoff.exists():
            initial = float(handoff.read_text(encoding="utf-8-sig"))
        if (training / "training.jsonl").exists():
            with (training / "training.jsonl").open() as log:
                elapsed = max((json.loads(line)["seconds"] for line in log if line.endswith("\n")), default=0.)
            summary = training / "training-summary.json"
            if summary.exists():
                elapsed = max(elapsed, json.loads(summary.read_text())["seconds"])
                if not handoff.exists():
                    phase = "evaluation"
            initial += elapsed
        total = 900 if quick else hours * 3600
        reserve = 720 if quick else min(3600, total / 6)
        state = dict(seconds=initial, total=total, training_limit=total-reserve, phase=phase)
    if state["phase"] == "complete":
        return
    start, previous = awake(), state["seconds"]
    def used():
        return previous + awake() - start
    def save():
        state["seconds"] = used()
        tmp = ledger.with_suffix(".tmp")
        tmp.write_text(json.dumps(state), encoding="utf-8")
        tmp.replace(ledger)
    save()
    if handoff.exists():
        (training / "finish-training").unlink(missing_ok=True)
        handoff.unlink()
    stop = threading.Event()
    def heartbeat():
        while not stop.wait(5):
            save()
    thread = threading.Thread(target=heartbeat, daemon=True)
    thread.start()
    try:
        print(f"G3 active budget: {previous / 3600:.3f} h used of {state['total'] / 3600:.2f} h", flush=True)
        if used() >= state["total"]:
            raise RuntimeError("G3 active budget exhausted")
        if build:
            repo = Path(__file__).resolve().parents[3]
            subprocess.run([str(repo / "tools/rust/cargo.bat"), "build", "--release", "--locked", "-p", "pvp-rl"],
                           cwd=repo, check=True, timeout=state["total"]-used())
        if state["phase"] == "training":
            from g3_train import parser, train
            opts = [str(training), "--guided", "--seed", "83002", "--until", "0"]
            if (training / "settings.json").exists():
                opts.append("--resume")
            if quick:
                opts += ["--updates", "2", "--lanes", "32"]
            args = parser().parse_args(opts)
            args.until = lambda: state["training_limit"] - used()
            args.seconds = float("inf")
            train(args)
            state["phase"] = "evaluation"
        from g3_evaluate import main
        opts = [str(training), str(root / "evaluation"), "--until", "0"]
        if quick:
            opts += ["--smoke", "--pairs", "1", "--dev-pairs", "1"]
        main(opts, lambda: state["total"] - used())
        state["phase"] = "complete"
    finally:
        stop.set()
        thread.join()
        save()


if __name__ == "__main__":
    run(sys.argv[1], int(sys.argv[2]), sys.argv[3] == "True")
