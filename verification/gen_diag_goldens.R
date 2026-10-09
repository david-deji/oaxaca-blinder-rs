#!/usr/bin/env Rscript
# =============================================================================
# gen_diag_goldens.R  --  oracle for support, interval, percentile and weight diagnostics
#                         (0120-MERIDIAN S6 / S7 / S8 / S9; V6, V7, V8, V9)
# =============================================================================
# REGENERATION-ONLY TOOLING. `cargo test` never runs R: the Rust tests read the committed
# fixtures and `diag_goldens_r.json` offline and REFUSE a golden whose recorded sha256 of this
# script or of any fixture no longer matches the files on disk.
#
# NO EXPECTED VALUE IN THE GOLDEN COMES FROM ENGINE OUTPUT. Every number is computed here by
# base R (`lm`, `predict.lm`, `quantile(type = 7)`, `hatvalues`, `pt`), `ddecompose` and `Hmisc`.
#
#   R_LIBS_USER=/path/to/lib Rscript verification/gen_diag_goldens.R
#
# Requires R, ddecompose 1.0.0, Hmisc, jsonlite, digest. Writes two synthetic fixtures
# (diag_df5.csv, diag_grid.csv; seeded, byte-identical on a rerun) and diag_goldens_r.json. The
# four static fixtures diag_nooverlap / diag_linear_nooverlap / diag_kink_overlap / diag_tiny
# were produced by the 2026-10-09 re-ground (ground/2026-10-09-C-reground.md section 0) and are
# inputs here, hashed in the golden.
#
# NOT USED: mem_profile_50k.csv. It is git-ignored (the memory-ceiling job generates it), so a golden
# built on it could not be checked for staleness on a fresh clone. The 10 000-row employers fixture
# has the same shape.
#
# WHAT IS CHECKED, AND AGAINST WHAT
#  V6  support: per continuous predictor the baseline [min, max] and type-7 [p1, p99], the share
#      of target rows outside each, the Imbens-Rubin normalised difference (asserted here to be
#      exactly sqrt(2) times `ddecompose:::get_normalized_difference`, which omits the 1/2 in the
#      pooled variance), leverage from `hatvalues` and (X'X)^-1, and the count of target rows
#      whose leverage exceeds the baseline maximum.
#  V7  intervals: `predict.lm(interval = "prediction", level)` for the baseline regression at
#      three levels on a 10 000-row fixture and on a 5-residual-df fixture (t = 2.571 against
#      z = 1.960 there, so a z implementation cannot pass); the pooled-regression group test
#      (t and p by `pt`) and a grid of `2 * pt(-|t|, df)` values.
#  V8  percentiles: `quantile(type = 7)` per group, the empirical CDF at it, and the exact count
#      of rows tied at it, on the employers fixture and on a 4-step pay grid.
#  V9  weights: `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)` on four weight patterns,
#      and integer weights against `quantile(rep(y, w), type = 7)`.
# =============================================================================

user_lib <- Sys.getenv("R_LIBS_USER")
if (nzchar(user_lib)) .libPaths(c(user_lib, .libPaths()))
invisible(Sys.setlocale("LC_COLLATE", "C"))
suppressWarnings(suppressMessages({ library(ddecompose); library(Hmisc); library(jsonlite); library(digest) }))
options(digits = 15, width = 200)

this_file <- sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE)[1])
REPO_ROOT <- normalizePath(file.path(dirname(this_file), ".."), mustWork = TRUE)
FIX <- file.path(REPO_ROOT, "oaxaca_blinder/tests/fixtures")
GOLDEN <- file.path(FIX, "diag_goldens_r.json")
sha <- function(path) digest(file = path, algo = "sha256")
SEED <- 20261009L
RNGkind("Mersenne-Twister", "Inversion", "Rejection")

