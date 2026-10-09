#!/usr/bin/env python3
"""Validate .cargo/audit.toml ignores (0119-MERIDIAN S4).

Fails (exit 1) when an ignore:
  - has no `# until YYYY-MM-DD` date, or the date has passed;
  - matches no package in Cargo.lock (stale);
  - is compiled in (non-empty `cargo tree -i <crate>@<version> ... -e normal`) without `risk-accepted`;
  - is labelled `lock-only` but the crate is compiled in, or is labelled neither;
  - carries no reason text.

Findings come from `cargo audit --json` run from an empty directory, so no audit.toml applies and
every advisory against Cargo.lock is visible, ignored or not.

Usage: validate-audit-ignores.py [--audit-toml PATH] [--lock PATH] [--today YYYY-MM-DD] [--no-fetch]
Needs: cargo, cargo-audit, python >= 3.11.
"""
import argparse
import datetime
import json
import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LINE = re.compile(r'^\s*"(RUSTSEC-\d{4}-\d{4})"\s*,?\s*(#.*)?$')
UNTIL = re.compile(r"#\s*until\s+(\d{4}-\d{2}-\d{2})\b")


def audit_findings(lock: Path, no_fetch: bool) -> dict:
    """advisory id -> list of (crate, version), from an audit run that sees no config."""
    cmd = ["cargo", "audit", "--json", "--file", str(lock)]
    if no_fetch:
        cmd.append("--no-fetch")
    with tempfile.TemporaryDirectory() as empty:
        proc = subprocess.run(cmd, cwd=empty, capture_output=True, text=True)
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError:
        sys.exit(f"cargo audit produced no JSON (rc={proc.returncode}): {proc.stderr.strip()[:400]}")
    found: dict = {}
    entries = list(data["vulnerabilities"]["list"])
    for group in data["warnings"].values():
        entries.extend(group)
    for e in entries:
        adv = e.get("advisory")
        if adv:
            found.setdefault(adv["id"], []).append((e["package"]["name"], e["package"]["version"]))
    return found


def compiled_in(crate: str, version: str) -> bool:
    proc = subprocess.run(
        ["cargo", "tree", "-i", f"{crate}@{version}", "--workspace", "--all-features",
         "--target", "all", "-e", "normal", "--locked"],
        cwd=ROOT, capture_output=True, text=True)
    if proc.returncode != 0:
        # A failed cargo tree must never read as an empty tree (that would class a compiled-in crate as lock-only).
        sys.exit(f"cargo tree failed for {crate}@{version} (rc={proc.returncode}): {proc.stderr.strip()[:400]}")
    return bool(proc.stdout.strip())


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--audit-toml", default=str(ROOT / ".cargo" / "audit.toml"))
    ap.add_argument("--lock", default=str(ROOT / "Cargo.lock"))
    ap.add_argument("--today", default=datetime.date.today().isoformat())
    ap.add_argument("--no-fetch", action="store_true")
    args = ap.parse_args()
    today = datetime.date.fromisoformat(args.today)

    text = Path(args.audit_toml).read_text()
    ids = tomllib.loads(text).get("advisories", {}).get("ignore", [])
    comments = {}
    for line in text.splitlines():
        m = LINE.match(line)
        if m:
            comments[m.group(1)] = m.group(2) or ""

    found = audit_findings(Path(args.lock), args.no_fetch)
    errors = []
    for adv in ids:
        comment = comments.get(adv)
        if comment is None:
            errors.append(f"{adv}: ignore is not on its own `\"ID\", # ...` line, so it cannot be checked")
            continue
        m = UNTIL.search(comment)
        if not m:
            errors.append(f"{adv}: undated (needs `# until YYYY-MM-DD`)")
        else:
            try:
                until = datetime.date.fromisoformat(m.group(1))
            except ValueError:
                errors.append(f"{adv}: bad date {m.group(1)!r}")
                until = None
            if until and until < today:
                errors.append(f"{adv}: expired on {until} (today {today})")
        reason = UNTIL.sub("", comment).replace("lock-only", "").replace("risk-accepted", "")
        reason = reason.strip(" #;|")
        if len(reason) < 20:
            errors.append(f"{adv}: no reason given")
        pkgs = found.get(adv)
        if not pkgs:
            errors.append(f"{adv}: matches no locked package (stale ignore)")
            continue
        for crate, version in pkgs:
            if compiled_in(crate, version):
                if "risk-accepted" not in comment:
                    errors.append(f"{adv}: {crate} {version} is compiled in; needs `risk-accepted` with a reason")
            else:
                if "lock-only" not in comment:
                    errors.append(f"{adv}: {crate} {version} is not compiled in (empty cargo tree); needs `lock-only`")
        if "lock-only" in comment and "risk-accepted" in comment:
            errors.append(f"{adv}: labelled both lock-only and risk-accepted")
        if any(compiled_in(c, v) for c, v in pkgs) and "lock-only" in comment:
            errors.append(f"{adv}: labelled lock-only but the crate is compiled in")

    if errors:
        print("audit ignore validation FAILED:")
        for e in errors:
            print(f"  - {e}")
        return 1
    print(f"audit ignore validation passed: {len(ids)} ignores, all dated, matched to a locked package, classified.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
