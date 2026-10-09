#!/usr/bin/env python3
"""Audit a finished run's jobs for silenced steps (0119-MERIDIAN, run by the `gate` job).

Input: the JSON that `gh api repos/<repo>/actions/runs/<run id>/jobs --paginate` prints (one or more
concatenated objects with a `jobs` list). For every job other than `gate` whose conclusion is `success`,
every step must also be `success`: a skipped step (`if: false`, a failed `if` expression) or a cancelled step
leaves the job green and says nothing, which is what `gate` exists to refuse. A job that is skipped or failed
as a whole is not examined here: `gate`'s first step already turns that red.

--expect-jobs N: at least N non-gate jobs must have been seen as `success`, so an empty or truncated API
answer cannot pass for "nothing skipped".
Exit 0 = clean, 1 = findings, 2 = unreadable input.
"""
import json
import sys


def load_jobs(text):
    dec = json.JSONDecoder()
    i, jobs = 0, []
    text = text.strip()
    while i < len(text):
        obj, end = dec.raw_decode(text, i)
        jobs.extend(obj.get("jobs") or [])
        i = end
        while i < len(text) and text[i].isspace():
            i += 1
    return jobs


def audit(jobs, expect_jobs=0):
    findings, seen = [], 0
    for job in jobs:
        if job.get("name") == "gate" or job.get("conclusion") != "success":
            continue
        seen += 1
        for step in job.get("steps") or []:
            if step.get("conclusion") != "success":
                findings.append(f"job {job.get('name')!r}: step {step.get('name')!r} is {step.get('conclusion')!r}")
    if seen < expect_jobs:
        findings.append(f"saw {seen} successful jobs in the jobs API answer, expected at least {expect_jobs}")
    return findings, seen


def main(argv):
    expect = 0
    if "--expect-jobs" in argv:
        expect = int(argv[argv.index("--expect-jobs") + 1])
    paths = [a for i, a in enumerate(argv) if not a.startswith("--") and (i == 0 or argv[i - 1] != "--expect-jobs")]
    try:
        text = open(paths[0], encoding="utf-8").read() if paths else sys.stdin.read()
        jobs = load_jobs(text)
    except (OSError, ValueError, IndexError) as e:
        print(f"gate-step-audit: cannot read the jobs answer: {e}", file=sys.stderr)
        return 2
    findings, seen = audit(jobs, expect)
    if findings:
        for f in findings:
            print(f"FAIL {f}")
        return 1
    print(f"gate-step-audit: {seen} successful jobs, every step success")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