# ---------------------------------------------------------------- synthetic fixtures
set.seed(SEED)
# diag_df5: baseline group of 8 rows and 3 model columns -> 5 residual degrees of freedom.
ref <- data.frame(grp = "Ref", x1 = round(runif(8, 10, 20), 2), x2 = round(runif(8, 0, 10), 2))
ref$pay <- round(40000 + 1500 * ref$x1 + 800 * ref$x2 + rnorm(8, 0, 2000), 2)
tgt <- data.frame(grp = "Cmp", x1 = round(c(runif(5, 11, 19), 27), 2), x2 = round(c(runif(5, 1, 9), 14), 2))
tgt$pay <- round(38000 + 1500 * tgt$x1 + 800 * tgt$x2 + rnorm(6, 0, 2000), 2)
df5 <- rbind(ref, tgt)[, c("pay", "grp", "x1", "x2")]
write.table(df5, file.path(FIX, "diag_df5.csv"), sep = ",", row.names = FALSE, quote = FALSE)
# diag_grid: a continuous group against a group paid on a 4-step grid (30 rows each).
grid_pay <- rep(c(24, 26, 28, 30), c(6, 12, 9, 3))
cont_pay <- sort(round(runif(30, 23, 31), 4))
gr <- rbind(data.frame(pay = cont_pay, grp = "Cont", x = round(runif(30, 0, 10), 3)),
            data.frame(pay = grid_pay, grp = "Grid", x = round(runif(30, 0, 10), 3)))
write.table(gr, file.path(FIX, "diag_grid.csv"), sep = ",", row.names = FALSE, quote = FALSE)

FIXTURES <- c("employers_trust_fixture.csv", "parity_fixture.csv", "diag_nooverlap.csv",
              "diag_linear_nooverlap.csv", "diag_kink_overlap.csv", "diag_tiny.csv", "diag_df5.csv", "diag_grid.csv")
path_of <- function(f) file.path(FIX, f)
fixture_hash <- setNames(lapply(FIXTURES, function(f) sha(path_of(f))), FIXTURES)
extra_hash <- list(`0118-fixture-f.csv` = sha(file.path(REPO_ROOT, "engine/tests/fixtures/0118-fixture-f.csv")),
                   `wage.csv` = sha(file.path(REPO_ROOT, "oaxaca_blinder/tests/data/wage.csv")))

# ---------------------------------------------------------------- V6: support
hand_type7 <- function(x, p) as.numeric(quantile(x, p, type = 7, names = FALSE))
support_case <- function(csv, outcome, group, preds, ref_group = NULL) {
  d <- read.csv(csv, stringsAsFactors = FALSE)
  d <- d[complete.cases(d[, c(outcome, group, preds)]), ]
  if (is.null(ref_group)) {                        # the advantaged group is the baseline
    m <- tapply(d[[outcome]], d[[group]], mean); ref_group <- names(m)[which.max(m)]
  }
  is_ref <- d[[group]] == ref_group
  A <- d[is_ref, ]; B <- d[!is_ref, ]
  form <- as.formula(paste("~", paste(preds, collapse = " + ")))
  XA <- model.matrix(form, A); XB <- model.matrix(form, B)
  k <- ncol(XA)
  XtXi <- solve(crossprod(XA))
  hA <- rowSums((XA %*% XtXi) * XA); hB <- rowSums((XB %*% XtXi) * XB)
  fit <- lm(as.formula(paste(outcome, "~", paste(preds, collapse = " + "))), data = A)
  stopifnot(max(abs(hatvalues(fit) - hA)) < 1e-12)
  hmax <- max(hA)
  nd_dd <- ddecompose:::get_normalized_difference(form, d, rep(1, nrow(d)), rep(1, nrow(d)), d[[group]], ref_group)
  out <- list()
  for (p in preds) {
    a <- A[[p]]; b <- B[[p]]
    s <- sqrt((var(a) + var(b)) / 2)
    nd <- if (s > 0) (mean(b) - mean(a)) / s else NULL
    if (!is.null(nd)) stopifnot(abs(nd - nd_dd[p, "Normalized  difference"] * sqrt(2)) < 1e-12)
    p01 <- hand_type7(a, .01); p99 <- hand_type7(a, .99)
    out[[p]] <- list(reference_min = min(a), reference_max = max(a), reference_p01 = p01, reference_p99 = p99,
                     target_min = min(b), target_max = max(b),
                     target_outside_range_share = mean(b < min(a) | b > max(a)),
                     target_outside_p01_p99_share = mean(b < p01 | b > p99),
                     normalised_difference = nd,
                     ddecompose_normalized_difference = nd_dd[p, "Normalized  difference"])
  }
  list(outcome = outcome, group = group, predictors = preds, reference_group = ref_group,
       reference_count = nrow(A), target_count = nrow(B), model_columns = k,
       reference_residual_df = nrow(A) - k, target_residual_df = nrow(B) - k,
       reference_max_leverage = hmax, extrapolated_target_count = sum(hB > hmax * (1 + 1e-9) + 1e-12),
       per_predictor = out)
}
support <- list(
  employers = support_case(path_of("employers_trust_fixture.csv"), "Salary", "Gender", c("Age", "Experience_Years"), "Male"),
  parity = support_case(path_of("parity_fixture.csv"), "log_wage", "gender", c("education", "experience", "tenure"), "M"),
  fixture_f = support_case(file.path(REPO_ROOT, "engine/tests/fixtures/0118-fixture-f.csv"), "Salary", "Gender", "Experience", "Male"),
  wage5 = support_case(file.path(REPO_ROOT, "oaxaca_blinder/tests/data/wage.csv"), "wage", "gender", "education", "M"),
  nooverlap = support_case(path_of("diag_nooverlap.csv"), "wage", "gender", "edu", "M"),
  linear_nooverlap = support_case(path_of("diag_linear_nooverlap.csv"), "wage", "gender", "edu", "M"),
  kink_overlap = support_case(path_of("diag_kink_overlap.csv"), "wage", "gender", "edu", "M"),
  tiny = support_case(path_of("diag_tiny.csv"), "wage", "gender", c("edu", "exp", "age"), "M"),
  df5 = support_case(path_of("diag_df5.csv"), "pay", "grp", c("x1", "x2"), "Ref")
)

