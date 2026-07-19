#!/usr/bin/env Rscript
# =============================================================================
# gen_trust_goldens.R  —  Statistical-Trust-Layer golden generator (0014-MERIDIAN)
# =============================================================================
# REGENERATION-ONLY TOOLING (INV-01). NOT run at `cargo test` time — the Rust
# trust tests read the committed JSON offline (no R, no network). Mirrors the
# statsmodels pattern in verification/gen_parity_golden.py with a SECOND, wholly
# independent oracle stack for OLS fit and quantile decomposition (R `lm()` +
# `quantreg`/`ddecompose`). The `oaxaca` package is loaded for version
# provenance / parity context only — it is NOT invoked as the Oaxaca-Blinder
# arithmetic oracle; that role is filled by `ddecompose::ob_decompose()`
# (Section 5, AC-6). Section 2's OB arithmetic is transliterated in-script from
# builder.rs/decomposition.rs, validated only via R's independently-fit lm() OLS.
#
# Two independent oracles agreeing (lm()-fit/ddecompose-arithmetic + statsmodels) is
# strictly stronger than one.
#
# Defensibility (W9, primary-source): R's bootstrap reproducibility is fully
# determined by set.seed()+RNGkind() fixing .Random.seed (R `boot`/base docs);
# Stata's [R] set seed fixes the global RNG before [R] bootstrap. TM's seeded
# ChaCha8 bootstrap matches that guarantee AND improves on it — bit-identical
# across thread counts, which R/Stata parallel bootstrap do NOT guarantee across
# ncpus changes.
#
# Requires (regeneration time only): R>=4.1, oaxaca==0.1.5, quantreg, ddecompose,
# rifreg. If a package is absent the corresponding golden block is skipped and
# recorded absent in _meta.packages — the Rust side treats an absent block as
# "pending regeneration on an R-equipped machine" (OI-2), never as a pass.
#
# Writes (all committed):
#   oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv   (PII-stripped 10k)
#   oaxaca_blinder/tests/fixtures/resample_indices.csv          (bootstrap-SE subset indices)
#   oaxaca_blinder/tests/fixtures/trust_goldens_r.json          (all reference values)
# =============================================================================

suppressWarnings(suppressMessages({
  # Loaded for version provenance (_meta.oaxaca_version) and parity context only;
  # NOT used to compute the OB decomposition below (see Section 2 note) —
  # ddecompose::ob_decompose() (Section 5) is the independent OB-arithmetic oracle.
  library(oaxaca)
  has_quantreg   <- requireNamespace("quantreg",   quietly = TRUE)
  has_ddecompose <- requireNamespace("ddecompose", quietly = TRUE)
  library(jsonlite)
}))

# --- provenance / determinism -------------------------------------------------
SEED <- 20260717L
RNGkind("Mersenne-Twister", "Inversion", "Rejection")
set.seed(SEED)

REPO_ROOT <- normalizePath(file.path(dirname(sub("--file=", "",
  grep("--file=", commandArgs(FALSE), value = TRUE)[1])), ".."), mustWork = FALSE)
if (is.na(REPO_ROOT) || REPO_ROOT == "") REPO_ROOT <- normalizePath("..", mustWork = FALSE)

RAW_CSV  <- "/home/deji/Downloads/Employers_data.csv"
FIX_DIR  <- file.path(REPO_ROOT, "oaxaca_blinder/tests/fixtures")
FIXTURE  <- file.path(FIX_DIR, "employers_trust_fixture.csv")
INDICES  <- file.path(FIX_DIR, "resample_indices.csv")
GOLDEN   <- file.path(FIX_DIR, "trust_goldens_r.json")

OUTCOME      <- "log_salary"              # log(Salary), precomputed into the fixture
GROUP        <- "Gender"
NUM_PREDS    <- c("Age", "Experience_Years")
CAT_PREDS    <- c("Education_Level", "Department", "Location")
EXCLUDED     <- c("Job_Title")            # high-cardinality, near-collinear w/ Department
KEEP_COLS    <- c("Age", "Gender", "Department", "Job_Title",
                  "Experience_Years", "Education_Level", "Location", "Salary")

