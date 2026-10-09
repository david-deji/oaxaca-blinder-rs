#!/usr/bin/env python3
"""ground_probes.py: the work behind scripts/ground.sh (0119-MERIDIAN S6).

Contract: internal-ops-bureau/knowledge/dev-loop.md (telos-machina) section "ground.sh contract". Pure
probes, no LLM, no judgment. The shape follows the app's scripts/lib/ground-probes.mjs: one JSON
object with ran_at, commit, suites, gate_receipts, verify_live, register_drift, open_issues,
flake_recurrence, app_specific, errors[].

Rules this file keeps (each one is a way the app's GROUND went quiet before):
  * A probe that could not run writes `null` plus a reason; it never omits its key. The reason lands
    in app_specific.probe_reasons and, at the end, in errors[], so "not measured" is never read as
    "measured and fine".
  * Test counts are summed from every `test result:` line AND cross-checked against
    `cargo test -- --list`; a count that disagrees with the listing is an error, not a number.
  * Only a COMPLETED run of main counts as a CI receipt; a job that was skipped or cancelled is
    reported as dark (it proves nothing), never as green.
  * The validator at the end appends to errors[] when total tests is 0 or below the previous probes
    file, when a gating job of the latest completed main run has no completed conclusion, or when any
    probe could not run.
Always exits 0: this script reports, it does not decide.
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
GROUND = Path(os.environ.get("GROUND_DIR") or ROOT / "ground")  # GROUND_DIR: tests write elsewhere
CI_YML = ROOT / ".github" / "workflows" / "ci.yml"

CARGO_TIMEOUT = int(os.environ.get("GROUND_CARGO_TIMEOUT", "3000"))
WASM_VERIFY_TIMEOUT = int(os.environ.get("GROUND_WASM_VERIFY_TIMEOUT", "2400"))
NET_TIMEOUT = int(os.environ.get("GROUND_NET_TIMEOUT", "90"))

ENV = dict(os.environ)
ENV.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")  # disk is tight: no debuginfo in native builds

result: dict = {
    "ran_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "commit": None,
    "suites": {},
    "gate_receipts": {},
    "verify_live": {},
    "register_drift": {},
    "open_issues": None,
    "flake_recurrence": None,
    "app_specific": {"probe_reasons": {}},
    "errors": [],
}
REASONS: dict = result["app_specific"]["probe_reasons"]


class Unavailable(Exception):
    """A probe could not run; the message is the reason that is written next to its null."""


def say(msg: str) -> None:
    print(f"ground: {msg}", file=sys.stderr, flush=True)


def run(cmd: list, timeout: int, cwd: Path = ROOT) -> tuple:
    """Run a command; returns (rc, stdout, stderr). Missing binary or timeout raises Unavailable."""
    exe = cmd[0]
    if shutil.which(exe, path=ENV.get("PATH")) is None:
        raise Unavailable(f"`{exe}` not found on PATH")
    try:
        p = subprocess.run(cmd, cwd=str(cwd), env=ENV, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise Unavailable(f"`{' '.join(cmd[:3])}` timed out after {timeout}s")
    except OSError as e:
        raise Unavailable(f"`{exe}` could not start: {e}")
    return p.returncode, p.stdout, p.stderr


def probe(name: str, fn, fallback):
    """Run fn(); on Unavailable or any exception write `fallback` and record the reason."""
    try:
        return fn()
    except Unavailable as e:
        REASONS[name] = str(e)
        return fallback(str(e)) if callable(fallback) else fallback
    except Exception as e:  # a bug in a probe must not take the others down
        REASONS[name] = f"probe raised {type(e).__name__}: {e}"
        return fallback(REASONS[name]) if callable(fallback) else fallback


def age_hours(iso: str | None):
    if not iso:
        return None
    try:
        then = datetime.fromisoformat(iso.replace("Z", "+00:00"))
    except ValueError:
        return None
    return round((datetime.now(timezone.utc) - then).total_seconds() / 3600, 2)


# --------------------------------------------------------------------------------------------------
# previous report: read BEFORE this run overwrites today's file, so a same-day rerun still has a baseline.
# --------------------------------------------------------------------------------------------------
def previous_reports() -> list:
    out = []
    for f in sorted(GROUND.glob("*-probes*.json")):
        try:
            out.append((f.name, json.loads(f.read_text())))
        except (OSError, ValueError):
            continue
    return out


PREVIOUS = previous_reports()


def total_tests(report: dict):
    suites = (report or {}).get("suites") or {}
    total, seen = 0, False
    for s in suites.values():
        if isinstance(s, dict) and isinstance(s.get("passed"), int):
            seen = True
            total += s["passed"] + (s.get("failed") or 0) + (s.get("skipped") or 0)
    return total if seen else None


def last_total():
    """Total tests of the newest previous report that measured any. A report in which the suites could
    not run (no cargo) has no total, and must not erase the baseline for the next run."""
    for _, rep in reversed(PREVIOUS):
        t = total_tests(rep)
        if t is not None:
            return t
    return None


# --------------------------------------------------------------------------------------------------
# repo identity
# --------------------------------------------------------------------------------------------------
def repo_slug() -> str:
    if os.environ.get("GROUND_REPO"):
        return os.environ["GROUND_REPO"]
    rc, out, _ = run(["git", "remote", "get-url", "origin"], 20)
    m = re.search(r"github\.com[:/]([^/]+/[^/.]+?)(?:\.git)?$", out.strip())
    if rc != 0 or not m:
        raise Unavailable("cannot derive the GitHub repo from `git remote get-url origin`")
    return m.group(1)


def gh_json(args: list):
    """gh call that returns parsed JSON; every failure becomes Unavailable with gh's own words."""
    rc, out, err = run(["gh"] + args, NET_TIMEOUT)
    if rc != 0:
        raise Unavailable(f"`gh {' '.join(args[:2])}` failed (exit {rc}): {(err or out).strip()[:200]}")
    try:
        return json.loads(out) if out.strip() else None
    except ValueError as e:
        raise Unavailable(f"`gh {' '.join(args[:2])}` returned non-JSON: {e}")


