#!/usr/bin/env python3
"""Frugal Tokenomics provider conflict solver (PRD section 45).

Given a proposed set of active providers, validates the capability rules and
resolves conflicts deterministically:

  1. BLOCKED providers are never activated.
  2. Explicit `conflicts` between two active providers → higher priority wins.
  3. Exclusive capabilities (one active implementation max, e.g.
     compression.document, request_proxy) → higher priority wins.
  4. `requires` capabilities must be satisfied by the final active set.

Exit codes: 0 = resolved cleanly, 1 = plan has drops/unsatisfied requirements,
2 = usage error.

Usage:
    python frugal_conflicts.py --enable ast-grep graphify token-compact
    python frugal_conflicts.py --enable a b --json
"""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from frugal_providers import EXCLUSIVE_CAPABILITIES, FRUGAL_DIR, load_registry  # noqa: E402


def resolve(providers: list) -> dict:
    """Return {'active': [...], 'dropped': [{'id', 'reason'}], 'unsatisfied': [...]}"""
    active = {}
    dropped = []

    for p in sorted(providers, key=lambda x: (-x["priority"], x["id"])):
        if p["trust"] == "BLOCKED":
            dropped.append({"id": p["id"], "reason": "trust class BLOCKED"})
            continue
        reason = None
        for other in active.values():
            if p["id"] in other.get("conflicts", []) or other["id"] in p.get("conflicts", []):
                reason = f"explicit conflict with {other['id']} (priority {other['priority']} >= {p['priority']})"
                break
            shared = (set(p["capabilities"]) & set(other["capabilities"])
                      & EXCLUSIVE_CAPABILITIES)
            if shared:
                reason = (f"exclusive capability {sorted(shared)[0]!r} already "
                          f"provided by {other['id']}")
                break
        if reason:
            dropped.append({"id": p["id"], "reason": reason})
        else:
            active[p["id"]] = p

    provided = {cap for p in active.values() for cap in p["capabilities"]}
    unsatisfied = []
    for p in active.values():
        for req in p.get("requires", []):
            if req not in provided:
                unsatisfied.append({"id": p["id"], "requires": req})

    return {
        "active": [p["id"] for p in active.values()],
        "dropped": dropped,
        "unsatisfied": unsatisfied,
        "total_fixed_context_tax_tokens": sum(
            p.get("fixed_context_tax_tokens", 0) for p in active.values()),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--enable", nargs="+", required=True,
                        help="provider ids to activate")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--dir", type=Path, default=FRUGAL_DIR)
    args = parser.parse_args()

    registry, errors = load_registry(args.dir)
    for err in errors:
        print(f"warning: {err}", file=sys.stderr)
    by_id = {p["id"]: p for p in registry}

    unknown = [pid for pid in args.enable if pid not in by_id]
    if unknown:
        print(f"error: unknown provider(s): {', '.join(unknown)}", file=sys.stderr)
        return 2

    result = resolve([by_id[pid] for pid in args.enable])

    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print("ACTIVE:")
        for pid in result["active"]:
            print(f"  + {pid}")
        if result["dropped"]:
            print("DROPPED:")
            for d in result["dropped"]:
                print(f"  - {d['id']}: {d['reason']}")
        if result["unsatisfied"]:
            print("UNSATISFIED REQUIREMENTS:")
            for u in result["unsatisfied"]:
                print(f"  ! {u['id']} requires capability {u['requires']!r}")
        print(f"fixed context tax of active set: "
              f"~{result['total_fixed_context_tax_tokens']} tokens/session")

    return 1 if (result["dropped"] or result["unsatisfied"]) else 0


if __name__ == "__main__":
    sys.exit(main())