# trimws is load-bearing: formatC right-pads a numeric vector to a common width with leading
# spaces; R's read.csv strips it but polars (the engine's CSV reader) infers a padded column
# as `str`, not Float64 → SchemaMismatch at design-matrix build. trimws guarantees clean numerics.
g17 <- function(x) if (is.numeric(x)) trimws(formatC(x, digits = 17, format = "g")) else as.character(x)

# =============================================================================
# 1. PII strip + fixture (In-Scope 13; AC-2)
# =============================================================================
raw <- read.csv(RAW_CSV, stringsAsFactors = FALSE, check.names = FALSE)
stopifnot("Salary must be strictly positive for log()" = all(raw$Salary > 0))
fx <- raw[, KEEP_COLS, drop = FALSE]                       # drops Employee_ID, Name
fx$log_salary <- log(fx$Salary)

# write with full f64 precision so the Rust engine reads byte-identical inputs
fx_out <- fx
num_cols <- vapply(fx_out, is.numeric, logical(1))
for (nm in names(fx_out)[num_cols]) fx_out[[nm]] <- g17(fx_out[[nm]])
dir.create(FIX_DIR, recursive = TRUE, showWarnings = FALSE)
# NO comment/header preamble — the engine (polars LazyCsvReader) would parse a leading
# `#` line as the header row, and AC-2's `head -1` check must see only column names.
# PII-strip provenance lives in _meta.pii_stripped instead.
suppressWarnings(write.table(fx_out, FIXTURE, sep = ",", row.names = FALSE, quote = FALSE))

# round-trip: oracle sees exactly the committed bytes
fxr <- read.csv(FIXTURE, stringsAsFactors = FALSE)
for (c in CAT_PREDS) fxr[[c]] <- factor(fxr[[c]])          # alpha-sorted levels (R default)
fxr[[GROUP]] <- factor(fxr[[GROUP]])

GENDER_REF <- levels(fxr[[GROUP]])[1]                      # alphabetical-first == engine ascending-sort base
GENDER_A   <- setdiff(levels(fxr[[GROUP]]), GENDER_REF)[1] # non-reference (engine Group A)
stopifnot("Gender must be binary for the two-group decomposition" =
          nlevels(fxr[[GROUP]]) == 2)

# engine-style design column names: {col}_{level} for cats, numeric as-is, intercept __ob_intercept__
engine_name <- function(term) {
  if (term == "(Intercept)") return("__ob_intercept__")
  for (c in CAT_PREDS) if (startsWith(term, c)) {
    lvl <- sub(paste0("^", c), "", term)
    return(paste0(c, "_", lvl))
  }
  term
}

# =============================================================================
# 2. lm()-based OB GroupB golden — point, per-variable, three-fold (AC-3)
#    FIT-ORACLE, NOT ARITHMETIC-ORACLE: R's lm() independently validates the OLS
#    fit (R lm() vs the engine's nalgebra OLS). The OB decomposition arithmetic
#    below is transliterated in-script from builder.rs/decomposition.rs:102/113
#    (beta* = beta_B) — it is NOT an independent implementation of the OB
#    arithmetic. The independent OB-arithmetic oracle is ddecompose::ob_decompose()
#    in Section 5 (AC-6), which has its own C/R implementation of the decomposition.
# =============================================================================
model_rhs <- paste(c(NUM_PREDS, CAT_PREDS), collapse = " + ")
fml <- as.formula(paste(OUTCOME, "~", model_rhs))

dA <- droplevels(fxr[fxr[[GROUP]] == GENDER_A,   ])
dB <- droplevels(fxr[fxr[[GROUP]] == GENDER_REF, ])

fitA <- lm(fml, data = dA)
fitB <- lm(fml, data = dB)

# design-matrix means (same terms/order for both — build on the full frame's contrasts)
mmA <- model.matrix(fml, data = dA)
mmB <- model.matrix(fml, data = dB)
stopifnot(identical(colnames(mmA), colnames(mmB)))
terms_raw   <- colnames(mmA)
terms_eng   <- vapply(terms_raw, engine_name, character(1))
xbarA <- colMeans(mmA); xbarB <- colMeans(mmB)
bA <- coef(fitA)[terms_raw]; bB <- coef(fitB)[terms_raw]
bstar <- bB                                              # GroupB scheme: beta* = beta_B

