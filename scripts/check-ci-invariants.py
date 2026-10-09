#!/usr/bin/env python3
"""Guard for .github/workflows (0119-MERIDIAN S2, S5/V5).

A green CI run only means something while the workflow cannot silence itself. This fails when:
  * a gating job has `continue-on-error` (job or step level),
  * a gating job other than `gate` and `browser-parity` has a job-level `if` (a skipped job is how
    the old browser-parity check went dark for six weeks); `browser-parity` may carry only the one
    reviewed expression below,
  * a step of a gating job, of `gate` or of a release.yml job has a step-level `if` that is not in ALLOWED_STEP_IFS (`if: ${{ false }}`
    leaves the job `success` and `gate` green), a `shell:` override (`{0}` drops errexit), or `defaults`,
  * a `run` text of those steps contains `|| true`, `|| :`, `set +e` or `exit 0`,
  * a job-level `permissions` differs from JOB_PERMISSIONS (default: none, so the top-level
    `contents: read` applies); this covers release.yml too,
  * `gate` is not named `gate`, does not run `if: always()`, its `needs` differs from the set of
    gating jobs (every job except `gate` and the schedule-only benchmark), or its steps differ from
    GATE_STEPS (the step that turns a non-success need red is held verbatim),
  * a job has no `timeout-minutes`,
  * the top-level `permissions` is not exactly `contents: read` (in every workflow file),
  * a third-party action is not pinned to a full 40-hex commit SHA (every workflow file),
  * a run step uses `npm install` or pipes a download into a shell,
  * `on.push` / `on.pull_request` use `paths` or `paths-ignore` (a PR that never reports `gate`
    could not merge once `gate` is required).

The script is itself run by `gate`; scripts/test-ci-invariants.py proves it fails on mutated copies of
the workflows. A static check cannot see every way to disable a step, so `gate` also audits the finished
run through the jobs API (scripts/gate-step-audit.py): no step of a successful job may be skipped.

`--workflows DIR` checks another directory of workflows (the test uses it). `--protection OWNER/REPO`
additionally diffs branches/main/protection against the plan (contexts == ["gate"], strict false,
enforce_admins false, no required reviews, no force pushes, no deletions).
Exit 0 = clean, 1 = findings, 2 = could not run.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"

NON_GATING = {"benchmark-speedup"}  # schedule/dispatch only, never fails the run
GATE = "gate"
ALWAYS_ALLOWED = {
    "browser-parity": "always() && needs.wasm-threaded.result != 'cancelled'",
}
SHA_RE = re.compile(r"^[0-9a-f]{40}$")

# Step-level `if` is how a step is silently switched off. Only these exact (file, job, step name) pairs may
# carry one; each keeps a validate, record or upload step running after an earlier step failed, and none of
# them guards a check that decides the result.
CANCELLED = "!cancelled()"
ALLOWED_STEP_IFS = {
    ("ci.yml", "security", "Validate .cargo/audit.toml ignores"): CANCELLED,
    ("ci.yml", "wasm-seq", "Record raw WASM hash and toolchain"): CANCELLED,
    ("ci.yml", "wasm-seq", "Upload raw sequential WASM"): CANCELLED,
    ("ci.yml", "wasm-threaded", "Record raw WASM hash and toolchain"): CANCELLED,
    ("ci.yml", "wasm-threaded", "Upload raw threaded WASM"): CANCELLED,
    ("ci.yml", "wasm-threaded", "Upload threaded WASM pkg (input of browser-parity)"): CANCELLED,
    ("ci.yml", "wasm-repro", "Record hashes"): CANCELLED,
    ("ci.yml", "wasm-repro", "Upload both builds"): CANCELLED,
}

# Job-level permissions: absent (the top-level `contents: read` applies) unless listed here.
JOB_PERMISSIONS = {
    ("ci.yml", GATE): {"contents": "read", "actions": "read"},  # actions: read lets gate list the run's jobs and steps
    ("release.yml", "github-release"): {"contents": "write"},   # publishes the release; nothing else (no id-token)
}

# Text in a `run:` that makes a failing command exit 0 or stops errexit.
RUN_SILENCERS = [
    (re.compile(r"\|\|\s*(true|:)(\s|;|$)"), "`|| true` / `|| :`"),
    (re.compile(r"\bset\s+\+[a-z]*e"), "`set +e`"),
    (re.compile(r"\bexit\s+0\b"), "`exit 0`"),
]

# The gate's steps, held verbatim (after the checkout). Step 1 is what turns a skipped or failed job into a
# red run: editing it to `exit 0`, adding `continue-on-error`, or putting an `if` on it must fail.
GATE_STEPS = [
    {
        "name": "Every gating job must be success (skipped and cancelled are failures)",
        "if": "always()",
        "env": {"NEEDS": "${{ toJSON(needs) }}"},
        "run": """echo "$NEEDS" | jq -r 'to_entries[] | "\\(.key): \\(.value.result)"'
BAD=$(echo "$NEEDS" | jq -r 'to_entries[] | select(.value.result != "success") | "\\(.key)=\\(.value.result)"')
if [ -n "$BAD" ]; then
  echo "gate: not success:"; echo "$BAD"; exit 1
fi
echo "gate: all $(echo "$NEEDS" | jq 'length') gating jobs succeeded"
""",
    },
    {
        "name": "No step of a successful job was skipped or cancelled (the run's jobs API)",
        "if": "always()",
        "env": {
            "NEEDS": "${{ toJSON(needs) }}",
            "GH_TOKEN": "${{ github.token }}",
            "REPO": "${{ github.repository }}",
            "RUN_ID": "${{ github.run_id }}",
        },
        "run": """gh api "repos/${REPO}/actions/runs/${RUN_ID}/jobs?per_page=100" --paginate > run-jobs.json
