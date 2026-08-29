#!/usr/bin/env python3
"""Frugal Tokens adaptive provider lifecycle manager (PRD sections 46, 55-60).

Profiles the current repository, intelligently recommends which third-party
optimizers to install (Graphify, token-compact, token-saver, ...), and manages
their full lifecycle: install, per-session disable/enable, and uninstall.
Recommendations adapt over time: measured ROI data (frugal_roi.py ledger)
overrides heuristics — a provider that proved itself stays recommended; one
that measured REJECTED gets flagged for disable/uninstall.

Nothing is modified without an explicit command. `recommend` is read-only.

Usage:
    python frugal_setup.py recommend                 # profile repo + advise
    python frugal_setup.py recommend --repo path/to/repo
    python frugal_setup.py install graphify          # git clone into skills dir
    python frugal_setup.py disable token-compact     # park it (skipped next session)
    python frugal_setup.py enable token-compact      # bring it back
    python frugal_setup.py uninstall token-compact   # move to backups (recoverable)
    python frugal_setup.py uninstall token-compact --purge   # delete permanently
    python frugal_setup.py state                     # lifecycle status table
"""
import argparse
import datetime
import json
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from frugal_providers import FRUGAL_DIR, load_registry  # noqa: E402

SKILLS_DIR = Path.home() / ".claude" / "skills"
CODE_SUFFIXES = {".py", ".ts", ".tsx", ".js", ".jsx", ".go", ".rs", ".java",
                 ".kt", ".rb", ".php", ".cs", ".cpp", ".c", ".h", ".swift",
                 ".dart", ".vue", ".svelte", ".sql"}
DOC_SUFFIXES = {".md", ".mdx", ".rst", ".txt", ".adoc"}
SKIP_DIRS = {".git", "node_modules", "dist", "build", "target", ".next",
             "__pycache__", ".venv", "venv", "vendor", ".frugal"}
MONOREPO_MARKERS = ("packages", "apps", "libs", "services")

def is_installable(provider: dict) -> bool:
    """Any provider that ships as a Claude skill with a repo URL participates
    in the install/disable/uninstall lifecycle — including new third-party
    tools registered later via a JSON manifest. No code change needed."""
    return "skill" in provider.get("detect", {}) and bool(provider.get("repo"))


# ---------------------------------------------------------------- repo profile

def profile_repo(repo: Path) -> dict:
    code_tokens = doc_tokens = code_files = doc_files = 0
    manifests = 0
    for p in repo.rglob("*"):
        if any(part in SKIP_DIRS for part in p.parts):
            continue
        if not p.is_file():
            continue
        suffix = p.suffix.lower()
        try:
            tokens = int(p.stat().st_size / 4)
        except OSError:
            continue
        if suffix in CODE_SUFFIXES:
            code_tokens += tokens
            code_files += 1
        elif suffix in DOC_SUFFIXES:
            doc_tokens += tokens
            doc_files += 1
        if p.name in ("package.json", "pyproject.toml", "Cargo.toml", "go.mod",
                      "pom.xml", "build.gradle"):
            manifests += 1
    monorepo = manifests > 2 or any((repo / m).is_dir() for m in MONOREPO_MARKERS)
    return {
        "repo": str(repo),
        "code_files": code_files,
        "code_tokens": code_tokens,
        "doc_files": doc_files,
        "doc_tokens": doc_tokens,
        "monorepo": monorepo,
    }


# ------------------------------------------------------------- roi integration

