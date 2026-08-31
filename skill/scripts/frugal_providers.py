#!/usr/bin/env python3
"""Frugal Tokenomics provider framework (PRD sections 16-17, 89).

Registry of optimization providers with capabilities, trust classes, and
installation detection. Stdlib only — no dependencies.

Usage:
    python frugal_providers.py list                 # registry with trust/capabilities
    python frugal_providers.py status               # which providers are installed
    python frugal_providers.py manifest ast-grep    # full manifest for one provider
    python frugal_providers.py status --json        # machine-readable output

Custom providers: drop a JSON manifest into ~/.frugal/providers/<id>.json using
the same shape as the built-in entries (see `manifest` output).
"""
import argparse
import json
import shutil
import sys
from pathlib import Path

FRUGAL_DIR = Path.home() / ".frugal"

# Capabilities marked exclusive may have at most ONE active provider
# (enforced by frugal_conflicts.py).
EXCLUSIVE_CAPABILITIES = {
    "compression.document",
    "compression.output",
    "request_proxy",
    "compaction.manager",
}

BUILTIN_PROVIDERS = [
    {
        "id": "ripgrep",
        "name": "ripgrep",
        "trust": "VERIFIED",
        "capabilities": ["navigation.grep"],
        "conflicts": [],
        "requires": [],
        "priority": 100,
        "detect": {"which": "rg"},
        "install": "built into Claude Code (Grep tool); standalone: winget install BurntSushi.ripgrep.MSVC",
        "repo": "https://github.com/BurntSushi/ripgrep",
        "fixed_context_tax_tokens": 0,
    },
    {
        "id": "ast-grep",
        "name": "ast-grep",
        "trust": "VERIFIED",
        "capabilities": ["navigation.ast"],
        "conflicts": [],
        "requires": [],
        "priority": 90,
        "detect": {"which": "ast-grep"},
        "install": "cargo install ast-grep --locked  (or: npm i -g @ast-grep/cli)",
        "repo": "https://github.com/ast-grep/ast-grep",
        "fixed_context_tax_tokens": 0,
    },
    {
        "id": "tree-sitter",
        "name": "tree-sitter CLI",
        "trust": "VERIFIED",
        "capabilities": ["navigation.parse"],
        "conflicts": [],
        "requires": [],
        "priority": 50,
        "detect": {"which": "tree-sitter"},
        "install": "npm i -g tree-sitter-cli",
        "repo": "https://github.com/tree-sitter/tree-sitter",
        "fixed_context_tax_tokens": 0,
    },
    {
        "id": "graphify",
        "name": "Graphify",
        "trust": "COMMUNITY",
        "capabilities": ["navigation.graph", "memory.repository"],
        "conflicts": [],
        "requires": [],
        "priority": 70,
        "detect": {"skill": "graphify"},
        "install": "git clone https://github.com/DCS-Hub-DCS/Graphify ~/.claude/skills/graphify",
        "repo": "https://github.com/DCS-Hub-DCS/Graphify",
        "fixed_context_tax_tokens": 400,
    },
    {
        "id": "token-compact",
        "name": "token-compact",
        "trust": "COMMUNITY",
        "capabilities": ["compression.document"],
        "conflicts": [],
        "requires": [],
        "priority": 60,
        "detect": {"skill": "token-compact"},
        "install": "git clone https://github.com/theosib/token-compact ~/.claude/skills/token-compact",
        "repo": "https://github.com/theosib/token-compact",
        "fixed_context_tax_tokens": 300,
    },
    {
        "id": "token-saver",
        "name": "token-saver",
        "trust": "COMMUNITY",
        "capabilities": ["measurement.billing_audit", "routing.subagent"],
        "conflicts": [],
        "requires": [],
        "priority": 60,
        "detect": {"skill": "token-saver"},
        "install": "git clone https://github.com/bryanvine/token-saver ~/.claude/skills/token-saver",
        "repo": "https://github.com/bryanvine/token-saver",
        "fixed_context_tax_tokens": 300,
    },
    {
        "id": "typescript-language-server",
        "name": "TypeScript LSP",
        "trust": "VERIFIED",
        "capabilities": ["navigation.lsp"],
        "conflicts": [],
        "requires": [],
        "priority": 80,
        "detect": {"which": "typescript-language-server"},
        "install": "npm i -g typescript-language-server typescript",
        "repo": "https://github.com/typescript-language-server/typescript-language-server",
        "fixed_context_tax_tokens": 0,
    },
    {
        "id": "pyright",
        "name": "Pyright (Python LSP)",
        "trust": "VERIFIED",
        "capabilities": ["navigation.lsp"],
        "conflicts": [],
        "requires": [],
        "priority": 80,
        "detect": {"which": "pyright"},
        "install": "npm i -g pyright  (or: pip install pyright)",
        "repo": "https://github.com/microsoft/pyright",
        "fixed_context_tax_tokens": 0,
    },
    {
        "id": "rust-analyzer",
        "name": "rust-analyzer (Rust LSP)",
        "trust": "VERIFIED",
        "capabilities": ["navigation.lsp"],
        "conflicts": [],
        "requires": [],
        "priority": 80,
        "detect": {"which": "rust-analyzer"},
        "install": "rustup component add rust-analyzer",
        "repo": "https://github.com/rust-lang/rust-analyzer",
        "fixed_context_tax_tokens": 0,
    },
]