# --------------------------------------------------------------------------------------------------
# commit
# --------------------------------------------------------------------------------------------------
def p_commit():
    rc, out, err = run(["git", "rev-parse", "--short", "HEAD"], 20)
    if rc != 0:
        raise Unavailable(f"git rev-parse failed: {err.strip()[:120]}")
    return out.strip()


result["commit"] = probe("commit", p_commit, None)

# --------------------------------------------------------------------------------------------------
# suites.cargo: sum every `test result:` line, cross-check against `cargo test -- --list`
# --------------------------------------------------------------------------------------------------
RESULT_RE = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", re.M)
LIST_RE = re.compile(r"^(\d+) tests?, (\d+) benchmarks?$", re.M)
CARGO_ARGS = ["test", "--workspace", "--all-features", "--locked"]


def suite_null(reason: str) -> dict:
    return {"passed": None, "failed": None, "skipped": None, "files": None, "reason": reason}


def p_suite_cargo():
    started = time.time()
    say("suites.cargo: cargo test --workspace --all-features --locked (this is the slow one)")
    rc, out, err = run(["cargo"] + CARGO_ARGS, CARGO_TIMEOUT)
    text = out + "\n" + err
    rows = RESULT_RE.findall(text)
    if not rows:
        tail = " ".join(text.strip().splitlines()[-3:])[:240]
        raise Unavailable(f"cargo test produced no `test result:` line (exit {rc}): {tail}")
    passed = sum(int(r[1]) for r in rows)
    failed = sum(int(r[2]) for r in rows)
    ignored = sum(int(r[3]) for r in rows)
    measured = sum(int(r[4]) for r in rows)
    failed_tests = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED$", text, re.M)))

    lrc, lout, lerr = run(["cargo"] + CARGO_ARGS + ["--", "--list"], CARGO_TIMEOUT)
    blocks = LIST_RE.findall(lout)
    listed = sum(int(b[0]) for b in blocks) if blocks else None
    suite = {
        "passed": passed,
        "failed": failed,
        "skipped": ignored,
        "files": len(rows),
        "measured": measured,
        "listed_total": listed,
        "listed_binaries": len(blocks) if blocks else None,
        "exit_code": rc,
        "elapsed_seconds": round(time.time() - started),
        "failed_tests": failed_tests,
    }
    ran = passed + failed + ignored + measured
    if listed is None:
        result["errors"].append(f"suites.cargo: `cargo test -- --list` printed no test counts (exit {lrc}), so the total {ran} is not cross-checked")
    elif listed != ran:
        result["errors"].append(f"suites.cargo: summed `test result:` lines give {ran} tests but `cargo test -- --list` names {listed}")
    elif len(blocks) != len(rows):
        result["errors"].append(f"suites.cargo: {len(rows)} `test result:` lines but {len(blocks)} test binaries in --list")
    prev = last_total()
    suite["coverage_delta"] = {"previous_total": prev, "total": ran, "shrank": (prev is not None and ran < prev)}
    return suite