def roi_verdicts(frugal_dir: Path) -> dict:
    """Per-provider measured outcome from the ROI ledger, if any."""
    ledger = frugal_dir / "roi" / "ledger.jsonl"
    if not ledger.is_file():
        return {}
    events = []
    for line in ledger.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    baseline = [e for e in events if e["phase"] == "baseline" and e["success"]]
    if not baseline:
        return {}
    base_cst = sum(e["cost_usd"] for e in baseline) / len(baseline)
    verdicts = {}
    providers = {p for e in events for p in e.get("providers", [])}
    for pid in providers:
        mine = [e for e in events if e["phase"] == "optimized"
                and pid in e.get("providers", [])]
        wins = [e for e in mine if e["success"]]
        if len(mine) < 3:
            verdicts[pid] = {"verdict": "MEASURING", "tasks": len(mine)}
            continue
        if not wins:
            verdicts[pid] = {"verdict": "REJECTED", "tasks": len(mine),
                             "reason": "no successful tasks"}
            continue
        cst = sum(e["cost_usd"] for e in wins) / len(wins)
        improvement = 1 - cst / base_cst
        quality_loss = 1 - len(wins) / len(mine)
        if quality_loss > 0.03 or improvement <= 0:
            verdicts[pid] = {"verdict": "REJECTED", "tasks": len(mine),
                             "improvement_pct": round(improvement * 100, 1)}
        elif improvement >= 0.10:
            verdicts[pid] = {"verdict": "PROVEN", "tasks": len(mine),
                             "improvement_pct": round(improvement * 100, 1)}
        else:
            verdicts[pid] = {"verdict": "NEUTRAL", "tasks": len(mine),
                             "improvement_pct": round(improvement * 100, 1)}
    return verdicts


# ------------------------------------------------------------- recommendations

def _rule_graph_navigation(profile, installed):
    if profile["monorepo"] or profile["code_tokens"] > 400_000:
        return ("RECOMMENDED",
                f"large codebase (~{profile['code_tokens']:,} code tokens"
                f"{', monorepo' if profile['monorepo'] else ''}) — graph "
                "navigation beats repeated raw exploration")
    if profile["code_tokens"] > 100_000:
        return ("OPTIONAL", "medium codebase — worthwhile if revisited "
                            "across many sessions")
    return ("NOT_RECOMMENDED", "small repo — index maintenance overhead "
                               "exceeds exploration savings")


def _rule_doc_compression(profile, installed):
    if profile["doc_tokens"] > 100_000:
        return ("RECOMMENDED",
                f"heavy documentation (~{profile['doc_tokens']:,} doc tokens) "
                "— document compression pays off, verify retention")
    if profile["doc_tokens"] > 30_000:
        return ("OPTIONAL", "moderate docs — useful only when large specs are "
                            "loaded as context")
    return ("NOT_RECOMMENDED", "few docs — nothing to compress; its fixed "
                               "context tax would be pure cost")


def _rule_measurement(profile, installed):
    return ("RECOMMENDED" if not installed else "OPTIONAL",
            "billing-audit visibility — establishes real (cache-aware) cost "
            "truth for validating every other optimizer")


def _rule_default_deny(profile, installed):
    return ("NOT_RECOMMENDED",
            "PRD default: prompt-cache economics usually beat proxy/output "
            "compression, and community billing audits show claimed savings "
            "often don't reduce real cost — adopt only with a measured A/B "
            "via frugal_roi.py")


# Recommendation intelligence is keyed by CAPABILITY, not by provider id.
# A new third-party tool registered via JSON manifest automatically inherits
# the rule matching its declared capabilities — no routing code changes.
CAPABILITY_RULES = [
    ("navigation.graph", _rule_graph_navigation),
    ("compression.document", _rule_doc_compression),
    ("measurement.billing_audit", _rule_measurement),
    ("compression.output", _rule_default_deny),
    ("request_proxy", _rule_default_deny),
]


def heuristic(provider: dict, profile: dict, installed: bool) -> tuple:
    """(level, reason) from workload signals. Levels:
    RECOMMENDED / OPTIONAL / NOT_RECOMMENDED.
    A manifest may pin `"default_recommendation": "..."` to skip heuristics."""
    pinned = provider.get("default_recommendation")
    if pinned:
        return (pinned, provider.get("recommendation_reason",
                                     "pinned by provider manifest"))
    caps = set(provider.get("capabilities", []))
    for capability, rule in CAPABILITY_RULES:
        if capability in caps:
            return rule(profile, installed)
    return ("OPTIONAL", "no workload heuristic matches its capabilities — "
                        "measure with frugal_roi.py before trusting")