REQUIRED_MANIFEST_KEYS = {"id", "name", "trust", "capabilities", "priority"}
VALID_TRUST = {"VERIFIED", "COMMUNITY", "EXPERIMENTAL", "BLOCKED"}


def _skill_installed(skill_id: str) -> bool:
    for base in (Path.home() / ".claude" / "skills", Path.cwd() / ".claude" / "skills"):
        if (base / skill_id).is_dir():
            return True
    return False


def _mcp_configured(server_name: str) -> bool:
    """Detect an MCP server by name in project .mcp.json or ~/.claude.json."""
    for config_path in (Path.cwd() / ".mcp.json", Path.home() / ".claude.json"):
        if not config_path.is_file():
            continue
        try:
            data = json.loads(config_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        servers = data.get("mcpServers", {})
        if server_name in servers:
            return True
        # ~/.claude.json also nests mcpServers per project
        for project in data.get("projects", {}).values():
            if server_name in project.get("mcpServers", {}):
                return True
    return False


def detect_installed(provider: dict) -> bool:
    detect = provider.get("detect", {})
    if "which" in detect:
        return shutil.which(detect["which"]) is not None
    if "skill" in detect:
        return _skill_installed(detect["skill"])
    if "mcp" in detect:
        return _mcp_configured(detect["mcp"])
    if "path" in detect:
        return Path(detect["path"]).expanduser().exists()
    return False


def _load_manifest_dir(custom_dir: Path):
    providers, errors = [], []
    if not custom_dir.is_dir():
        return providers, errors
    for manifest_path in sorted(custom_dir.glob("*.json")):
        try:
            data = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{manifest_path.name}: unreadable ({exc})")
            continue
        missing = REQUIRED_MANIFEST_KEYS - set(data)
        if missing:
            errors.append(f"{manifest_path.name}: missing keys {sorted(missing)}")
            continue
        if data.get("trust") not in VALID_TRUST:
            errors.append(f"{manifest_path.name}: invalid trust {data.get('trust')!r}")
            continue
        data.setdefault("conflicts", [])
        data.setdefault("requires", [])
        data.setdefault("fixed_context_tax_tokens", 0)
        providers.append(data)
    return providers, errors


def load_registry(frugal_dir: Path = FRUGAL_DIR):
    """Built-ins + bundled manifests (repo providers/) + user manifests
    (~/.frugal/providers/). Later layers override earlier ones by id."""
    bundled_dir = Path(__file__).resolve().parent.parent / "providers"
    bundled, errors = _load_manifest_dir(bundled_dir)
    custom, custom_errors = _load_manifest_dir(frugal_dir / "providers")
    errors += custom_errors

    merged = {p["id"]: p for p in BUILTIN_PROVIDERS}
    for layer in (bundled, custom):
        for p in layer:
            merged[p["id"]] = p
    return sorted(merged.values(), key=lambda p: (-p["priority"], p["id"])), errors


def cmd_list(registry, as_json: bool):
    if as_json:
        print(json.dumps(registry, indent=2))
        return
    print(f"{'id':<28} {'trust':<13} {'tax':>6}  capabilities")
    for p in registry:
        caps = ", ".join(p["capabilities"])
        print(f"{p['id']:<28} {p['trust']:<13} {p['fixed_context_tax_tokens']:>6}  {caps}")


def cmd_status(registry, as_json: bool):
    rows = [{**p, "installed": detect_installed(p)} for p in registry]
    if as_json:
        print(json.dumps(rows, indent=2))
        return
    print(f"{'id':<28} {'installed':<10} install hint (if missing)")
    for r in rows:
        hint = "" if r["installed"] else r["install"]
        mark = "yes" if r["installed"] else "no"
        print(f"{r['id']:<28} {mark:<10} {hint}")


def cmd_manifest(registry, provider_id: str, as_json: bool):
    for p in registry:
        if p["id"] == provider_id:
            print(json.dumps(p, indent=2))
            return 0
    print(f"error: unknown provider {provider_id!r}", file=sys.stderr)
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["list", "status", "manifest"])
    parser.add_argument("provider_id", nargs="?", help="provider id (manifest command)")
    parser.add_argument("--json", action="store_true", help="machine-readable output")
    parser.add_argument("--dir", type=Path, default=FRUGAL_DIR,
                        help="frugal data directory (default ~/.frugal)")
    args = parser.parse_args()

    registry, errors = load_registry(args.dir)
    for err in errors:
        print(f"warning: custom manifest skipped — {err}", file=sys.stderr)

    if args.command == "list":
        cmd_list(registry, args.json)
    elif args.command == "status":
        cmd_status(registry, args.json)
    elif args.command == "manifest":
        if not args.provider_id:
            print("error: manifest requires a provider id", file=sys.stderr)
            return 2
        return cmd_manifest(registry, args.provider_id, args.json)
    return 0


if __name__ == "__main__":
    sys.exit(main())