result["suites"]["cargo"] = probe("suites.cargo", p_suite_cargo, suite_null)


# --------------------------------------------------------------------------------------------------
# suites.node_unit: the browser-parity comparator and baseline-stamp unit tests (run by CI before Playwright)
# --------------------------------------------------------------------------------------------------
def p_suite_node():
    d = ROOT / "verification" / "browser-parity"
    rc, out, err = run(["node", "--test", "--test-reporter=tap", "native-wasm-diff.test.mjs", "baseline-stamp.test.mjs"], 300, d)
    text = out + "\n" + err

    def n(key):
        m = re.search(rf"^# {key} (\d+)$", text, re.M)
        return int(m.group(1)) if m else None

    tests, passed, failed, skipped = n("tests"), n("pass"), n("fail"), n("skipped")
    if tests is None or passed is None:
        raise Unavailable(f"node --test printed no TAP summary (exit {rc}): {' '.join(text.strip().splitlines()[-2:])[:200]}")
    return {"passed": passed, "failed": failed or 0, "skipped": skipped or 0, "files": 2, "exit_code": rc}


result["suites"]["node_unit"] = probe("suites.node_unit", p_suite_node, suite_null)

# --------------------------------------------------------------------------------------------------
# CI: latest COMPLETED run on main, per-job conclusions, skipped reported as dark
# --------------------------------------------------------------------------------------------------
GATING_IDS: list = []
JOB_NAMES: dict = {}   # job id -> display name, from ci.yml


def load_ci():
    global GATING_IDS, JOB_NAMES
    try:
        import yaml  # type: ignore
        doc = yaml.safe_load(CI_YML.read_text())
        jobs = doc.get("jobs") or {}
        JOB_NAMES = {k: (v.get("name") or k) for k, v in jobs.items()}
        gate = jobs.get("gate") or {}
        needs = gate.get("needs") or []
        GATING_IDS = ([needs] if isinstance(needs, str) else list(needs)) + (["gate"] if "gate" in jobs else [])
    except Exception as e:  # PyYAML missing or ci.yml unreadable
        REASONS["ci_yml"] = f"cannot read gating jobs from .github/workflows/ci.yml ({type(e).__name__}: {e})"


load_ci()


def ci_run():
    slug = repo_slug()
    runs = gh_json(["run", "list", "-R", slug, "--branch", "main", "--workflow", "ci.yml", "--status", "completed", "--limit", "1",
                    "--json", "databaseId,conclusion,headSha,createdAt,updatedAt,event"])
    if not runs:
        raise Unavailable("no completed CI run exists on main")
    run_row = runs[0]
    view = gh_json(["run", "view", str(run_row["databaseId"]), "-R", slug, "--json", "jobs"])
    return run_row, (view or {}).get("jobs") or []


def p_gate_receipts():
    run_row, jobs = ci_run()
    out = {}
    for j in jobs:
        concl = j.get("conclusion") or None
        out[j.get("name")] = {
            "last_ran": j.get("completedAt"),
            "age_hours": age_hours(j.get("completedAt")),
            "conclusion": concl,
            # skipped / cancelled / never concluded proves nothing about the code: dark, not green
            "dark": concl not in ("success", "failure"),
            "run_id": run_row["databaseId"],
        }
    result["app_specific"]["ci_main_run"] = {
        "run_id": run_row["databaseId"], "conclusion": run_row.get("conclusion"), "head_sha": run_row.get("headSha"),
        "event": run_row.get("event"), "created_at": run_row.get("createdAt"), "age_hours": age_hours(run_row.get("updatedAt")),
    }
    return out


def gate_null(reason: str) -> dict:
    result["app_specific"]["ci_main_run"] = {"run_id": None, "conclusion": None, "reason": reason}
    return {"ci_main": {"last_ran": None, "age_hours": None, "conclusion": None, "dark": True, "reason": reason}}


result["gate_receipts"] = probe("gate_receipts", p_gate_receipts, gate_null)