# ---------------------------------------------------------------- V7: intervals
interval_case <- function(csv, outcome, group, preds, ref_group, levels, sample_rows) {
  d <- read.csv(csv, stringsAsFactors = FALSE)
  d$.ordinal <- seq_len(nrow(d)) - 1L                       # 0-based data-row ordinal
  d <- d[complete.cases(d[, c(outcome, group, preds)]), ]
  is_ref <- d[[group]] == ref_group
  A <- d[is_ref, ]; B <- d[!is_ref, ]
  fit <- lm(as.formula(paste(outcome, "~", paste(preds, collapse = " + "))), data = A)
  form <- as.formula(paste("~", paste(preds, collapse = " + ")))
  XA <- model.matrix(form, A); XB <- model.matrix(form, B)
  XtXi <- solve(crossprod(XA))
  hB <- rowSums((XB %*% XtXi) * XB)
  pick_b <- if (is.null(sample_rows)) seq_len(nrow(B)) else unique(c(seq_len(min(sample_rows, nrow(B))), order(hB, decreasing = TRUE)[1:3]))
  pick_a <- seq_len(min(if (is.null(sample_rows)) nrow(A) else 10, nrow(A)))
  lv <- list()
  for (cl in levels) {
    rows <- function(dd, idx) {
      pr <- predict(fit, newdata = dd[idx, ], interval = "prediction", level = cl)
      lapply(seq_along(idx), function(i) list(ordinal = dd$.ordinal[idx[i]], fair = pr[i, "fit"], lwr = pr[i, "lwr"], upr = pr[i, "upr"],
                                              wage = dd[[outcome]][idx[i]]))
    }
    lv[[format(cl, nsmall = 2)]] <- list(critical = qt(1 - (1 - cl) / 2, df.residual(fit)), target = rows(B, pick_b), reference = rows(A, pick_a))
  }
  # pooled-regression group test, exactly the frontier's design: intercept, target indicator, features
  d$.is_target <- as.numeric(d[[group]] != ref_group)
  # frontier pooled design lists reference rows first, then target rows; row order does not matter to lm
  pf <- lm(as.formula(paste(outcome, "~ .is_target +", paste(preds, collapse = " + "))), data = d)
  cf <- summary(pf)$coefficients[".is_target", ]
  list(reference_group = ref_group, outcome = outcome, group = group, predictors = preds,
       residual_df = df.residual(fit), reference_rows = nrow(A), target_rows = nrow(B),
       levels = lv,
       pooled_group_test = list(coefficient = unname(cf["Estimate"]), t = unname(cf["t value"]), p = unname(cf["Pr(>|t|)"]),
                                df = df.residual(pf)))
}
intervals <- list(
  employers = interval_case(path_of("employers_trust_fixture.csv"), "Salary", "Gender", c("Age", "Experience_Years"), "Male", c(0.90, 0.95, 0.99), 30),
  df5 = interval_case(path_of("diag_df5.csv"), "pay", "grp", c("x1", "x2"), "Ref", c(0.90, 0.95, 0.99), NULL)
)
t_grid <- list()
for (df in c(3, 5, 12, 30, 4888)) for (t in c(0.25, 1, 1.96, 2.571, 3.5, 8)) {
  t_grid[[length(t_grid) + 1]] <- list(t = t, df = df, p = 2 * pt(-abs(t), df))
}

