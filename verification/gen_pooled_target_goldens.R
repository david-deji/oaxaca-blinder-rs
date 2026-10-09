#!/usr/bin/env Rscript
# =============================================================================
# gen_pooled_target_goldens.R  --  oracle for the optimiser's Pooled target (0120-MERIDIAN T8)
# =============================================================================
# REGENERATION-ONLY TOOLING. `cargo test` never runs R: engine/tests/pooled_target_test.rs reads
# the committed `pooled_target_goldens_r.json` offline and REFUSES a golden whose recorded sha256
# of this script or of any fixture no longer matches the files on disk.
#
# NO EXPECTED VALUE IN THE GOLDEN COMES FROM ENGINE OUTPUT. Every number is computed here by base
# R (`lm`, `predict.lm`, `hatvalues`, `model.matrix`, `qt`) and cross-checked against the R
# `oaxaca` package's pooled-with-indicator weight (-2).
#
#   R_LIBS_USER=/home/deji/R/library Rscript verification/gen_pooled_target_goldens.R
#
# THE FIT. The Pooled optimise target fits ONE regression on both groups with a target-group
# indicator, `y ~ x + group` (group = 0 reference, 1 target), drops the indicator's coefficient,
# and reads every row's fair wage off the remaining line at indicator 0:
#     fair  = predict.lm(fit, newdata = row with group = 0)
#     bound = predict.lm(fit, newdata = row with group = 0, interval = "prediction", level)
# so sigma^2, (Z'Z)^-1 and the residual df (n - k - 1) are the pooled fit's. Because the indicator
# is in the design, the OLS residuals sum to zero inside each group, so the target group's mean
# shortfall to this line is exactly the indicator's coefficient `gamma`: the decomposition's
# `Pooled` unexplained gap (Elder et al. 2010, Jann 2008 `pooled`). Asserted below and against
# `oaxaca::oaxaca` weight -2.
#
# EXTRAPOLATED. A row is extrapolated when its leverage at indicator 0,
# h = (x, 0)' (Z'Z)^-1 (x, 0), exceeds the largest leverage among the REFERENCE rows of the
# pooled design (`hatvalues`), with the engine's tolerance h > hmax * (1 + 1e-9) + 1e-12. The
# golden records how many rows lie within 1e-6 (relative) of that line without being tied to it
# (a tie, as with integer predictors, is 1e-14), so the test can refuse a case the tolerance decides.
# =============================================================================

user_lib <- Sys.getenv("R_LIBS_USER")
if (nzchar(user_lib)) .libPaths(c(user_lib, .libPaths()))
invisible(Sys.setlocale("LC_COLLATE", "C"))
suppressWarnings(suppressMessages({ library(oaxaca); library(jsonlite); library(digest) }))
options(digits = 15, width = 200)

this_file <- sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE)[1])
REPO_ROOT <- normalizePath(file.path(dirname(this_file), ".."), mustWork = TRUE)
FIX <- file.path(REPO_ROOT, "oaxaca_blinder/tests/fixtures")
GOLDEN <- file.path(FIX, "pooled_target_goldens_r.json")
sha <- function(path) digest(file = path, algo = "sha256")
path_of <- function(f) file.path(FIX, f)

FIXTURES <- c("employers_trust_fixture.csv", "diag_df5.csv", "diag_nooverlap.csv", "diag_tiny.csv",
              "norm_skewed_fixture.csv", "norm_balanced_fixture.csv")
fixture_hash <- setNames(lapply(FIXTURES, function(f) sha(path_of(f))), FIXTURES)

LEVELS <- c(0.90, 0.95, 0.99)
TOL_H <- function(hmax) hmax * (1 + 1e-9) + 1e-12