# --------------------------------------------------------------------------------------------------
# verify_live: newest ground/receipts/*-live.json
# --------------------------------------------------------------------------------------------------
def p_verify_live():
    receipts = []
    for f in (GROUND / "receipts").glob("*-live.json"):
        try:
            receipts.append((f.stat().st_mtime, f, json.loads(f.read_text())))
        except (OSError, ValueError):
            continue
    if not receipts:
        return {"last_receipt": None, "ran_at": None, "age_hours": None, "result": None, "reason": "no verify-live receipt exists yet (scripts/verify-live.sh <epic> writes one)"}
    receipts.sort(key=lambda r: r[0], reverse=True)
    _, f, data = receipts[0]
    return {"last_receipt": str(f.relative_to(ROOT)) if f.is_relative_to(ROOT) else str(f), "ran_at": data.get("ran_at"), "age_hours": age_hours(data.get("ran_at")), "result": data.get("result")}


result["verify_live"] = probe("verify_live", p_verify_live, lambda r: {"last_receipt": None, "ran_at": None, "age_hours": None, "result": None, "reason": r})

# --------------------------------------------------------------------------------------------------
# register_drift: this repo has no REGISTER.md (epics are tracked in the app tracker, 0119-MERIDIAN)
# --------------------------------------------------------------------------------------------------
result["register_drift"] = {
    "counter": None, "highest_file": None, "rows_missing": [], "applicable": False,
    "reason": "oaxaca-blinder-rs has no REGISTER.md; its epics are tracked in the Meridian app tracker and its GitHub issues are listed in open_issues",
}


