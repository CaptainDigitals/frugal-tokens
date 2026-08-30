#!/usr/bin/env python3
"""Estimate token cost of files/directories before loading them into context.

Deterministic work offloading (Complexity 0): answer "how expensive would it be
to read this?" without spending model tokens.

Usage:
    python estimate_tokens.py <path> [<path> ...]
    python estimate_tokens.py src/ --top 10

Token estimate uses the ~4 chars/token heuristic (good to within ~15% for code).
"""
import argparse
import sys
from pathlib import Path

CHARS_PER_TOKEN = 4.0
SKIP_DIRS = {".git", "node_modules", "dist", "build", "target", ".next",
             "__pycache__", ".venv", "venv", "vendor", ".frugal"}
SKIP_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".pdf",
                 ".zip", ".gz", ".woff", ".woff2", ".ttf", ".eot", ".mp4",
                 ".lock", ".map", ".min.js", ".min.css"}


def estimate_file(path: Path) -> int:
    try:
        size = path.stat().st_size
    except OSError:
        return 0
    return int(size / CHARS_PER_TOKEN)


def iter_files(path: Path):
    if path.is_file():
        yield path
        return
    for p in sorted(path.rglob("*")):
        if not p.is_file():
            continue
        if any(part in SKIP_DIRS for part in p.parts):
            continue
        if p.suffix.lower() in SKIP_SUFFIXES or p.name.endswith(".min.js"):
            continue
        yield p


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="+", help="files or directories")
    parser.add_argument("--top", type=int, default=20,
                        help="show N most expensive files (default 20)")
    args = parser.parse_args()

    results = []
    for raw in args.paths:
        path = Path(raw)
        if not path.exists():
            print(f"warning: {raw} not found", file=sys.stderr)
            continue
        for f in iter_files(path):
            results.append((estimate_file(f), f))

    if not results:
        print("no readable files found", file=sys.stderr)
        return 1

    results.sort(reverse=True)
    total = sum(t for t, _ in results)

    print(f"{'est. tokens':>12}  file")
    for tokens, f in results[: args.top]:
        print(f"{tokens:>12,}  {f}")
    if len(results) > args.top:
        rest = sum(t for t, _ in results[args.top:])
        print(f"{rest:>12,}  ... {len(results) - args.top} more files")
    print("-" * 40)
    print(f"{total:>12,}  TOTAL across {len(results)} files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
