#!/usr/bin/env python3
"""Brute-force oracle for the remedy's budget rule (0122-MERIDIAN V1).

REGENERATION-ONLY TOOLING. `cargo test` never runs Python: engine/tests/remedy_rules_test.rs reads the
committed `engine/tests/fixtures/remedy_bruteforce.json` and REFUSES it when the recorded sha256 of this
script or of the fixture no longer matches the files on disk.

NO EXPECTED VALUE COMES FROM ENGINE OUTPUT. The roster is `engine/tests/fixtures/0122-remedy-tiny.csv`:
three reference rows on the exact line wage = 40000 + 1000 x, and three compared rows. The fair wage of a
compared row is therefore its line value, with no regression involved. The remedy the app states is:

    pay each compared employee p_i, 0 <= p_i <= max(0, fair_i - wage_i), so that the group's mean gap
    g = mean(wage_i + p_i - fair_i) reaches a target g_T; spend as little as possible.

This script ENUMERATES every p on a 250-dollar grid (itertools.product over the three caps) and records
the minimum cost over the feasible points, the number of feasible points, and the cost of every feasible
point's cheapest member. It is not an LP solver and shares no formula with the engine: it simply tries
everything. The closed form n_T (g_T - g_0) the engine uses, and the claim that every allocation of that
cost is optimal, are tested against it.

Sign: g is mean(wage - fair), negative while the group sits below the line (the engine's convention).

    python3 verification/gen_remedy_bruteforce.py
"""
import csv, hashlib, itertools, json, os, sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
FIXTURE = "engine/tests/fixtures/0122-remedy-tiny.csv"
OUT = os.path.join(ROOT, "engine/tests/fixtures/remedy_bruteforce.json")
STEP = 250.0
TARGETS = [-2000.0, -1333.5, -1000.0, -500.0, 0.0, 250.0, 333.0, 334.0, 500.0]


def sha(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


rows = list(csv.DictReader(open(os.path.join(ROOT, FIXTURE))))
ref = [(float(r["x"]), float(r["wage"])) for r in rows if r["group"] == "R"]
tgt = [(float(r["x"]), float(r["wage"])) for r in rows if r["group"] == "T"]
# the reference rows lie exactly on wage = a + b x; recover a, b from the two end points (no regression)
(x0, y0), (x1, y1) = ref[0], ref[-1]
b = (y1 - y0) / (x1 - x0)
a = y0 - b * x0
for x, y in ref:
    assert abs(a + b * x - y) < 1e-9, "the reference rows must lie on one exact line"
fair = [a + b * x for x, _ in tgt]
wage = [y for _, y in tgt]
cap = [max(0.0, f - w) for f, w in zip(fair, wage)]
n = len(tgt)
g0 = sum(w - f for f, w in zip(fair, wage)) / n
grid = [[k * STEP for k in range(int(c / STEP) + 1)] for c in cap]
assert all(abs(c % STEP) < 1e-9 for c in cap), "caps must sit on the grid"


def gap(p):
    return sum(w + pi - f for f, w, pi in zip(fair, wage, p)) / n


cases = []
for gT in TARGETS:
    best, feasible = None, 0
    for p in itertools.product(*grid):
        if gap(p) >= gT - 1e-9:
            feasible += 1
            c = sum(p)
            if best is None or c < best:
                best = c
    cases.append({"target_gap": gT, "feasible_points": feasible, "min_cost": best})

golden = {
    "_meta": {
        "generator_sha256": sha(os.path.abspath(__file__)),
        "fixture_sha256": {FIXTURE: sha(os.path.join(ROOT, FIXTURE))},
        "expected_values_from_engine_output": False,
        "grid_step": STEP,
    },
    "fair": fair, "wage": wage, "caps": cap, "g0": g0,
    "cases": cases,
}
with open(OUT, "w") as f:
    json.dump(golden, f, indent=2)
print("wrote", OUT)
for c in cases:
    print(c)
