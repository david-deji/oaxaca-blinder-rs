# Phase 4.5 — Reconciliation & Gate Scans

> Orchestrator, 2026-07-17. Post-Phase-4, pre-buildability-gate. Balanced tier.

## Spec-codebase reconciliation (build-safety.md)

Every load-bearing anchor the 7 specs build against was verified on disk. Notable findings:

| Claim | Verified | Result |
|---|---|---|
| In-Scope 12 needs new RIF-quantile math | `math/rif.rs:14` `calculate_rif(series, quantile)` | **REUSE, not implement.** Already computes `RIF = Q_τ + (τ − I(y≤Q_τ))/f(Q_τ)` with R Type-7 quantile + Silverman Gaussian KDE (`rif.rs:78-85`). The writer's OI-1/MAJOR risk resolves in the safe direction — In-Scope 12 is wiring, not a new primitive. |
| KDE utility exists | `math/kde.rs:20` `kde(...)` + `:44` `silverman_bandwidth` | Exists (though `calculate_rif` embeds its own density, so kde.rs is optional for this path). |
| Gardeazabal-Ugidos normalization exists | `math/normalization.rs:5` `normalize_categorical_coefficients` | Exists — categoricals route through it as the spec assumes. |
| OLS + QR utilities exist | `math/ols.rs`, `math/quantile_regression.rs` | Both exist — RIF-OLS reuses `ols.rs`. |
| `analysis.rs:201-204` returns empty detail | Confirmed by auditor + writer | The In-Scope 12 API gap is real; the fix populates from new accessors. |
| `quantile_decomposition.rs:267-271` aggregate-only | Confirmed | New detail is additive; aggregates untouched. |

Net: In-Scope 12 (the founder scope expansion) is **materially cheaper** to build than the Charter's own risk note assumed — the statistical primitive already exists and is numerically complete.

## Gate scans

- **[CLARIFY] markers**: none open across all 7 specs + Charter (only negative confirmations present).
- **Drift check (scope)**: every `In-Scope N` reference across the 7 specs traces to a Charter item in 1–13. All 13 covered; nothing outside the set. Sole expansion is In-Scope 12 (Scope-Delta logged 2026-07-17, founder-approved). **No drift.**
- **SaaS-creep scan** (against `ai-native-architecture.md` 7 signals): **clean, zero signals.** The only "cron" is a GitHub Actions scheduled benchmark job; the only Flask touch is a two-line COOP/COEP `after_request` header on an existing scoped hook (event-plumbing, not business-logic routes). This deliverable is a WASM compute optimization of an existing Rust statistical engine + thin worker wiring — the compute IS the AI-native substrate; no web service is being reinvented.

## Open items routed to the buildability gate

1. **Reproducibility strategy** — all 7 specs written for **Strategy B** (single threaded artifact, rayon sequential fallback). Founder ratifies B vs A (SC-02). Strategy-A fallback is fully specified in every affected spec.
2. **H2 — memory margins** — `St`, `Marg`, `Marg_init`, `headroom_frac`, `N_target` are band-sourced placeholders; ratify once the memory profile lands (memory-budget O1). The founder's "memory of utmost importance" ruling may want larger headroom than the research-band defaults.
3. **Mandatory adversarial council** — Charter `blast_radius: client-facing` sets a gate-strictness floor that calls for the post-reconciliation adversarial council. Founder declined the two OPTIONAL councils (post-Phase-1, post-Phase-2); this one is the floor-mandated one. Surface at the gate for a founder decision (run / override).
4. **E1/E2/E3 build-preflight** — nightly compile validation, shared-memory link args, nested-worker initThreadPool spawn — assigned to /build Phase 0, not /specify. E3 is the top unknown with a specified main-thread-relay fallback.
5. **Build-readiness audit** — dispatched (Sonnet, 7 specs); verdict feeds the gate.