def recommend(profile: dict, registry: list, frugal_dir: Path,
              skills_dir: Path) -> list:
    measured = roi_verdicts(frugal_dir)
    disabled_dir = frugal_dir / "disabled-skills"
    rows = []
    for p in registry:
        if not is_installable(p):
            continue
        pid = p["id"]
        skill = p.get("detect", {}).get("skill", pid)
        installed = (skills_dir / skill).is_dir()
        disabled = (disabled_dir / skill).is_dir()
        level, reason = heuristic(p, profile, installed)
        roi = measured.get(pid)
        # Measured data overrides heuristics — this is the adaptive part.
        if roi:
            if roi["verdict"] == "PROVEN":
                level = "KEEP (PROVEN)"
                reason = (f"measured {roi['improvement_pct']}% CST improvement "
                          f"over {roi['tasks']} tasks")
            elif roi["verdict"] == "REJECTED":
                level = "DISABLE/UNINSTALL"
                reason = (f"measured on {roi['tasks']} tasks: did not earn its "
                          "place (cost/quality regression)")
            elif roi["verdict"] == "NEUTRAL":
                level = "OPTIONAL"
                reason = (f"measured only {roi['improvement_pct']}% improvement "
                          "— below the 10% acceptance threshold")
        rows.append({
            "id": pid, "trust": p["trust"],
            "installed": installed, "disabled": disabled,
            "recommendation": level, "reason": reason,
            "install_cmd": f"python {Path(__file__).name} install {pid}",
        })
    # Proxy/MCP request compressors: listed by category, default-deny per PRD.
    rows.append({
        "id": "(proxy/MCP request compressors)", "trust": "EXPERIMENTAL",
        "installed": False, "disabled": False,
        "recommendation": "NOT_RECOMMENDED",
        "reason": "PRD default: prompt-cache economics usually beat proxy "
                  "compression, and community billing audits show claimed "
                  "savings often don't reduce real cost — only adopt one with "
                  "a measured A/B via frugal_roi.py, never stacked with "
                  "token-compact (exclusive capability)",
    })
    return rows


# ------------------------------------------------------------------ lifecycle

def _timestamp() -> str:
    return datetime.datetime.now().strftime("%Y%m%d-%H%M%S")


def _find_provider(registry: list, pid: str) -> dict:
    for p in registry:
        if p["id"] == pid:
            return p
    print(f"error: unknown provider {pid!r}", file=sys.stderr)
    sys.exit(2)


def _skill_name(provider: dict) -> str:
    return provider.get("detect", {}).get("skill", provider["id"])


def cmd_install(provider: dict, skills_dir: Path, frugal_dir: Path) -> int:
    if not is_installable(provider):
        print(f"error: {provider['id']} is not an installable skill provider "
              "(CLI tools install via their own package manager — see "
              "frugal_providers.py status)", file=sys.stderr)
        return 2
    if provider["trust"] == "BLOCKED":
        print("error: provider trust class is BLOCKED", file=sys.stderr)
        return 2
    target = skills_dir / _skill_name(provider)
    if target.exists():
        print(f"{provider['id']} already installed at {target}")
        return 0
    disabled = frugal_dir / "disabled-skills" / _skill_name(provider)
    if disabled.exists():
        print(f"{provider['id']} is installed but disabled — re-enabling instead")
        return cmd_enable(provider, skills_dir, frugal_dir)
    skills_dir.mkdir(parents=True, exist_ok=True)
    print(f"cloning {provider['repo']} -> {target}")
    result = subprocess.run(["git", "clone", "--depth", "1",
                             provider["repo"], str(target)])
    if result.returncode != 0:
        print("error: git clone failed — nothing was modified", file=sys.stderr)
        return 1
    print(f"installed {provider['id']} ({provider['trust']}). Restart the "
          "session to load it, then record optimized tasks with frugal_roi.py "
          "so its ROI can be measured.")
    return 0


def _move(src: Path, dst: Path) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(src), str(dst))


def cmd_disable(provider: dict, skills_dir: Path, frugal_dir: Path) -> int:
    src = skills_dir / _skill_name(provider)
    dst = frugal_dir / "disabled-skills" / _skill_name(provider)
    if not src.is_dir():
        print(f"{provider['id']} is not installed (or already disabled)")
        return 1
    if dst.exists():
        dst = dst.with_name(f"{dst.name}-{_timestamp()}")
    _move(src, dst)
    print(f"disabled {provider['id']} — parked at {dst}. It will not load in "
          "new sessions. Re-enable with: enable " + provider["id"])
    return 0