# ---------------------------------------------------------------- V8: percentiles
q_group <- function(x, tau) {
  q <- hand_type7(x, tau)
  list(count = length(x), quantile_value = q, ecdf_at_quantile = mean(x <= q), tie_count = sum(x == q), tie_share = mean(x == q))
}
quantile_case <- function(csv, outcome, group, ref_group, taus) {
  d <- read.csv(csv, stringsAsFactors = FALSE)
  A <- d[[outcome]][d[[group]] == ref_group]; B <- d[[outcome]][d[[group]] != ref_group]
  out <- list()
  for (tau in taus) {
    r <- q_group(A, tau); t <- q_group(B, tau)
    out[[format(tau, nsmall = 1)]] <- list(reference = r, target = t, quantile_gap = t$quantile_value - r$quantile_value)
  }
  list(outcome = outcome, group = group, reference_group = ref_group, taus = out)
}
quantiles <- list(
  employers = quantile_case(path_of("employers_trust_fixture.csv"), "log_salary", "Gender", "Male", c(0.1, 0.5, 0.9)),
  grid = quantile_case(path_of("diag_grid.csv"), "pay", "grp", "Cont", c(0.5, 0.9))
)

# ---------------------------------------------------------------- V9: weights
hm <- function(y, w, tau) as.numeric(Hmisc::wtd.quantile(y, weights = w, probs = tau, type = "quantile", normwt = TRUE))
set.seed(SEED + 1L)
wcases <- list(
  list(name = "mixed FTE", y = c(40, 42, 44, 46, 48, 60, 62, 64), w = c(1, 1, 1, 1, 1, .5, .5, .5)),
  list(name = "integer 1,3,2,1", y = c(1, 2, 3, 7), w = c(1, 3, 2, 1)),
  list(name = "uniform 0.5", y = c(20, 24, 28, 32), w = rep(.5, 4)),
  list(name = "random", y = round(runif(12, 30, 80), 4), w = round(runif(12, .2, 1), 4)),
  list(name = "ties", y = c(10, 10, 10, 20, 20, 30, 40, 40), w = c(1.5, .5, 1, 2, .25, 3, .75, 1))
)
taus <- c(0.1, 0.25, 0.5, 0.75, 0.9)
weights_block <- lapply(wcases, function(cs) {
  rel <- setNames(lapply(taus, function(t) hm(cs$y, cs$w, t)), format(taus, nsmall = 2))
  c(cs, list(relative = rel))
})
freq_cases <- list(list(name = "integer 1,3,2,1", y = c(1, 2, 3, 7), w = c(1, 3, 2, 1)),
                   list(name = "integer 2,2,5", y = c(15, 18, 31), w = c(2, 2, 5)))
freq_block <- lapply(freq_cases, function(cs) {
  ex <- rep(cs$y, cs$w)
  c(cs, list(frequency = setNames(lapply(taus, function(t) as.numeric(quantile(ex, t, type = 7))), format(taus, nsmall = 2))))
})

golden <- list(
  `_meta` = list(
    generated_utc = format(Sys.time(), tz = "UTC", usetz = TRUE),
    r_version = as.character(getRversion()),
    packages = list(ddecompose = as.character(packageVersion("ddecompose")), Hmisc = as.character(packageVersion("Hmisc")),
                    jsonlite = as.character(packageVersion("jsonlite")), digest = as.character(packageVersion("digest"))),
    seed = SEED, rngkind = paste(RNGkind(), collapse = ","),
    generator_sha256 = sha(this_file),
    fixture_sha256 = c(fixture_hash, extra_hash),
    tolerances = list(support_numbers = 1e-12, intervals = 1e-9, percentiles = 1e-12, weights = 1e-9),
    normalised_difference = "Imbens-Rubin: (mean_target - mean_reference) / sqrt((var_target + var_reference) / 2); ddecompose prints this divided by sqrt(2)",
    expected_values_from_engine_output = FALSE
  ),
  support = support, intervals = intervals, t_grid = t_grid, quantiles = quantiles,
  weights = list(relative = weights_block, frequency = freq_block)
)
writeLines(toJSON(golden, auto_unbox = TRUE, digits = I(17), pretty = TRUE, null = "null", na = "null"), GOLDEN)
cat("wrote", GOLDEN, "\n")
