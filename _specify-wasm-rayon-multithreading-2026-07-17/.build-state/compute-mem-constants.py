#!/usr/bin/env python3
"""D2/D3 shared-memory sizing constants from the committed D1 profile.
Usage: compute_constants.py H_res_bytes Sc_bytes St_obs_bytes
All formulas per phase4-final-memory-budget.md D2/D3. Placeholder safety
constants carry their cited bands (D2 table). N_max_const folds in the 8-clamp
at emission (council MJ-6)."""
import sys, math

PAGE = 65_536
St = 1_048_576          # per-thread stack, 1 MiB placeholder (band 0.5-1 MiB, shared-memory-limits:20); must be >= St_obs w/ margin
Marg = 67_108_864       # max-sizing margin, 64 MiB (shared-memory-limits:22)
Marg_init = 16_777_216  # initial-sizing margin, 16 MiB (derived)
headroom_frac = 0.15    # INV-05 ceiling
N_target = 8            # desired worker count (band 4-8, shared-memory-limits:13); == the 8-clamp ceiling
GiB = 1024**3
MiB = 1024**2

def round_up_page(x): return math.ceil(x / PAGE) * PAGE

H_res, Sc, St_obs = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3])

M_max_formula = round_up_page(H_res + N_target*(Sc + St) + Marg)
# Band guidance: default 256-512 MiB envelope; give the buffer headroom by
# taking at least the 256 MiB band floor (larger M_max is strictly safer; the
# thread cap clamps to 8 regardless). Never exceed the 2 GiB portable ceiling.
BAND_FLOOR = 256*MiB
M_max = min(max(M_max_formula, BAND_FLOOR), 2*GiB)
M_init = round_up_page(H_res + Sc + Marg_init)

N_max = math.floor((M_max - H_res - Marg) / (Sc + St))
N_max_const = min(N_max, 8)
N_chosen = min(N_max_const, 8)  # hardwareConcurrency folded at runtime; here upper-bounded by 8

# Consistency assert (D2): M_max >= H_res + N*(Sc+St) + Marg
consistency = M_max >= H_res + N_chosen*(Sc + St) + Marg
St_ok = St >= St_obs

print(f"INPUTS (measured @ 50k):")
print(f"  H_res   = {H_res:>12,} B ({H_res/MiB:.2f} MiB)")
print(f"  Sc      = {Sc:>12,} B ({Sc/MiB:.3f} MiB)")
print(f"  St_obs  = {St_obs:>12,} B ({St_obs/1024:.1f} KiB)")
print(f"CONSTANTS (band-sourced placeholders):")
print(f"  St={St:,} Marg={Marg:,} Marg_init={Marg_init:,} N_target={N_target} headroom_frac={headroom_frac}")
print(f"OUTPUTS:")
print(f"  M_max (formula)     = {M_max_formula:>12,} B ({M_max_formula/MiB:.2f} MiB)")
print(f"  M_max (band-floored)= {M_max:>12,} B ({M_max/MiB:.2f} MiB)   [<= 2 GiB: {M_max <= 2*GiB}]")
print(f"  M_init              = {M_init:>12,} B ({M_init/MiB:.2f} MiB)")
print(f"  N_max (uncapped)    = {N_max}")
print(f"  N_max_const (min(_,8)) = {N_max_const}   <-- AC-M6 build-time constant")
print(f"CHECKS:")
print(f"  St >= St_obs (AC-M4 margin): {St_ok}")
print(f"  M_max consistency assert (AC-M7): {consistency}")
print(f"  N_max_const == min(floor((M_max-H_res-Marg)/(Sc+St)),8): "
      f"{N_max_const == min(math.floor((M_max-H_res-Marg)/(Sc+St)),8)}")
print(f"  memory {'NON-BINDING' if N_max>=8 else 'BINDING'} at 50k (N_max={N_max} vs 8-clamp)")
# link-arg values for toolchain-build handoff
print(f"LINK-ARG VALUES (handoff to toolchain-build):")
print(f"  -zstack-size={St}")
print(f"  --max-memory={M_max}")
print(f"  --initial-memory={M_init}")