def cmd_enable(provider: dict, skills_dir: Path, frugal_dir: Path) -> int:
    src = frugal_dir / "disabled-skills" / _skill_name(provider)
    dst = skills_dir / _skill_name(provider)
    if not src.is_dir():
        print(f"{provider['id']} has no disabled copy at {src}", file=sys.stderr)
        return 1
    if dst.exists():
        print(f"error: {dst} already exists — resolve manually", file=sys.stderr)
        return 1
    _move(src, dst)
    print(f"enabled {provider['id']} — active from the next session")
    return 0


def cmd_uninstall(provider: dict, skills_dir: Path, frugal_dir: Path,
                  purge: bool) -> int:
    name = _skill_name(provider)
    src = skills_dir / name
    if not src.is_dir():
        src = frugal_dir / "disabled-skills" / name
    if not src.is_dir():
        print(f"{provider['id']} is not installed")
        return 1
    if purge:
        shutil.rmtree(src)
        print(f"uninstalled {provider['id']} (purged permanently)")
    else:
        dst = frugal_dir / "backups" / f"uninstalled-{name}-{_timestamp()}"
        _move(src, dst)
        print(f"uninstalled {provider['id']} — recoverable copy at {dst}")
    return 0


def cmd_state(registry: list, skills_dir: Path, frugal_dir: Path) -> int:
    disabled_dir = frugal_dir / "disabled-skills"
    print(f"{'provider':<16} {'state':<12} location")
    for p in registry:
        if not is_installable(p):
            continue
        name = _skill_name(p)
        if (skills_dir / name).is_dir():
            print(f"{p['id']:<16} {'ENABLED':<12} {skills_dir / name}")
        elif (disabled_dir / name).is_dir():
            print(f"{p['id']:<16} {'DISABLED':<12} {disabled_dir / name}")
        else:
            print(f"{p['id']:<16} {'NOT INSTALLED':<12} -")
    return 0


# ------------------------------------------------------------------------ cli

def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command",
                        choices=["recommend", "install", "disable", "enable",
                                 "uninstall", "state"])
    parser.add_argument("provider_id", nargs="?")
    parser.add_argument("--repo", type=Path, default=Path.cwd(),
                        help="repository to profile (default: cwd)")
    parser.add_argument("--dir", type=Path, default=FRUGAL_DIR,
                        help="frugal data directory (default ~/.frugal)")
    parser.add_argument("--skills-dir", type=Path, default=SKILLS_DIR,
                        help="Claude skills directory")
    parser.add_argument("--purge", action="store_true",
                        help="uninstall: delete instead of backing up")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    registry, errors = load_registry(args.dir)
    for err in errors:
        print(f"warning: {err}", file=sys.stderr)

    if args.command == "recommend":
        profile = profile_repo(args.repo)
        rows = recommend(profile, registry, args.dir, args.skills_dir)
        if args.json:
            print(json.dumps({"profile": profile, "recommendations": rows},
                             indent=2))
            return 0
        print(f"REPO PROFILE  {profile['repo']}")
        print(f"  code: {profile['code_files']} files, "
              f"~{profile['code_tokens']:,} tokens"
              f"{'  (monorepo)' if profile['monorepo'] else ''}")
        print(f"  docs: {profile['doc_files']} files, "
              f"~{profile['doc_tokens']:,} tokens")
        print("\nRECOMMENDATIONS")
        for r in rows:
            state = ("enabled" if r["installed"]
                     else "disabled" if r["disabled"] else "not installed")
            print(f"\n  {r['id']}  [{r['trust']}, {state}]")
            print(f"    {r['recommendation']} — {r['reason']}")
        print("\nNothing was modified. Install/disable/uninstall are explicit "
              "commands.")
        return 0

    if args.command == "state":
        return cmd_state(registry, args.skills_dir, args.dir)

    if not args.provider_id:
        print(f"error: {args.command} requires a provider id", file=sys.stderr)
        return 2
    provider = _find_provider(registry, args.provider_id)

    if args.command == "install":
        return cmd_install(provider, args.skills_dir, args.dir)
    if args.command == "disable":
        return cmd_disable(provider, args.skills_dir, args.dir)
    if args.command == "enable":
        return cmd_enable(provider, args.skills_dir, args.dir)
    if args.command == "uninstall":
        return cmd_uninstall(provider, args.skills_dir, args.dir, args.purge)
    return 2


if __name__ == "__main__":
    sys.exit(main())