# per-variable two-fold (engine formula)
explained   <- (xbarA - xbarB) * bstar
unexplained <- xbarA * (bA - bstar) + xbarB * (bstar - bB)
# three-fold (endowments/coefficients/interaction)
endowments   <- (xbarA - xbarB) * bB
coefficients <- xbarB * (bA - bB)
interaction  <- (xbarA - xbarB) * (bA - bB)

names(explained) <- names(unexplained) <- terms_eng
names(endowments) <- names(coefficients) <- names(interaction) <- terms_eng

ybarA <- mean(dA[[OUTCOME]]); ybarB <- mean(dB[[OUTCOME]])
total_gap <- ybarA - ybarB
agg_explained   <- sum(explained);   agg_unexplained <- sum(unexplained)
stopifnot("internal: explained+unexplained==gap" =
          abs((agg_explained + agg_unexplained) - total_gap) < 1e-9)

named_list <- function(v) as.list(v)

groupb <- list(
  y_mean_A = ybarA, y_mean_B = ybarB, total_gap = total_gap,
  n_a = nrow(dA), n_b = nrow(dB),
  aggregate = list(explained = agg_explained, unexplained = agg_unexplained),
  detailed_explained   = named_list(explained),
  detailed_unexplained = named_list(unexplained),
  three_fold = list(
    aggregate = list(endowments = sum(endowments),
                     coefficients = sum(coefficients),
                     interaction = sum(interaction)),
    detailed_endowments   = named_list(endowments),
    detailed_coefficients = named_list(coefficients),
    detailed_interaction  = named_list(interaction)
  )
)

# =============================================================================
# 3. Bootstrap-SE golden (D-3, W5) — manual loop over a committed index matrix,
#    on a deterministic 1500-row subset (keeps the index file < ~1 MB). The Rust
#    side reads the SAME indices and computes replicate estimates via the engine's
#    single-pass decompose, so SEs compare EXACTLY (same resamples), rel 1e-3.
# =============================================================================
# Kept small deliberately: this is an EXACT-index R-vs-Rust cross-check (not a statistical SE
# estimate), so 60 reps on an 800-row subset suffice and keep resample_indices.csv < 500 KB
# (repo-management: no 1-5 MB test fixtures). The consuming Rust bootstrap-SE test is a
# documented TODO — the golden + indices are ready for it.
SUBSET_N   <- min(800L, nrow(fxr))
REPS_SE    <- 60L
sub_idx    <- sort(sample.int(nrow(fxr), SUBSET_N))
sub        <- droplevels(fxr[sub_idx, ])
subA_rows  <- which(sub[[GROUP]] == GENDER_A)
subB_rows  <- which(sub[[GROUP]] == GENDER_REF)
nA <- length(subA_rows); nB <- length(subB_rows)

# per-rep within-group 0-based indices into the subset's A-rows / B-rows
idxA <- matrix(0L, REPS_SE, nA); idxB <- matrix(0L, REPS_SE, nB)
for (r in seq_len(REPS_SE)) {
  idxA[r, ] <- sample.int(nA, nA, replace = TRUE) - 1L
  idxB[r, ] <- sample.int(nB, nB, replace = TRUE) - 1L
}

ob_estimate <- function(dfA, dfB) {
  fA <- lm(fml, data = dfA); fB <- lm(fml, data = dfB)
  mA <- model.matrix(fml, data = dfA); mB <- model.matrix(fml, data = dfB)
  tr <- colnames(mA)
  cA <- coef(fA)[tr]; cB <- coef(fB)[tr]
  xa <- colMeans(mA); xb <- colMeans(mB)
  ex <- sum((xa - xb) * cB); un <- sum(xa * (cA - cB))     # cB = beta*, so unexplained collapses
  gap <- mean(dfA[[OUTCOME]]) - mean(dfB[[OUTCOME]])
  c(explained = ex, unexplained = gap - ex, total_gap = gap)
}
boot <- matrix(NA_real_, REPS_SE, 3,
               dimnames = list(NULL, c("explained", "unexplained", "total_gap")))
subA <- droplevels(sub[subA_rows, ]); subB <- droplevels(sub[subB_rows, ])
for (r in seq_len(REPS_SE)) {
  ra <- droplevels(subA[idxA[r, ] + 1L, ]); rb <- droplevels(subB[idxB[r, ] + 1L, ])
  boot[r, ] <- tryCatch(ob_estimate(ra, rb), error = function(e) rep(NA_real_, 3))
}
boot <- boot[stats::complete.cases(boot), , drop = FALSE]
se_golden <- apply(boot, 2, sd)