python3 scripts/gate-step-audit.py run-jobs.json --expect-jobs "$(echo "$NEEDS" | jq 'length')"
""",
    },
    {
        "name": "Workflow invariants (scripts/check-ci-invariants.py)",
        "if": "always()",
        "run": "python3 scripts/check-ci-invariants.py\n",
    },
    {
        "name": "Tests of the workflow guard (mutated copies must fail)",
        "if": "always()",
        "run": "python3 scripts/test-ci-invariants.py\n",
    },
]


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


def check_steps(fname, job_id, job, findings, allowed_ifs=ALLOWED_STEP_IFS):
    """Step-level silencers, for a gating job or for gate."""
    if "defaults" in job:
        findings.append(f"{fname}: job {job_id} sets `defaults` (a shell override can drop errexit)")
    for s in job.get("steps") or []:
        sname = s.get("name", s.get("uses", "?"))
        where = f"{fname}: job {job_id}: step {sname!r}"
        if "continue-on-error" in s:
            findings.append(f"{where} has continue-on-error")
        if "shell" in s:
            findings.append(f"{where} overrides `shell` (`bash {{0}}` drops errexit)")
        if "if" in s:
            allowed = allowed_ifs.get((fname, job_id, s.get("name")))
            if allowed is None or norm(s["if"]) != allowed:
                findings.append(
                    f"{where} has step-level `if: {s['if']}` (not in ALLOWED_STEP_IFS"
                    + (f"; only `{allowed}` is allowed there" if allowed else "")
                    + ")"
                )
        for rx, label in RUN_SILENCERS:
            if rx.search(s.get("run", "") or ""):
                findings.append(f"{where}: run text contains {label}")


def check_workflow_common(path, doc, findings):
    name = path.name
    if doc.get("permissions") != {"contents": "read"}:
        findings.append(f"{name}: top-level permissions must be exactly {{contents: read}}, found {doc.get('permissions')!r}")
    if "defaults" in doc:
        findings.append(f"{name}: top-level `defaults` (a shell override can drop errexit)")
    for job_id, job in (doc.get("jobs") or {}).items():
        if not isinstance(job.get("timeout-minutes"), int):
            findings.append(f"{name}: job {job_id} has no integer timeout-minutes")
        want = JOB_PERMISSIONS.get((name, job_id))
        if job.get("permissions") != want:
            findings.append(
                f"{name}: job {job_id}: job-level permissions {job.get('permissions')!r}, reviewed value is {want!r}"
            )
        steps = job.get("steps") or []
        if name != "ci.yml":  # ci.yml's gating jobs and gate are checked in check_ci
            check_steps(name, job_id, job, findings)
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


def check_gate_steps(gate, findings):
    steps = gate.get("steps") or []
    if not steps or "uses" not in steps[0] or not str(steps[0]["uses"]).startswith("actions/checkout@"):
        findings.append("ci.yml: gate step 1 must be actions/checkout (the guard scripts are read from the checkout)")
        return
    got = steps[1:]
    if len(got) != len(GATE_STEPS):
        findings.append(f"ci.yml: gate has {len(got)} steps after checkout, expected {len(GATE_STEPS)} ({[g['name'] for g in GATE_STEPS]})")
    for want, have in zip(GATE_STEPS, got):
        label = f"ci.yml: gate step {want['name']!r}"
        if have.get("name") != want["name"]:
            findings.append(f"{label}: found a step named {have.get('name')!r} in its place")
            continue
        if norm(have.get("if", "")) != want["if"]:
            findings.append(f"{label}: `if` is {have.get('if')!r}, must be {want['if']!r}")
        if (have.get("env") or None) != (want.get("env") or None):
            findings.append(f"{label}: env is {have.get('env')!r}, must be {want.get('env')!r}")
        if (have.get("run") or "").strip() != want["run"].strip():
            findings.append(f"{label}: run text differs from the reviewed text in scripts/check-ci-invariants.py")
        extra = set(have) - {"name", "if", "env", "run"}
        if extra:
            findings.append(f"{label}: unexpected keys {sorted(extra)}")


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
        check_steps("ci.yml", j, job, findings)
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
        return gating
    if "continue-on-error" in gate:
        findings.append(f"ci.yml: {GATE} has continue-on-error")
    if gate.get("name") != GATE:
        findings.append(f"ci.yml: the gate job's name must be exactly {GATE!r} (the required check), found {gate.get('name')!r}")
    if norm(gate.get("if", "")) != "always()":
        findings.append(f"ci.yml: {GATE} must run `if: always()`, found {gate.get('if')!r}")
    needs = set(gate.get("needs") or [])
    if needs != gating:
        findings.append(
            f"ci.yml: {GATE}.needs differs from the gating jobs: missing {sorted(gating - needs)}, extra {sorted(needs - gating)}"
        )
    # gate's own steps: the same silencer checks, with `always()` allowed where GATE_STEPS says so
    gate_ifs = {("ci.yml", GATE, g["name"]): g["if"] for g in GATE_STEPS}
    check_steps("ci.yml", GATE, gate, findings, allowed_ifs=gate_ifs)
    check_gate_steps(gate, findings)
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
    workflows = WORKFLOWS
    if "--workflows" in argv:
        workflows = Path(argv[argv.index("--workflows") + 1])
    ci_path = workflows / "ci.yml"
    findings = []
    ci = load_yaml(ci_path)
    gating = check_ci(ci, findings) or set()
    for path in sorted(workflows.glob("*.y*ml")):
        check_workflow_common(path, load_yaml(path) if path != ci_path else ci, findings)
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