pooled_case <- function(csv, outcome, group, cont, cats, ref_group, sample_rows, check_oaxaca) {
  d <- read.csv(csv, stringsAsFactors = FALSE)
  d$.ordinal <- seq_len(nrow(d)) - 1L                     # 0-based data-row ordinal
  d <- d[complete.cases(d[, c(outcome, group, cont, cats)]), ]
  for (cn in cats) d[[cn]] <- factor(d[[cn]])
  d$.is_target <- as.numeric(d[[group]] != ref_group)
  terms <- c(cont, cats)
  form <- as.formula(paste(outcome, "~", paste(c(terms, ".is_target"), collapse = " + ")))
  fit <- lm(form, data = d)
  gamma <- unname(coef(fit)[".is_target"])
  dof <- df.residual(fit)
  A <- d[d$.is_target == 0, ]; B <- d[d$.is_target == 1, ]

  # newdata at the REFERENCE level of the indicator, for every row
  at_ref <- function(dd) { dd$.is_target <- 0; dd }
  fair_all <- unname(predict(fit, newdata = at_ref(d)))
  # the identity: the target group's mean shortfall to the line is gamma (residuals sum to 0 per group)
  shortfall <- mean(B[[outcome]] - fair_all[d$.is_target == 1])
  scale <- max(1, abs(gamma))
  stopifnot(abs(shortfall - gamma) <= 1e-9 * scale)
  if (check_oaxaca) {
    o <- oaxaca(as.formula(paste(outcome, "~", paste(terms, collapse = " + "), "| .is_target")), data = d, R = NULL)
    ow <- o$twofold$overall
    ox <- ow[ow[, "group.weight"] == -2, "coef(unexplained)"]
    # oaxaca reports (group 0) - (group 1); the engine reports target - reference
    stopifnot(abs(abs(ox) - abs(gamma)) <= 1e-8 * scale)
  }

  # leverage of (x, 0) under (Z'Z)^-1 of the pooled design
  Z <- model.matrix(fit)
  ZtZi <- solve(crossprod(Z))
  Zref <- Z; Zref[, ".is_target"] <- 0
  h_ref0 <- rowSums((Zref %*% ZtZi) * Zref)                # leverage at indicator 0, every row
  hmax <- max(hatvalues(fit)[d$.is_target == 0])
  stopifnot(abs(max(h_ref0[d$.is_target == 0]) - hmax) <= 1e-12)
  extrap <- h_ref0 > TOL_H(hmax)
  margin <- abs(h_ref0 / hmax - 1)
  near_line <- sum(margin > 1e-12 & margin < 1e-6)          # rows the tolerance (1e-9) could decide

  idx_t <- which(d$.is_target == 1); idx_r <- which(d$.is_target == 0)
  pick_t <- if (is.null(sample_rows)) idx_t else unique(c(head(idx_t, sample_rows), idx_t[order(h_ref0[idx_t], decreasing = TRUE)[1:5]]))
  pick_r <- if (is.null(sample_rows)) idx_r else head(idx_r, sample_rows)

  lv <- list()
  for (cl in LEVELS) {
    rows <- function(idx) {
      pr <- predict(fit, newdata = at_ref(d[idx, ]), interval = "prediction", level = cl)
      lapply(seq_along(idx), function(i) list(ordinal = d$.ordinal[idx[i]], fair = pr[i, "fit"], lwr = pr[i, "lwr"],
                                              upr = pr[i, "upr"], wage = d[[outcome]][idx[i]]))
    }
    lv[[format(cl, nsmall = 2)]] <- list(critical = qt(1 - (1 - cl) / 2, dof), target = rows(pick_t), reference = rows(pick_r))
  }

  # the baseline-only fit the old interval used, for the planted mutant's contrast
  fit_ref <- lm(as.formula(paste(outcome, "~", paste(terms, collapse = " + "))), data = A)

  cf <- coef(fit)
  list(reference_group = ref_group, outcome = outcome, group = group, predictors = as.list(cont), categorical = as.list(cats),
       reference_rows = nrow(A), target_rows = nrow(B), model_columns = length(cf) - 1L,
       residual_df = dof, baseline_only_residual_df = df.residual(fit_ref),
       sigma_squared = sigma(fit)^2, gamma = gamma,
       coefficients = as.list(setNames(unname(cf[cont]), cont)),
       intercept = unname(cf["(Intercept)"]),
       h_max_reference = hmax, rows_near_line = near_line,
       extrapolated_ordinals = d$.ordinal[extrap],
       extrapolated_target_count = sum(extrap[d$.is_target == 1]),
       levels = lv)
}

cases <- list(
  employers = pooled_case(path_of("employers_trust_fixture.csv"), "Salary", "Gender", c("Age", "Experience_Years"), character(0), "Male", 30, TRUE),
  employers_cat = pooled_case(path_of("employers_trust_fixture.csv"), "Salary", "Gender", c("Age", "Experience_Years"),
                              c("Department", "Education_Level", "Location"), "Male", 30, FALSE),
  df5 = pooled_case(path_of("diag_df5.csv"), "pay", "grp", c("x1", "x2"), character(0), "Ref", NULL, TRUE),
  nooverlap = pooled_case(path_of("diag_nooverlap.csv"), "wage", "gender", c("edu"), character(0), "M", NULL, TRUE),
  tiny = pooled_case(path_of("diag_tiny.csv"), "wage", "gender", c("edu", "exp", "age"), character(0), "M", NULL, TRUE),
  skewed = pooled_case(path_of("norm_skewed_fixture.csv"), "log_salary", "Gender", c("Age", "Experience_Years"),
                       c("Department", "Location"), "Male", 30, FALSE),
  balanced = pooled_case(path_of("norm_balanced_fixture.csv"), "log_salary", "Gender", c("Age", "Experience_Years"),
                         c("Department", "Location"), "Male", 30, FALSE)
)

golden <- list(
  `_meta` = list(
    generated_utc = format(Sys.time(), tz = "UTC", usetz = TRUE),
    r_version = as.character(getRversion()),
    packages = list(oaxaca = as.character(packageVersion("oaxaca")), jsonlite = as.character(packageVersion("jsonlite")),
                    digest = as.character(packageVersion("digest"))),
    generator_sha256 = sha(this_file),
    fixture_sha256 = fixture_hash,
    tolerances = list(coefficients = 1e-9, identity = 1e-9, bounds = 1e-9, critical = 1e-10),
    expected_values_from_engine_output = FALSE
  ),
  cases = cases
)
writeLines(toJSON(golden, auto_unbox = TRUE, digits = I(17), pretty = TRUE, null = "null", na = "null"), GOLDEN)
cat("wrote", GOLDEN, "\n")