# commit the index matrix: reps rows, [nA A-indices | nB B-indices], 0-based
idx_df <- as.data.frame(cbind(idxA, idxB))
colnames(idx_df) <- c(paste0("a", seq_len(nA)), paste0("b", seq_len(nB)))
write.csv(idx_df, INDICES, row.names = FALSE, quote = FALSE)

bootstrap <- list(
  subset_n = SUBSET_N, reps = REPS_SE, n_a = nA, n_b = nB,
  subset_seed = SEED, index_file = "resample_indices.csv",
  # 0-based indices into employers_trust_fixture.csv (the FULL 10k-row fixture) identifying
  # which SUBSET_N rows form `sub`, in ascending order — required for the Rust side to
  # reconstruct subA/subB before applying resample_indices.csv. Without this, R's exact
  # sample.int() draw (line ~187) cannot be reproduced in Rust (no equivalent
  # Mersenne-Twister sampler exists in the engine), so the bootstrap-SE consuming test
  # (tests/bootstrap_se_golden_test.rs) stays BLOCKED until this golden is regenerated.
  sub_idx_0based = as.integer(sub_idx - 1L),
  se_explained = unname(se_golden["explained"]),
  se_unexplained = unname(se_golden["unexplained"]),
  se_total_gap = unname(se_golden["total_gap"]),
  note = "SEs = sd across replicate estimates on committed within-group indices; compare rel 1e-3"
)

# =============================================================================
# 4. QR location-scale golden (D-5) — tau-varying analytic slopes + quantreg::rq
#    Y = b0 + b1*X + (1 + c*X)*U ; U~N(0,1); true beta1(tau) = b1 + c*z_tau.
# =============================================================================
qr_block <- list(available = FALSE)
if (has_quantreg) {
  suppressMessages(library(quantreg))
  set.seed(SEED + 7L)
  nQ <- 8000L; b0 <- 2.0; b1 <- 0.5; cc <- 0.3
  X  <- runif(nQ, 1, 6)                     # X>0 so (1+c*X)>0
  U  <- rnorm(nQ)
  Yq <- b0 + b1 * X + (1 + cc * X) * U
  qr_fixture <- data.frame(X = X, Y = Yq)
  taus <- c(0.10, 0.25, 0.50, 0.75, 0.90)
  rq_slope <- sapply(taus, function(t) coef(rq(Y ~ X, tau = t, data = qr_fixture))[["X"]])
  true_slope <- b1 + cc * qnorm(taus)
  qr_block <- list(
    available = TRUE, b0 = b0, b1 = b1, c = cc, n = nQ, seed = SEED + 7L,
    taus = taus,
    # committed synthetic sample so the Rust QR runs on identical inputs
    X = X, Y = Yq,
    rq_slope   = as.list(setNames(rq_slope,   paste0("tau_", taus))),
    true_slope = as.list(setNames(true_slope, paste0("tau_", taus))),
    note = "Rust solve_qr vs rq() rel 1e-4; vs analytic beta1(tau) abs ~0.05; discrimination beta1(.9)-beta1(.1) >= c*(z.9-z.1)*0.8"
  )
}

