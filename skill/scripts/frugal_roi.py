#!/usr/bin/env python3
"""Frugal Tokens ROI measurement engine (PRD sections 3, 43-44, 96).

Records per-task economics into a local JSONL ledger and computes:

  - CST (Cost per Successful Task) for baseline vs optimized phases
  - Optimization Efficiency = baseline CST / optimized CST
  - per-provider ROI verdicts using the acceptance rule:
        cost improvement >= 10%  AND  retry increase <= 5%
        AND  success-rate loss <= 3%

Ledger: ~/.frugal/roi/ledger.jsonl (one JSON object per line, append-only).

Usage:
    # record a baseline task (before enabling optimizers)
    python frugal_roi.py record --phase baseline --task "fix auth race" \
        --input-tokens 82000 --output-tokens 9000 --cache-read 22000 \
        --cost 1.84 --retries 1 --success

    # record an optimized task (note which providers were active)
    python frugal_roi.py record --phase optimized --task "add rbac" \
        --providers ast-grep,graphify \
        --input-tokens 35000 --output-tokens 6000 --cache-read 27000 \
        --cost 0.97 --retries 0 --success

    python frugal_roi.py report
    python frugal_roi.py report --json
"""
import argparse
import datetime
import json
import sys
from pathlib import Path

FRUGAL_DIR = Path.home() / ".frugal"

ACCEPT_MIN_COST_IMPROVEMENT = 0.10   # >= 10% cheaper
ACCEPT_MAX_RETRY_INCREASE = 0.05     # <= 5 pp more retries per task
ACCEPT_MAX_QUALITY_LOSS = 0.03       # <= 3 pp success-rate drop


def ledger_path(frugal_dir: Path) -> Path:
    return frugal_dir / "roi" / "ledger.jsonl"


def cmd_record(args) -> int:
    event = {
        "ts": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "phase": args.phase,
        "task": args.task,
        "providers": sorted(p for p in (args.providers or "").split(",") if p),
        "input_tokens": args.input_tokens,
        "output_tokens": args.output_tokens,
        "cache_read_tokens": args.cache_read,
        "cost_usd": args.cost,
        "retries": args.retries,
        "success": args.success,
        "duration_min": args.duration_min,
    }
    path = ledger_path(args.dir)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(event) + "\n")
    print(f"recorded {args.phase} task -> {path}")
    return 0


def load_events(frugal_dir: Path):
    path = ledger_path(frugal_dir)
    if not path.is_file():
        return []
    events = []
    for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            print(f"warning: skipping malformed ledger line {lineno}", file=sys.stderr)
    return events


def summarize(events: list) -> dict:
    if not events:
        return {"tasks": 0}
    successes = [e for e in events if e["success"]]
    total_cost = sum(e["cost_usd"] for e in events)
    return {
        "tasks": len(events),
        "successful": len(successes),
        "success_rate": len(successes) / len(events),
        "total_cost_usd": round(total_cost, 4),
        "cst_usd": round(total_cost / len(successes), 4) if successes else None,
        "avg_input_tokens": round(sum(e["input_tokens"] for e in events) / len(events)),
        "avg_output_tokens": round(sum(e["output_tokens"] for e in events) / len(events)),
        "avg_cache_read_tokens": round(sum(e["cache_read_tokens"] for e in events) / len(events)),
        "avg_retries": round(sum(e["retries"] for e in events) / len(events), 2),
    }


def verdict(baseline: dict, optimized: dict) -> dict:
    """Apply the PRD section 96 acceptance rule."""
    if not baseline.get("cst_usd") or not optimized.get("cst_usd"):
        return {"verdict": "INSUFFICIENT DATA",
                "reason": "need >=1 successful task in both phases"}
    cost_improvement = 1 - optimized["cst_usd"] / baseline["cst_usd"]
    retry_increase = optimized["avg_retries"] - baseline["avg_retries"]
    quality_loss = baseline["success_rate"] - optimized["success_rate"]
    efficiency = baseline["cst_usd"] / optimized["cst_usd"]

    checks = {
        "cost_improvement": cost_improvement >= ACCEPT_MIN_COST_IMPROVEMENT,
        "retry_increase_ok": retry_increase <= ACCEPT_MAX_RETRY_INCREASE,
        "quality_loss_ok": quality_loss <= ACCEPT_MAX_QUALITY_LOSS,
    }
    if all(checks.values()):
        label = "EXCELLENT" if cost_improvement >= 0.30 else "ACCEPTED"
    elif not checks["quality_loss_ok"] or not checks["retry_increase_ok"]:
        label = "REJECTED (quality/retry regression)"
    elif cost_improvement > 0:
        label = "NEUTRAL (savings below 10% threshold)"
    else:
        label = "REJECTED (costs more)"

    return {
        "verdict": label,
        "cost_improvement_pct": round(cost_improvement * 100, 1),
        "optimization_efficiency": round(efficiency, 2),
        "retry_increase": round(retry_increase, 2),
        "quality_loss_pct": round(quality_loss * 100, 1),
        "checks": checks,
    }


