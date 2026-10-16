# Deep Dive Code Audit: `oaxaca-blinder-rs`

**Date:** 2026-03-14
**Scope:** Full workspace — `oaxaca_blinder`, `pay-equity-engine`, `meridian-mcp`

## Executive Summary

5 parallel audit agents analyzed the workspace across security, correctness, architecture, math, and MCP protocol dimensions. After deduplication: **4 critical, 14 high, 12 medium, 6 low** findings.

---

## CRITICAL (fix immediately)

### C1. Undefined Behavior: Unsafe pointer cast in Python bindings
**`oaxaca_blinder/src/python.rs:230-236`**

```rust
let df: DataFrame = unsafe {
    let ptr = &df_old_version as *const _ as *const DataFrame;
    std::ptr::read(ptr)
};
std::mem::forget(df_old_version);
```

Raw pointer cast between potentially incompatible Polars DataFrame versions. If struct layouts differ at all, this silently corrupts memory. **Action:** Remove the unsafe cast. Use Arrow IPC or pin to a single Polars version across the Python binding boundary.

### C2. Panicking unwraps on user data in AKM
**`oaxaca_blinder/src/akm.rs:272,278`**

```rust
.map(|opt| worker_map.get(opt.unwrap()).unwrap().clone())
```

Any null in worker/firm ID columns crashes the process. This is reachable from the MCP server. **Action:** Replace with `ok_or(AkmError::...)? `.

### C3. MCP auth bypass when API key unset
**`meridian-mcp/src/main.rs:49-50,317`**

`api_key` is `Option<String>`. If unset, the server starts with zero authentication and no warning. Combined with binding to `0.0.0.0` (line 262), this exposes all tools to the network. **Action:** Either require the key at startup or emit a prominent warning. Default bind to `127.0.0.1`.

### C4. Suspended partner gets `Ok` -- access control logic bug
**`engine/src/access.rs:46-48`**

```rust
PartnerStatus::Suspended => Ok(config.clone()),  // should be Err
```

Both `Active` and `Suspended` return `Ok`. **Action:** Return an error for `Suspended`.

---

## HIGH (fix before production)

### H1. Silent null-to-zero imputation across all modules
**`akm.rs:282`, `quantile_decomposition.rs:93`, `matching/engine.rs:75`, `dfl.rs:144`, `engine/analysis.rs:105,117`, and others**

`unwrap_or(0.0)` on wage/outcome values. A single null wage becoming $0 dramatically skews decomposition results. **Action:** Fail on nulls in outcome columns; document imputation policy for predictors.

### H2. Covariance matrix silently falls back to identity
**`engine/src/analysis.rs:474-478`, `engine/src/defensibility.rs:150`**

When `(X'X)` is singular, `try_inverse().unwrap_or_else(|| DMatrix::identity(...))` silently produces meaningless confidence intervals. These intervals drive defensibility determinations. **Action:** Return an error on singularity.

### H3. RIF quantile index uses non-standard formula
**`oaxaca_blinder/src/math/rif.rs:25-27`**

```rust
let q_index = (quantile * n).ceil() as usize;
```

Does not match any standard quantile definition (R Types 1-9). For small samples typical in pay equity (n=20-50), quantile bias is material and propagates through the entire RIF-regression decomposition. **Action:** Use R Type 7 interpolation: `h = (n-1)*tau; lerp(sorted[floor(h)], sorted[ceil(h)], frac(h))`.

### H4. Logit sigmoid has no probability clamping
**`oaxaca_blinder/src/math/logit.rs:15-17`**

No clamping of predicted probabilities away from 0/1. Perfect separation (common in small pay equity datasets) makes the Hessian singular. The probit implementation clamps but logit does not. **Action:** Add `p.clamp(1e-10, 1.0 - 1e-10)`.

### H5. Heckman IMR clamps to zero instead of large positive value
**`oaxaca_blinder/src/heckman.rs:59-60`**

When `Phi(z)` is near zero, IMR should be large and positive (asymptotically `-z`), not zero. Setting to 0.0 defeats the Heckman selection correction for observations near the selection boundary. **Action:** `let imr = (phi / big_phi.max(1e-300)).min(40.0)`.

### H6. No rate limiting on HTTP transport
**`meridian-mcp/src/main.rs:238-266`**

The `governor` rate limiter only applies to stdio mode. The HTTP/SSE transport has zero rate limiting. **Action:** Add tower rate-limit middleware to the Axum router.

### H7. Blocking CPU computation on async runtime
**`meridian-mcp/src/main.rs:572-631`**

All tool calls (decompose, optimize, etc.) run synchronously in `async fn`, starving all other connections. Bootstrap + rayon can block for minutes. **Action:** Wrap in `tokio::task::spawn_blocking()`.

### H8. No request body or parameter limits
**`meridian-mcp/src/main.rs`**

No `DefaultBodyLimit` on the router. `bootstrap_reps` is unbounded -- a caller can set `1000000` to force enormous CPU consumption. **Action:** Set explicit body limit; cap `bootstrap_reps` (e.g., max 10,000).

### H9. Hardcoded secret in Dockerfile
**`meridian-mcp/Dockerfile:13`, `docker-compose.yml:14`**

`MCP_API_KEY=dev-secret-key` baked into the image. **Action:** Remove; use `.env` or Docker secrets.

### H10. `partial_cmp().unwrap()` panics on NaN in sorts
**`quantile_decomposition.rs:152`, `inference.rs:27`, `math/kde.rs:52`, `math/rif.rs:24`, `engine/analysis.rs:705,1064`**

Any `NaN` from upstream computation causes a panic. **Action:** Use `f64::total_cmp` or `unwrap_or(Ordering::Equal)`.