# =============================================================================
# 5. ddecompose per-predictor quantile golden (D-6 track 1; AC-6)
#    reweighting=FALSE => one-stage RIF-OLS (FFL-2009), matching the engine route.
#    normalize_factors=FALSE => treatment contrasts w/ alpha-first base (engine default).
#    Tolerance is MEASURED-then-pinned by the Rust test (MJ-3), not asserted 1e-4 on faith.
# =============================================================================
qd_block <- list(available = FALSE)
if (has_ddecompose) {
  suppressMessages(library(ddecompose))
  probs <- c(0.10, 0.50, 0.90)
  BOOT_Q <- 200L
  ddec <- tryCatch(
    ddecompose::ob_decompose(fml, data = fxr, group = fxr[[GROUP]],
      reweighting = FALSE, normalize_factors = FALSE,
      rifreg_statistic = "quantiles", rifreg_probs = probs,
      bootstrap = TRUE, bootstrap_iterations = BOOT_Q),
    error = function(e) { message("ddecompose ERR: ", conditionMessage(e)); NULL })
  if (!is.null(ddec)) {
    per_tau <- list()
    for (p in probs) {
      qn <- paste0("quantile_", p)
      dtt <- ddec[[qn]]$decomposition_terms
      rows <- dtt[dtt$Variable != "Total", , drop = FALSE]
      comp <- setNames(as.list(rows$Composition_effect), vapply(rows$Variable, engine_name, character(1)))
      strc <- setNames(as.list(rows$Structure_effect),   vapply(rows$Variable, engine_name, character(1)))
      tot  <- dtt[dtt$Variable == "Total", ]
      per_tau[[qn]] <- list(
        aggregate_composition = tot$Composition_effect,
        aggregate_structure   = tot$Structure_effect,
        observed_difference   = tot$Observed_difference,
        detailed_composition  = comp,   # == engine explained (characteristics)
        detailed_structure    = strc    # == engine unexplained (coefficients)
      )
    }
    qd_block <- list(
      available = TRUE, probs = probs, reweighting = FALSE,
      normalize_factors = FALSE, bootstrap_iterations = BOOT_Q,
      group_reference = ddec$reference_group,
      direction_note = paste("ddecompose Composition_effect==engine explained;",
        "Structure_effect==engine unexplained. Sign/direction aligned to the engine's",
        "mean(A)-mean(B) in the Rust test (group0 =", GENDER_REF, "= engine ref B)."),
      per_tau = per_tau,
      tolerance_note = "MEASURED-then-pinned (MJ-3): record engine-vs-ddecompose agreement on this fixture, pin to it (~1e-3..1e-2). Do NOT assert 1e-4 on faith."
    )
  }
}

# =============================================================================
# 6. Assemble + write JSON (full f64 precision)
# =============================================================================
golden <- list(
  `_meta` = list(
    generated_utc = format(Sys.time(), tz = "UTC", usetz = TRUE),
    r_version = as.character(getRversion()),
    oaxaca_version = as.character(packageVersion("oaxaca")),
    quantreg_version = if (has_quantreg) as.character(packageVersion("quantreg")) else "MISSING",
    ddecompose_version = if (has_ddecompose) as.character(packageVersion("ddecompose")) else "MISSING",
    seed = SEED, rngkind = paste(RNGkind(), collapse = ","),
    fixture = "employers_trust_fixture.csv", n_rows = nrow(fxr),
    pii_stripped = "Direct identifiers Employee_ID + Name removed at generation; aggregate goldens only.",
    group_col = GROUP, group_reference = GENDER_REF, group_a = GENDER_A,
    outcome = OUTCOME, num_predictors = NUM_PREDS, cat_predictors = CAT_PREDS,
    excluded_predictors = list(Job_Title = "high-cardinality, near-collinear with Department"),
    design_columns = unname(terms_eng),
    education_encoding = paste(levels(fxr$Education_Level), collapse = ","),
    reference_coefficients = "GroupB (beta*=beta_B); == builder.rs default",
    packages = list(oaxaca = TRUE, quantreg = has_quantreg, ddecompose = has_ddecompose),
    tolerances = list(point = 1e-6, internal = 1e-9, bootstrap_se = 1e-3,
                      qr_rq = 1e-4, quantile_detail = "measured-then-pinned"),
    defensibility = paste("R boot/set.seed reproducibility (W9); TM seeded ChaCha8",
                          "matches + improves (bit-identical across thread counts).")
  ),
  groupb = groupb,
  bootstrap = bootstrap,
  qr_location_scale = qr_block,
  quantile_detail = qd_block
)

writeLines(jsonlite::toJSON(golden, auto_unbox = TRUE, digits = 17,
                            pretty = TRUE, null = "null", na = "null"), GOLDEN)

cat("Wrote", FIXTURE, "\n")
cat("Wrote", INDICES, "(", REPS_SE, "reps x", nA + nB, "cols )\n")
cat("Wrote", GOLDEN, "\n")
cat(sprintf("  total_gap=%.10f  explained=%.10f  unexplained=%.10f\n",
            total_gap, agg_explained, agg_unexplained))
cat(sprintf("  bootstrap se(explained)=%.6f  QR=%s  ddecompose=%s\n",
            bootstrap$se_explained, qr_block$available, qd_block$available))