def per_provider(events: list) -> dict:
    optimized = [e for e in events if e["phase"] == "optimized"]
    providers = sorted({p for e in optimized for p in e["providers"]})
    return {p: summarize([e for e in optimized if p in e["providers"]])
            for p in providers}


def cmd_report(args) -> int:
    events = load_events(args.dir)
    if not events:
        print(f"no events in {ledger_path(args.dir)} — record baseline tasks first",
              file=sys.stderr)
        return 1

    baseline = summarize([e for e in events if e["phase"] == "baseline"])
    optimized = summarize([e for e in events if e["phase"] == "optimized"])
    result = {
        "baseline": baseline,
        "optimized": optimized,
        "assessment": verdict(baseline, optimized),
        "per_provider": per_provider(events),
    }

    if args.json:
        print(json.dumps(result, indent=2))
        return 0

    print("FRUGAL TOKENS — ROI REPORT")
    print("=" * 42)
    for label, s in (("BASELINE", baseline), ("OPTIMIZED", optimized)):
        print(f"\n{label}  ({s.get('tasks', 0)} tasks)")
        if not s.get("tasks"):
            continue
        print(f"  success rate        {s['success_rate']:.0%}")
        print(f"  cost/successful task ${s['cst_usd']}" if s["cst_usd"] is not None
              else "  cost/successful task n/a (no successes)")
        print(f"  avg input tokens    {s['avg_input_tokens']:,}")
        print(f"  avg cache reads     {s['avg_cache_read_tokens']:,}")
        print(f"  avg retries         {s['avg_retries']}")

    a = result["assessment"]
    print("\nASSESSMENT")
    print(f"  verdict             {a['verdict']}")
    if "cost_improvement_pct" in a:
        print(f"  cost improvement    {a['cost_improvement_pct']}%")
        print(f"  efficiency          {a['optimization_efficiency']}x")
        print(f"  retry increase      {a['retry_increase']}")
        print(f"  quality loss        {a['quality_loss_pct']}%")

    if result["per_provider"]:
        print("\nPER PROVIDER (optimized tasks where active)")
        for pid, s in result["per_provider"].items():
            cst = f"${s['cst_usd']}" if s.get("cst_usd") is not None else "n/a"
            print(f"  {pid:<20} tasks={s['tasks']:<3} CST={cst:<8} "
                  f"retries={s['avg_retries']}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dir", type=Path, default=FRUGAL_DIR,
                        help="frugal data directory (default ~/.frugal)")
    sub = parser.add_subparsers(dest="command", required=True)

    rec = sub.add_parser("record", help="append a task event to the ledger")
    rec.add_argument("--phase", choices=["baseline", "optimized"], required=True)
    rec.add_argument("--task", required=True)
    rec.add_argument("--providers", default="",
                     help="comma-separated provider ids active for this task")
    rec.add_argument("--input-tokens", type=int, required=True)
    rec.add_argument("--output-tokens", type=int, required=True)
    rec.add_argument("--cache-read", type=int, default=0)
    rec.add_argument("--cost", type=float, required=True, help="effective cost USD")
    rec.add_argument("--retries", type=int, default=0)
    rec.add_argument("--duration-min", type=float, default=None)
    grp = rec.add_mutually_exclusive_group(required=True)
    grp.add_argument("--success", dest="success", action="store_true")
    grp.add_argument("--failed", dest="success", action="store_false")
    rec.set_defaults(func=cmd_record)

    rep = sub.add_parser("report", help="baseline vs optimized economics")
    rep.add_argument("--json", action="store_true")
    rep.set_defaults(func=cmd_report)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
