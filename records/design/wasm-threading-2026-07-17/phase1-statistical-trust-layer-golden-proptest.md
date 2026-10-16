# Phase 1 — Trust Layer: Golden Files vs R/Stata, Property Tests, QR Validation

> Unit: u9 | Domain: statistical-trust-layer | Date: 2026-07-17 | Method: perplexity_ask (medium context)

## Executive Summary

R `oaxaca` (Hlavac) and Stata `oaxaca` (Jann 2008) remain the reference implementations — Hlavac
explicitly matches Jann's estimation procedures, giving one canonical numeric target. Golden-file
surface: group means, total gap, two-fold + three-fold components, per-variable detailed
contributions, delta-method/bootstrap SEs. Tolerances: relative 1e-6..1e-8 on point estimates,
looser (1e-5 or 1-2%) on SEs; compare bootstrap via pre-computed resample indices or
distributional summaries, never raw RNG streams across implementations. Property-based testing
(proptest): adding-up identities + detailed-vs-aggregate consistency + label-swap antisymmetry +
scale equivariance, with generator guards against collinearity/tiny-groups/constant columns. QR
validation: R quantreg `rq()` (Koenker) as reference + location-scale synthetic designs where
true beta(tau) is known analytically — directly fixes the tau-insensitive current tests.

## Findings

1. [CONSENSUS] Reference targets: R `oaxaca` 0.1.5 (CRAN, Hlavac) states it uses the same estimation procedures as Stata `oaxaca` (Jann 2008). Golden comparison set: gap, two-fold (explained/unexplained by reference-coefficient choice), three-fold (endowments/coefficients/interaction), detailed per-variable rows, SEs. (cran oaxaca pdf + vignette; PMC8343972 review)
2. [CONSENSUS] Tolerance practice: relative 1e-6..1e-8 point estimates; absolute floor ~1e-8 for near-zero components; SEs 1e-5 relative or 1-2% when bootstrap-derived; compare underlying statistics not p-values. (established practice synthesis — no single canonical standard exists)
3. [CONSENSUS] Cross-implementation bootstrap comparison: (a) preferred — pre-computed resample-index matrix fed to both implementations (exact comparison); (b) fallback — distributional summaries within 2-3 Monte-Carlo SEs. Aligns with u5's owned-index-resampling design: the same index matrix drives R and Rust. (synthesis)
4. [CONSENSUS] proptest properties: explained+unexplained == gap; endowments+coefficients+interaction == gap; sum(detailed) == aggregate (each side); group-label swap → sign-flipped gap; y-scaling by c scales components by c. Generator guards: prop_assume group sizes ≥10, column variance > eps, bounded ranges, condition-number check for X'X. Two strategies: well-behaved (identity assertions) + stress (error-handling assertions). (general proptest practice)
5. [CONSENSUS] QR validation: location-scale design Y = b0 + b1·X + (1+c·X)·U, U~N(0,1) → true slope at tau = b1 + c·z_tau (analytically known, tau-VARYING) — the exact antidote to current linear-only tests; grid tau ∈ {0.1,0.25,0.5,0.75,0.9}; also golden-file vs quantreg::rq() on the same data. (Koenker quantreg as de-facto reference; construction standard)
6. [REPORTED] Machado-Mata: no official quantreg function for MM decomposition; reference outputs built from rq() coefficient grids + counterfactual construction, or an existing R/Stata MM routine on a shared dataset with shared resample indices. Phase 3: identify the concrete R routine (e.g. Chernozhukov-Fernandez-Val-Melly `Counterfactual` package?) for the golden generator. 
7. [SINGLE_SOURCE] Existing repo asset: `verification/gen_parity_golden.py` + `tests/parity_test.rs` already implement a golden pattern (Python statsmodels-based?) — Phase 3 must read them and decide extend-vs-replace before speccing new harness. (session recon; contents unread)

## Spec Implications

- Golden pipeline: one R script (oaxaca + quantreg, pinned versions, fixed seed/index matrix) generating JSON goldens from Employers_data.csv (10k) + synthetic designs → committed under verification/; Rust test compares within tolerance table.
- Employers_data.csv columns map cleanly: outcome=Salary (log?), group=Gender, predictors=Age, Experience_Years, Education_Level (cat), Department (cat), Location (cat), Job_Title (cat, high-card — maybe excluded). Spec defines the canonical model formula so R and Rust fit identically.
- The trust suite is threading-independent (runs native) — it validates the METHODS; the threading parity suite (u5/SC-03) validates the MODES. Keep them distinct in the spec.

## Sources

- https://cran.r-project.org/web/packages/oaxaca/oaxaca.pdf
- https://cran.gedik.edu.tr/web/packages/oaxaca/vignettes/oaxaca.pdf
- https://www.rdocumentation.org/packages/oaxaca/versions/0.1.5/topics/oaxaca
- https://pmc.ncbi.nlm.nih.gov/articles/PMC8343972/
- https://www.stata-journal.com/software/sj8-4/st0152/nldecompose.hlp

## Research Inventory

- Perplexity ask 2026-07-17 (citations above)
- Session recon 2026-07-17: verification/ dir listing, tests/parity_test.rs existence, ground-truth tests added this session (tests/ground_truth_verification_test.rs)
- /home/deji/Downloads/Employers_data.csv header inspection 2026-07-17
