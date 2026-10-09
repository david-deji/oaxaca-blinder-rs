#!/usr/bin/env python3
"""Guard for .github/workflows (0119-MERIDIAN S2, S5/V5).

A green CI run only means something while the workflow cannot silence itself. This fails when:
  * a gating job has `continue-on-error` (job or step level),
  * a gating job other than `gate` and `browser-parity` has a job-level `if` (a skipped job is how
    the old browser-parity check went dark for six weeks); `browser-parity` may carry only the one
    reviewed expression below,
  * `gate` is not named `gate`, does not run `if: always()`, or its `needs` differs from the set of
    gating jobs (every job except `gate` and the schedule-only benchmark),
  * a job has no `timeout-minutes`,
  * the top-level `permissions` is not exactly `contents: read` (in every workflow file),
  * a third-party action is not pinned to a full 40-hex commit SHA (every workflow file),
  * a run step uses `npm install` or pipes a download into a shell,
  * `on.push` / `on.pull_request` use `paths` or `paths-ignore` (a PR that never reports `gate`
    could not merge once `gate` is required).

`--protection OWNER/REPO` additionally diffs branches/main/protection against the plan
(contexts == ["gate"], strict false, enforce_admins false, no required reviews, no force pushes,
no deletions). Exit 0 = clean, 1 = findings, 2 = could not run.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"
CI = WORKFLOWS / "ci.yml"

NON_GATING = {"benchmark-speedup"}  # schedule/dispatch only, never fails the run
GATE = "gate"
ALWAYS_ALLOWED = {
    "browser-parity": "always() && needs.wasm-threaded.result != 'cancelled'",
}
SHA_RE = re.compile(r"^[0-9a-f]{40}$")


def load_yaml(path):
    try:
        import yaml  # type: ignore

        with open(path, encoding="utf-8") as f:
            return yaml.safe_load(f)
    except ImportError:
        out = subprocess.run(["yq", "-o=json", ".", str(path)], capture_output=True, text=True)
        if out.returncode != 0:
            print(f"cannot parse {path}: PyYAML missing and yq failed: {out.stderr}", file=sys.stderr)
            sys.exit(2)
        return json.loads(out.stdout)


def norm(expr):
    expr = str(expr).strip()
    if expr.startswith("${{") and expr.endswith("}}"):
        expr = expr[3:-2].strip()
    return re.sub(r"\s+", " ", expr)


def triggers(doc):
    # PyYAML (YAML 1.1) parses the key `on` as boolean True.
    return doc.get("on", doc.get(True, {})) or {}


def check_workflow_common(path, doc, findings):
    name = path.name
    if doc.get("permissions") != {"contents": "read"}:
        findings.append(f"{name}: top-level permissions must be exactly {{contents: read}}, found {doc.get('permissions')!r}")
    for job_id, job in (doc.get("jobs") or {}).items():
        if not isinstance(job.get("timeout-minutes"), int):
            findings.append(f"{name}: job {job_id} has no integer timeout-minutes")
        steps = job.get("steps") or []
        uses = [job["uses"]] if "uses" in job else []
        uses += [s["uses"] for s in steps if "uses" in s]
        for u in uses:
            if u.startswith("./"):
                continue
            ref = u.split("@", 1)[1] if "@" in u else ""
            if not SHA_RE.match(ref):
                findings.append(f"{name}: job {job_id}: action not pinned to a full commit SHA: {u}")
        for s in steps:
            run = s.get("run", "")
            if re.search(r"\bnpm\s+install\b", run):
                findings.append(f"{name}: job {job_id}: `npm install` (use `npm ci`)")
            if re.search(r"(curl|wget)[^\n|]*\|\s*(sudo\s+)?(sh|bash)\b", run):
                findings.append(f"{name}: job {job_id}: downloads piped into a shell")


def check_ci(doc, findings):
    jobs = doc.get("jobs") or {}
    gating = {j for j in jobs if j != GATE and j not in NON_GATING}

    for t in ("push", "pull_request"):
        cfg = triggers(doc).get(t) or {}
        for k in ("paths", "paths-ignore"):
            if k in cfg:
                findings.append(f"ci.yml: on.{t}.{k} is set; a PR that never reports `{GATE}` could not merge")

    for j in sorted(gating):
        job = jobs[j]
        if "continue-on-error" in job:
            findings.append(f"ci.yml: gating job {j} has continue-on-error")
        for s in job.get("steps") or []:
            if "continue-on-error" in s:
                findings.append(f"ci.yml: gating job {j}: step {s.get('name', '?')!r} has continue-on-error")
        if "if" in job:
            allowed = ALWAYS_ALLOWED.get(j)
            if allowed is None or norm(job["if"]) != allowed:
                findings.append(
                    f"ci.yml: gating job {j} has job-level `if: {job['if']}`"
                    + (f" (only `{allowed}` is allowed)" if allowed else " (gating jobs run unconditionally)")
                )
        elif j in ALWAYS_ALLOWED:
            findings.append(f"ci.yml: {j} must carry `if: {ALWAYS_ALLOWED[j]}` so a hash mismatch cannot skip it")

    gate = jobs.get(GATE)
    if gate is None:
        findings.append(f"ci.yml: no `{GATE}` job")
        return
    if gate.get("name") != GATE:
        findings.append(f"ci.yml: the gate job's name must be exactly {GATE!r} (the required check), found {gate.get('name')!r}")
    if norm(gate.get("if", "")) != "always()":
        findings.append(f"ci.yml: {GATE} must run `if: always()`, found {gate.get('if')!r}")
    needs = set(gate.get("needs") or [])
    if needs != gating:
        findings.append(
            f"ci.yml: {GATE}.needs differs from the gating jobs: missing {sorted(gating - needs)}, extra {sorted(needs - gating)}"
        )
    return gating


def check_protection(repo, gate_name, findings):
    out = subprocess.run(["gh", "api", f"repos/{repo}/branches/main/protection"], capture_output=True, text=True)
    if out.returncode != 0:
        findings.append(f"protection: could not read branches/main/protection: {out.stderr.strip() or out.stdout.strip()}")
        return
    p = json.loads(out.stdout)
    rsc = p.get("required_status_checks") or {}
    contexts = rsc.get("contexts") or [c.get("context") for c in rsc.get("checks", [])]
    if contexts != [gate_name]:
        findings.append(f"protection: required contexts {contexts!r}, plan is [{gate_name!r}]")
    if rsc.get("strict") is not False:
        findings.append(f"protection: strict is {rsc.get('strict')!r}, plan is false")
    if (p.get("enforce_admins") or {}).get("enabled") is not False:
        findings.append(f"protection: enforce_admins is {(p.get('enforce_admins') or {}).get('enabled')!r}, plan is false")
    if p.get("required_pull_request_reviews") is not None:
        findings.append("protection: required_pull_request_reviews is set, plan is none")
    if (p.get("allow_force_pushes") or {}).get("enabled") is not False:
        findings.append("protection: force pushes are allowed, plan is none")
    if (p.get("allow_deletions") or {}).get("enabled") is not False:
        findings.append("protection: deletions are allowed, plan is none")


def main(argv):
    findings = []
    ci = load_yaml(CI)
    gating = check_ci(ci, findings) or set()
    for path in sorted(WORKFLOWS.glob("*.y*ml")):
        check_workflow_common(path, load_yaml(path) if path != CI else ci, findings)
    if "--protection" in argv:
        repo = argv[argv.index("--protection") + 1]
        check_protection(repo, (ci.get("jobs") or {}).get(GATE, {}).get("name", GATE), findings)

    print(f"workflow invariants: gating jobs = {sorted(gating)}")
    if findings:
        for f in findings:
            print(f"FAIL {f}")
        return 1
    print("workflow invariants: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