### H11. `expect()` on Heckman selection outcome
**`oaxaca_blinder/src/lib.rs:348`**

Panics if selection outcome has nulls after cleaning. **Action:** Replace with `?` propagation.

### H12. Division by zero in percentage calculations
**`engine/src/analysis.rs:284-286`**

`explained / total * 100.0` produces `NaN` when `total == 0.0`. **Action:** Guard with `if total.abs() < f64::EPSILON { 0.0 } else { ... }`.

### H13. SSE endpoint allows unbounded connection accumulation
**`meridian-mcp/src/main.rs:356-383`**

GET endpoint creates streams that hang forever via `stream::pending()`. No auth on GET, no connection limit, no idle timeout. **Action:** Add auth, connection cap, and idle timeout.

### H14. Defensibility gap metrics are all hardcoded to zero
**`engine/src/defensibility.rs:280-287`**

`total_cost`, `original_gap`, `new_gap`, etc. all hardcoded to `0.0` in the return value. Clients reading these fields are silently misled. **Action:** Compute real values or remove the fields from the response.

---

## MEDIUM (should fix)

| # | Finding | Location |
|---|---------|----------|
| M1 | OLS DOF guard uses `w.sum()` not `x.nrows()` for WLS -- rejects valid regressions with probability weights | `ols.rs:95-97` |
| M2 | OLS variance denominator ambiguous for weight types -- wrong SEs for non-frequency weights | `ols.rs:79-80,129` |
| M3 | Logit returns stale predicted probabilities (one Newton step behind final beta) | `logit.rs:102-106` |
| M4 | Duplicate logistic regression implementations diverged (logit.rs vs matching/logistic.rs) | `math/logit.rs`, `matching/logistic.rs` |
| M5 | AKM normalisation inside convergence loop causes spurious slow convergence | `akm.rs:558-566` |
| M6 | Builder API inconsistency: `OaxacaBuilder` uses `&mut Self`, `AkmBuilder` uses consuming `Self` | `lib.rs:482`, `akm.rs:70` |
| M7 | Probit non-convergence result used silently (no caller checks `converged` field) | `probit.rs:139-145` |
| M8 | RwLock `.unwrap()` causes cascading server crash if any thread panics | `meridian-mcp/main.rs:279,298,391` |
| M9 | CORS allows any origin + any header | `meridian-mcp/main.rs:244-248` |
| M10 | Negative OLS weights not validated -- NaN poisons entire computation | `ols.rs:60` |
| M11 | Out-of-bounds adjustment indices silently ignored in verification | `engine/analysis.rs:67-73` |
| M12 | Column name collision possible with internal sentinels (`group_indicator`, `intercept`) | `lib.rs:900`, `meridian-mcp/main.rs:79-89` |

---

## LOW (cleanup)

| # | Finding | Location |
|---|---------|----------|
| L1 | Probit dead code: `_delta` computed but unused | `probit.rs:108` |
| L2 | `vec_to_dvec` is just `.clone()` | `lib.rs:336` |
| L3 | `DistanceMetric::distance()` defined but never called -- leaky abstraction | `matching/distance.rs` |
| L4 | Orphaned files: `types_new_fragment.rs`, `check_features.rs` | `engine/src/` |
| L5 | Session map grows without bound (no TTL) | `meridian-mcp/main.rs:278-287` |
| L6 | Bootstrap DataFrame cloned inside par_iter (group split should be hoisted) | `lib.rs:1238-1255`, `quantile_decomposition.rs:304-336` |

---

## MCP Protocol Violations

| Issue | Impact |
|-------|--------|
| Missing `ping` handler -- clients get error `-32603` instead of empty result | Well-behaved clients disconnect |
| Unknown method returns `-32603` (Internal Error) instead of `-32601` (Method Not Found) | Clients can't distinguish server crash from missing feature |
| Malformed JSON in stdio produces no response (should return `-32700`) | Client hangs waiting for reply |
| SSE GET endpoint generates session UUID never registered in session map | Legacy SSE flow is non-functional |
| All engine errors use code `-32603` regardless of cause | Callers can't distinguish bad params from server bugs |

---

## Prioritized Action Plan

### Phase 1 -- Safety (blocks production)
1. Remove unsafe pointer cast in `python.rs` (C1)
2. Replace all panicking unwraps on user data: AKM (C2), Heckman (H11), sorts (H10)
3. Fix access control: suspended partner (C4), auth bypass (C3)
4. Fix null imputation -- fail on null outcomes (H1)
5. Fix identity matrix fallback -- return error on singularity (H2)

### Phase 2 -- Correctness (affects results)
6. Fix RIF quantile interpolation (H3)
7. Clamp logit probabilities (H4)
8. Fix Heckman IMR truncation (H5)
9. Fix OLS DOF guard for WLS (M1, M2)
10. Fix division-by-zero in percentages (H12)
11. Fix defensibility zero-valued metrics (H14)
12. Move AKM normalisation outside convergence loop (M5)

### Phase 3 -- Server hardening
13. Add `spawn_blocking` for CPU work (H7)
14. Add HTTP rate limiting (H6)
15. Add body size + bootstrap_reps limits (H8)
16. Fix MCP protocol violations (ping, error codes, parse errors)
17. Restrict CORS, default to localhost (M9, C3)
18. Remove hardcoded secrets (H9)
19. Fix RwLock poisoning (M8), add session TTL (L5), connection limits (H13)

### Phase 4 -- Code quality
20. Consolidate duplicate logistic regression (M4)
21. Unify builder API pattern (M6)
22. Check probit convergence (M7)
23. Remove dead code: `_delta`, `vec_to_dvec`, orphaned files (L1, L2, L4)
24. Hoist bootstrap DataFrame splits (L6)
