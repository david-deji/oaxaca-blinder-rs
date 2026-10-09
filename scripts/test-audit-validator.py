#!/usr/bin/env python3
"""Tests for scripts/validate-audit-ignores.py (0119-MERIDIAN S4, review F3 and F4).

One mutated copy of .cargo/audit.toml or Cargo.lock per failure class, run through the validator with
`--no-fetch` (the advisory database cargo-audit already fetched). Each must exit 1 and name its cause;
the unmutated pair must exit 0, so a validator that fails everything cannot pass this test.
Needs cargo and cargo-audit, like the validator. Scratch files go under target/, never /tmp.

    python3 scripts/test-audit-validator.py
"""
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VALIDATOR = ROOT / "scripts" / "validate-audit-ignores.py"
TOML = ROOT / ".cargo" / "audit.toml"
LOCK = ROOT / "Cargo.lock"
FAILS = []


def check(cond, msg):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        FAILS.append(msg)


def line_of(text, advisory):
    for line in text.splitlines():
        if f'"{advisory}"' in line:
            return line
    raise AssertionError(f"{advisory} is not in audit.toml")


def edit_line(advisory, fn):
    def mutate(text):
        old = line_of(text, advisory)
        new = fn(old)
        assert new != old, f"mutation did not change the {advisory} line"
        return text.replace(old, new, 1)
    return mutate


def add_line(new_line):
    def mutate(text):
        anchor = "ignore = [\n"
        assert anchor in text
        return text.replace(anchor, anchor + new_line + "\n", 1)
    return mutate


H2 = "RUSTSEC-2026-0258"      # lock-only
FASTFLOAT = "RUSTSEC-2025-0003"  # compiled in, risk-accepted

# (label, toml mutation or None, lock mutation or None, extra args, expected finding)
CASES = [
    ("undated ignore", edit_line(H2, lambda l: re.sub(r"until \d{4}-\d{2}-\d{2}", "", l)), None, [], f"{H2}: undated"),
    ("unparseable date", edit_line(H2, lambda l: re.sub(r"until \d{4}-\d{2}-\d{2}", "until 2027-13-45", l)), None, [], "bad date"),
    ("expired ignore (--today)", None, None, ["--today", "2030-01-01"], "expired on"),
    ("no reason text", edit_line(H2, lambda l: l.split("#")[0] + "# until 2027-01-08 ; lock-only"), None, [], f"{H2}: no reason given"),
    ("stale ignore (matches no locked package)", add_line('    "RUSTSEC-2099-0001", # until 2027-01-08 ; lock-only ; a fabricated advisory that matches nothing in the lock'), None, [], "matches no locked package"),
    ("bare undated ID", add_line('    "RUSTSEC-2099-0002",'), None, [], "RUSTSEC-2099-0002: undated"),
    ("two IDs on one line", lambda t: t.replace(line_of(t, H2), '    "' + H2 + '", "RUSTSEC-2025-0020", # until 2027-01-08 ; lock-only ; two ignores share a line', 1), None, [], "not on its own"),
    ("lock-only label on a compiled-in crate", edit_line(FASTFLOAT, lambda l: l.replace("risk-accepted", "lock-only")), None, [], "labelled lock-only but the crate is compiled in"),
    ("compiled-in crate with no risk-accepted label", edit_line(FASTFLOAT, lambda l: l.replace("risk-accepted ;", "")), None, [], "needs `risk-accepted`"),
    ("lock-only crate labelled risk-accepted", edit_line(H2, lambda l: l.replace("lock-only", "risk-accepted")), None, [], "needs `lock-only`"),
    ("both labels", edit_line(H2, lambda l: l.replace("lock-only", "lock-only risk-accepted")), None, [], "labelled both"),
    ("xxhash-rust locked below the fixed version (GHSA only)", None,
     lambda t: re.sub(r'(name = "xxhash-rust"\nversion = ")[^"]+', r"\g<1>0.8.15", t, count=1), [], "xxhash-rust 0.8.15 is locked, minimum is 0.8.16"),
]


def run(toml_text, lock_text, extra, scratch):
    d = Path(tempfile.mkdtemp(prefix="v-", dir=scratch))
    t, l = d / "audit.toml", d / "Cargo.lock"
    t.write_text(toml_text)
    l.write_text(lock_text)
    p = subprocess.run([sys.executable, str(VALIDATOR), "--audit-toml", str(t), "--lock", str(l), "--no-fetch", *extra],
                       capture_output=True, text=True, cwd=ROOT)
    return p.returncode, p.stdout + p.stderr


def main():
    base = ROOT / "target" / "0119-tests"
    base.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="audit-validator-", dir=base))
    toml_text, lock_text = TOML.read_text(), LOCK.read_text()
    try:
        rc, out = run(toml_text, lock_text, [], scratch)
        check(rc == 0, f"control, the real audit.toml and Cargo.lock: exit {rc}" + ("" if rc == 0 else f" {out.strip()[-300:]!r}"))
        for label, tm, lm, extra, want in CASES:
            try:
                rc, out = run(tm(toml_text) if tm else toml_text, lm(lock_text) if lm else lock_text, extra, scratch)
            except AssertionError as e:
                check(False, f"{label}: {e}")
                continue
            ok = rc == 1 and want in out
            check(ok, f"{label}: exit {rc}, expected {want!r}" + ("" if ok else f"; output: {out.strip()[-300:]!r}"))
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    print("\n" + ("FAILED: " + "; ".join(FAILS) if FAILS else f"all checks passed (1 control, {len(CASES)} failure classes)"))
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