# --------------------------------------------------------------------------------------------------
# open_issues (GitHub issues of the engine repo)
# --------------------------------------------------------------------------------------------------
def p_open_issues():
    rows = gh_json(["issue", "list", "-R", repo_slug(), "--state", "open", "--limit", "200", "--json", "number,title,createdAt"]) or []
    out = []
    for r in rows:
        created = r.get("createdAt")
        d = age_hours(created)
        out.append({"id": f"#{r['number']}", "title": r.get("title"), "days_open": None if d is None else int(d // 24)})
    return out


result["open_issues"] = probe("open_issues", p_open_issues, None)


# --------------------------------------------------------------------------------------------------
# flake_recurrence: a test named in failed_tests of more than one report
# --------------------------------------------------------------------------------------------------
def p_flake():
    seen: dict = {}
    reports = [r for _, r in PREVIOUS] + [result]
    for rep in reports:
        names = set()
        for s in (rep.get("suites") or {}).values():
            if isinstance(s, dict):
                names.update(s.get("failed_tests") or [])
        for n in names:
            seen[n] = seen.get(n, 0) + 1
    return [{"test": n, "epochs": c} for n, c in sorted(seen.items()) if c >= 2]


result["flake_recurrence"] = probe("flake_recurrence", p_flake, None)


# --------------------------------------------------------------------------------------------------
# app_specific
# --------------------------------------------------------------------------------------------------
def p_wasm_verify():
    if os.environ.get("GROUND_SKIP_WASM_VERIFY"):
        raise Unavailable("skipped on request (GROUND_SKIP_WASM_VERIFY is set)")
    for tool in ("cargo", "rustup"):
        if shutil.which(tool, path=ENV.get("PATH")) is None:
            raise Unavailable(f"`{tool}` not found on PATH, so `build-wasm.sh --verify` cannot build")
    out_json = ROOT / "target" / "wasm-verify.json"
    if out_json.exists():
        out_json.unlink()  # a stale result must not be readable as this run's
    say("app_specific.wasm_verify: scripts/build-wasm.sh --verify")
    rc, out, err = run(["bash", "scripts/build-wasm.sh", "--verify"], WASM_VERIFY_TIMEOUT)
    if not out_json.exists():
        tail = " ".join((err or out).strip().splitlines()[-2:])[:240]
        raise Unavailable(f"build-wasm.sh --verify wrote no result (exit {rc}): {tail}")
    data = json.loads(out_json.read_text())
    data["exit_code"] = rc
    return data


result["app_specific"]["wasm_verify"] = probe("app_specific.wasm_verify", p_wasm_verify, lambda r: {"match": None, "reason": r})


def p_cargo_audit():
    rc, out, err = run(["cargo", "audit", "--json"], 300)
    if not out.strip():
        raise Unavailable(f"`cargo audit` printed nothing (exit {rc}): {err.strip()[:200]}")
    d = json.loads(out)
    warns = {k: len(v) for k, v in (d.get("warnings") or {}).items()}
    return {"vulnerabilities": (d.get("vulnerabilities") or {}).get("count"), "warnings": sum(warns.values()), "warnings_by_kind": warns, "exit_code": rc}


result["app_specific"]["cargo_audit"] = probe(
    "app_specific.cargo_audit", p_cargo_audit, lambda r: {"vulnerabilities": None, "warnings": None, "reason": r}
)


def p_dependabot():
    rows = gh_json(["api", f"repos/{repo_slug()}/dependabot/alerts?state=open&per_page=100"]) or []
    return {"open": len(rows), "alerts": [{"number": r.get("number"), "package": ((r.get("dependency") or {}).get("package") or {}).get("name")} for r in rows]}


result["app_specific"]["dependabot"] = probe("app_specific.dependabot", p_dependabot, lambda r: {"open": None, "reason": r})


def p_required_checks():
    names = sorted(set(JOB_NAMES.values()))
    if not names:
        raise Unavailable(REASONS.get("ci_yml", "no job names read from ci.yml"))
    slug = repo_slug()
    rc, out, err = run(["gh", "api", f"repos/{slug}/branches/main/protection/required_status_checks"], NET_TIMEOUT)
    if rc != 0:
        if "not protected" in (out + err).lower() or "404" in (out + err):
            return {"required": [], "ci_job_names": names, "missing_in_ci": [], "protected": False, "note": "main has no branch protection"}
        raise Unavailable(f"`gh api .../required_status_checks` failed (exit {rc}): {(err or out).strip()[:200]}")
    contexts = json.loads(out).get("contexts") or []
    return {"required": contexts, "ci_job_names": names, "missing_in_ci": [c for c in contexts if c not in names], "protected": True}


result["app_specific"]["required_checks"] = probe(
    "app_specific.required_checks", p_required_checks, lambda r: {"required": None, "ci_job_names": None, "missing_in_ci": None, "reason": r}
)


def p_ci_invariants():
    rc, out, err = run(["python3", "scripts/check-ci-invariants.py"], 120)
    return {"exit_code": rc, "ok": rc == 0, "output": (out + err).strip()[-400:]}


result["app_specific"]["ci_invariants"] = probe("app_specific.ci_invariants", p_ci_invariants, lambda r: {"exit_code": None, "ok": None, "reason": r})

# --------------------------------------------------------------------------------------------------
# validator
# --------------------------------------------------------------------------------------------------
ran_total = total_tests(result)
prev_total = last_total()
if ran_total is None:
    pass  # the suites' own reasons are reported below
elif ran_total == 0:
    result["errors"].append("validator: total tests is 0")
elif prev_total is not None and ran_total < prev_total:
    result["errors"].append(f"validator: total tests {ran_total} is below the previous probes file ({prev_total})")
result["app_specific"]["total_tests"] = {"total": ran_total, "previous": prev_total}

gr = result["gate_receipts"]
if "ci_main" not in gr:
    if not GATING_IDS:
        REASONS.setdefault("ci_yml", "ci.yml has no `gate` job with a `needs` list on this commit")
    for jid in GATING_IDS:
        name = JOB_NAMES.get(jid, jid)
        row = gr.get(name)
        if row is None:
            result["errors"].append(f"validator: gating job `{name}` ({jid}) has no entry in the latest completed main run (main's CI predates the gate)")
        elif row.get("conclusion") not in ("success", "failure"):
            result["errors"].append(f"validator: gating job `{name}` has no completed conclusion in the latest main run (conclusion: {row.get('conclusion')})")

for key, reason in REASONS.items():
    result["errors"].append(f"probe could not run: {key}: {reason}")

# --------------------------------------------------------------------------------------------------
GROUND.mkdir(exist_ok=True)
out_file = GROUND / f"{datetime.now(timezone.utc).strftime('%Y-%m-%d')}-probes.json"
tmp = out_file.with_suffix(".json.tmp")


def scrub(text: str) -> str:
    """This repo is public and the probes file is committed: no absolute path of this machine goes in it (0119 review OPS-2)."""
    for real, label in ((str(ROOT.parent), "<workspace>"), (str(Path.home()), "~")):
        text = text.replace(real, label)
    return text


payload = scrub(json.dumps(result, indent=2)) + "\n"
tmp.write_text(payload)
os.replace(tmp, out_file)
print(payload, end="")
sys.exit(0)
