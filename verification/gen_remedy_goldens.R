#!/usr/bin/env Rscript
# =============================================================================
# gen_remedy_goldens.R  --  oracle for the remedy's reported figures (0122-MERIDIAN)
# =============================================================================
# REGENERATION-ONLY TOOLING. `cargo test` never runs R: engine/tests/remedy_oracle_test.rs reads the
# committed `engine/tests/fixtures/remedy_goldens_r.json` offline and REFUSES a golden whose recorded
# sha256 of this script or of any fixture no longer matches the files on disk.
#
# NO EXPECTED VALUE IN THE GOLDEN COMES FROM ENGINE OUTPUT. This script re-implements, in base R, what
# the engine claims about a remedy and about a schedule, and shares no code with it:
#
#   * the fair wage and its prediction interval: `lm` + `predict.lm(interval = "prediction")` on the
#     reference group's own regression (target Reference) or on the pooled regression with a
#     compared-group indicator read at indicator 0 (target Pooled);
#   * who is eligible (below the chosen line by more than 1e-6 and by at least `min_gap_pct` of
#     current pay), the order they are paid in (largest shortfall first; ties compared before
#     reference, then row order), and the two strategies (Greedy pays down the order, Equitable pays
#     the same share of every shortfall);
#   * THE LINE IS REFITTED on the schedule's wages after every payment: `new_unexplained_gap` is
#     mean_T(y' - predict(lm(y' ~ x, reference rows))) for Reference and the indicator's `lm`
#     coefficient for Pooled. The engine takes this from an exact linear-weights formula; here it is
#     a refit, so a reference raise that moves the line, or a Pooled leverage effect, is measured, not
#     assumed;
#   * THE BUDGET FOR A TARGET GAP is found by ROOT FINDING on that refit (`uniroot`, tol 1e-12), not
#     by the closed form n_T (g_T - u0) the engine uses on the Reference line, nor the engine's walk
#     along the pay order for Pooled;
#   * the schedule checks: where each compared employee sits against the interval before and after,
#     the exact group test (`summary(lm)` coefficient, t, p, df of the indicator on the schedule's
#     wages), required budget, the cost split.
#
# SIGN. Every gap is mean(compared) minus the line (or the reference mean): negative while the
# compared group is underpaid, rising with every dollar paid.
#
#   R_LIBS_USER=/home/deji/R/library Rscript verification/gen_remedy_goldens.R
# =============================================================================

user_lib <- Sys.getenv("R_LIBS_USER")
if (nzchar(user_lib)) .libPaths(c(user_lib, .libPaths()))
invisible(Sys.setlocale("LC_COLLATE", "C"))
suppressWarnings(suppressMessages({ library(jsonlite); library(digest) }))
options(digits = 15, width = 200)

this_file <- sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE)[1])
REPO_ROOT <- normalizePath(file.path(dirname(this_file), ".."), mustWork = TRUE)
GOLDEN <- file.path(REPO_ROOT, "engine/tests/fixtures/remedy_goldens_r.json")
sha <- function(path) digest(file = path, algo = "sha256")

FIXTURES <- list(
  F = list(fixture = "engine/tests/fixtures/0122-fixture-f-noisy.csv", outcome = "Salary", group = "Gender",
           ref = "Male", cont = c("Experience", "Level"), cats = character(0)),
  K = list(fixture = "oaxaca_blinder/tests/fixtures/diag_kink_overlap.csv", outcome = "wage", group = "gender",
           ref = "M", cont = c("edu"), cats = character(0)),
  E = list(fixture = "oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv", outcome = "Salary",
           group = "Gender", ref = "Male", cont = c("Age", "Experience_Years"), cats = character(0)),
  T = list(fixture = "engine/tests/fixtures/0122-remedy-tiny.csv", outcome = "wage", group = "group",
           ref = "R", cont = c("x"), cats = character(0))
)
fixture_hash <- setNames(lapply(sapply(FIXTURES, function(f) f$fixture), function(f) sha(file.path(REPO_ROOT, f))),
                         sapply(FIXTURES, function(f) f$fixture))

TOL_POSITION <- 0.01   # one cent of slack either side of the interval, the defensibility tolerance

posf <- function(w, lwr, upr) ifelse(w < lwr - TOL_POSITION, "Below", ifelse(w > upr + TOL_POSITION, "Above", "Inside"))

# ---- data ---------------------------------------------------------------------------------------
load_case <- function(cfg) {
  d <- read.csv(file.path(REPO_ROOT, cfg$fixture), stringsAsFactors = FALSE, check.names = FALSE)
  d$.ordinal <- seq_len(nrow(d)) - 1L
  for (ov in cfg$overrides) d[d$.ordinal == ov$index, ov$column] <- ov$value
  keep <- complete.cases(d[, c(cfg$outcome, cfg$group, cfg$cont, cfg$cats)])
  d <- d[keep, ]
  for (cn in cfg$cats) d[[cn]] <- factor(d[[cn]])
  d$.y <- d[[cfg$outcome]]
  d$.t <- as.numeric(d[[cfg$group]] != cfg$ref)
  rownames(d) <- NULL
  d
}

formulas <- function(cfg) {
  terms <- c(cfg$cont, cfg$cats)
  list(ref = as.formula(paste(".y ~", paste(terms, collapse = " + "))),
       pooled = as.formula(paste(".y ~", paste(c(terms, ".t"), collapse = " + "))))
}

# Fair wage and prediction interval at the reference level of the indicator, for EVERY analysed row.
line_of <- function(d, y, cfg) {
  f <- formulas(cfg)
  dd <- d; dd$.y <- y
  if (cfg$target == "Reference") {
    fit <- lm(f$ref, data = dd[dd$.t == 0, ]); nd <- dd
  } else {
    fit <- lm(f$pooled, data = dd); nd <- dd; nd$.t <- 0
  }
  pr <- predict(fit, newdata = nd, interval = "prediction", level = cfg$level)
  list(fair = unname(pr[, "fit"]), lwr = unname(pr[, "lwr"]), upr = unname(pr[, "upr"]))
}

# The compared group's mean (y - fair) on the line REFITTED to the wages `y`.
gap_after <- function(d, y, cfg) {
  f <- formulas(cfg)
  dd <- d; dd$.y <- y
  if (cfg$target == "Reference") {
    fit <- lm(f$ref, data = dd[dd$.t == 0, ])
    t <- dd[dd$.t == 1, ]
    mean(t$.y - predict(fit, newdata = t))
  } else {
    unname(coef(lm(f$pooled, data = dd))[".t"])
  }
}

# ---- allocation -----------------------------------------------------------------------------------
allocate <- function(d, diff, elig, strategy, cap) {
  pay <- numeric(nrow(d))
  idx <- which(elig)
  if (length(idx) == 0) return(pay)
  ord <- idx[order(-diff[idx], d$.t[idx] == 0, d$.ordinal[idx])]
  need <- sum(diff[idx])
  if (strategy == "Greedy") {
    before <- cumsum(c(0, diff[ord]))[seq_along(ord)]
    pay[ord] <- pmin(diff[ord], pmax(0, cap - before))
  } else {
    ratio <- if (need > 0) min(1, cap / need) else 0
    pay[ord] <- diff[ord] * ratio
  }
  pay
}

run_remedy <- function(cfg) {
  d <- load_case(cfg)
  y <- d$.y
  isT <- d$.t == 1
  ln <- line_of(d, y, cfg)
  mid <- ln$fair
  linew <- switch(cfg$range, Midpoint = mid, LowerBound = ln$lwr, UpperBound = ln$upr)
  gapd <- linew - y
  pct <- ifelse(abs(y) > 1e-6, gapd / y, 0)
  pos <- gapd > 1e-6
  elig <- pos & pct >= cfg$min_pct & (isT | cfg$adjust_both)

  u0 <- mean(y[isT] - mid[isT])
  need_t <- sum(gapd[elig & isT]); need_r <- sum(gapd[elig & !isT]); alloc_need <- need_t + need_r
  overshoot <- mean(pmax(0, y[isT] - mid[isT]))
  thr_excl <- sum(isT & pos & pct < cfg$min_pct)

  pay_for <- function(cap) allocate(d, gapd, elig, cfg$strategy, cap)
  best <- gap_after(d, y + pay_for(Inf), cfg)

  g <- NULL
  spec <- cfg$target_gap_spec
  if (!is.null(spec)) {
    g <- switch(spec$kind,
      frac = u0 + spec$value * (best - u0),
      u0_plus = u0 + spec$value,
      best_plus = best + spec$value,
      abs = spec$value)
  }
  state <- "none"; tcap <- NA_real_; tbudget <- NA_real_; reachable <- NA; shortfall <- NA_real_
  if (!is.null(g)) {
    eps <- 1e-10 * max(1, abs(g), abs(u0), abs(best))
    if (g <= u0 + eps) {
      state <- "already_met"; tcap <- 0; tbudget <- 0; reachable <- TRUE
    } else if (g > best + eps) {
      state <- "unreachable"; tbudget <- need_t; reachable <- FALSE; shortfall <- g - best
    } else {
      f <- function(B) gap_after(d, y + pay_for(B), cfg) - g
      # The gap must be monotone in the budget for a root to mean "the least that reaches g".
      grid <- seq(0, need_t, length.out = 41)
      vals <- sapply(grid, f)
      stopifnot(all(diff(vals) >= -1e-9 * max(1, abs(vals))))
      B <- if (abs(f(need_t)) < 1e-12 * max(1, abs(g))) need_t else uniroot(f, c(0, need_t), tol = 1e-12, maxiter = 1000)$root
      state <- "reachable"; tcap <- B; tbudget <- B; reachable <- TRUE
    }
  }
  user_cap <- if (cfg$budget > 0) cfg$budget else NA_real_
  caps <- c(user_cap, tcap); caps <- caps[!is.na(caps)]
  cap <- if (length(caps) == 0) Inf else min(caps)
  eff <- if (is.infinite(cap)) alloc_need * 1.00001 else cap
  pay <- pay_for(eff)

  cost_t <- sum(pay[isT]); cost_r <- sum(pay[!isT])
  y_after <- y + pay
  limit <- min(if (is.na(tcap)) Inf else tcap, alloc_need)
  binding <- !is.na(user_cap) && (user_cap + 1e-9 * max(1, user_cap) < limit)

  expected <- list(
    original_gap = mean(y[isT]) - mean(y[!isT]),
    new_gap = mean(y_after[isT]) - mean(y_after[!isT]),
    original_unexplained_gap = u0,
    new_unexplained_gap = gap_after(d, y_after, cfg),
    required_budget = need_t, need_target = need_t, need_reference = need_r,
    total_cost = cost_t + cost_r, cost_target = cost_t, cost_reference = cost_r,
    best_reachable_gap = best, overshoot_mean = overshoot,
    closure = if (need_t > 0) cost_t / need_t else NA_real_,
    unfunded_amount = max(0, need_t - cost_t),
    unfunded_count = sum(isT & elig & (gapd - pay) > 1e-6),
    threshold_excluded_count = thr_excl,
    budget_binding = binding,
    target_state = state, target_gap_reachable = reachable,
    shortfall_to_target = shortfall, target_budget = tbudget)
  cfg$target_gap <- g
  rows <- NULL
  if (isTRUE(cfg$rows)) {
    listed <- elig
    rows <- lapply(which(listed), function(i) list(
      index = d$.ordinal[i], source = if (isT[i]) "Compared" else "Reference", adjustment = pay[i],
      current_wage = y[i], new_wage = y_after[i], fair_wage = mid[i], lower = ln$lwr[i], upper = ln$upr[i],
      range_position = posf(y_after[i], ln$lwr[i], ln$upr[i]), range_position_before = posf(y[i], ln$lwr[i], ln$upr[i])))
  }
  list(config = cfg, expected = expected, rows = rows)
}

# ---- schedules ------------------------------------------------------------------------------------
make_schedule <- function(d, cfg, ln) {
  y <- d$.y; isT <- d$.t == 1
  short <- pmax(0, ln$fair - y)
  s <- cfg$schedule
  pay <- numeric(nrow(d))
  switch(s$kind,
    explicit = { for (a in s$adjustments) pay[d$.ordinal == a$index] <- pay[d$.ordinal == a$index] + a$value },
    first_k = { idx <- which(isT & short > 1e-6); idx <- idx[order(d$.ordinal[idx])][seq_len(s$k)]; pay[idx] <- short[idx] },
    scaled = { idx <- which(isT & short > 1e-6); pay[idx] <- s$factor * short[idx] },
    to_upper = { idx <- which(isT & short > 1e-6); pay[idx] <- ln$upr[idx] - y[idx] + s$delta },
    both_groups = {
      it <- which(isT & short > 1e-6); pay[it] <- short[it]
      ir <- which(!isT & short > 1e-6); pay[ir] <- s$reference_factor * short[ir]
    })
  idx <- which(pay != 0)
  list(adjustments = lapply(idx[order(d$.ordinal[idx])], function(i) list(index = d$.ordinal[i], value = pay[i])), pay = pay)
}

run_schedule <- function(cfg) {
  d <- load_case(cfg)
  y <- d$.y; isT <- d$.t == 1
  ln <- line_of(d, y, cfg)
  sched <- make_schedule(d, cfg, ln)
  pay <- sched$pay
  y_after <- y + pay
  cfg$adjustments <- sched$adjustments
  mid <- ln$fair
  u0 <- mean(y[isT] - mid[isT])
  before <- posf(y[isT], ln$lwr[isT], ln$upr[isT])
  after <- posf(y_after[isT], ln$lwr[isT], ln$upr[isT])
  f <- formulas(cfg)
  dd <- d; dd$.y <- y_after
  fit_p <- lm(f$pooled, data = dd)
  cs <- summary(fit_p)$coefficients[".t", ]
  need_t <- sum(pmax(0, mid[isT] - y[isT])[(mid[isT] - y[isT]) > 1e-6])
  expected <- list(
    original_gap = mean(y[isT]) - mean(y[!isT]),
    new_gap = mean(y_after[isT]) - mean(y_after[!isT]),
    original_unexplained_gap = u0,
    new_unexplained_gap = gap_after(d, y_after, cfg),
    required_budget = need_t, need_target = need_t,
    total_cost = sum(pay), cost_target = sum(pay[isT]), cost_reference = sum(pay[!isT]),
    position_counts = list(
      below = sum(after == "Below"), inside = sum(after == "Inside"), above = sum(after == "Above"),
      below_before = sum(before == "Below"), inside_before = sum(before == "Inside"), above_before = sum(before == "Above"),
      newly_above = sum(after == "Above" & before != "Above")),
    group_test = list(group_coefficient = unname(cs["Estimate"]), t_statistic = unname(cs["t value"]),
                      p_value = unname(cs["Pr(>|t|)"]), degrees_of_freedom = df.residual(fit_p),
                      is_significant = unname(cs["Pr(>|t|)"]) < (1 - cfg$level)))
  rows <- NULL
  if (isTRUE(cfg$rows)) {
    idx <- which(pay != 0)
    rows <- lapply(idx[order(d$.ordinal[idx])], function(i) list(
      index = d$.ordinal[i], source = if (isT[i]) "Compared" else "Reference", adjustment = pay[i],
      current_wage = y[i], new_wage = y_after[i], fair_wage = mid[i], lower = ln$lwr[i], upper = ln$upr[i],
      range_position = posf(y_after[i], ln$lwr[i], ln$upr[i]), range_position_before = posf(y[i], ln$lwr[i], ln$upr[i])))
  }
  list(config = cfg, expected = expected, rows = rows)
}

# ---- the cases ------------------------------------------------------------------------------------
remedy <- function(fix, target = "Reference", strategy = "Greedy", range = "Midpoint", min_pct = 0,
                   adjust_both = FALSE, budget = 0, spec = NULL, rows = TRUE) {
  cfg <- c(FIXTURES[[fix]], list(kind = "remedy", target = target, strategy = strategy, range = range,
                                 min_pct = min_pct, adjust_both = adjust_both, budget = budget,
                                 target_gap_spec = spec, level = 0.95, rows = rows, overrides = list()))
  cfg
}
schedule <- function(fix, target = "Reference", schedule, overrides = list(), rows = TRUE) {
  c(FIXTURES[[fix]], list(kind = "schedule", target = target, schedule = schedule, level = 0.95, rows = rows,
                          overrides = overrides))
}
frac <- function(v) list(kind = "frac", value = v)

CASES <- list(
  # ---- the tiny roster the hand figures use (<= 6 rows) ----
  tiny_ref_greedy_full      = remedy("T"),
  tiny_ref_greedy_t500      = remedy("T", spec = list(kind = "abs", value = -500)),
  tiny_ref_equitable_t500   = remedy("T", strategy = "Equitable", spec = list(kind = "abs", value = -500)),
  tiny_ref_unreachable      = remedy("T", spec = list(kind = "abs", value = 500)),
  tiny_ref_already_met      = remedy("T", spec = list(kind = "abs", value = -2000)),
  tiny_ref_cap_binding      = remedy("T", budget = 1000, spec = list(kind = "abs", value = -500)),
  # ---- Fixture F, noisy reference line ----
  F_ref_greedy_full         = remedy("F"),
  F_ref_greedy_cap25000     = remedy("F", budget = 25000),
  F_ref_equitable_cap25000  = remedy("F", strategy = "Equitable", budget = 25000),
  F_ref_greedy_half         = remedy("F", spec = frac(0.5)),
  F_ref_equitable_half      = remedy("F", strategy = "Equitable", spec = frac(0.5)),
  F_ref_target_is_best      = remedy("F", spec = frac(1.0)),
  F_ref_unreachable         = remedy("F", spec = list(kind = "best_plus", value = 100)),
  F_ref_already_met         = remedy("F", spec = list(kind = "u0_plus", value = -100)),
  F_ref_target_cap_binding  = remedy("F", budget = 12000, spec = frac(0.5)),
  F_ref_threshold2          = remedy("F", min_pct = 0.02),
  F_ref_threshold5          = remedy("F", min_pct = 0.05),
  F_ref_threshold5_half     = remedy("F", min_pct = 0.05, spec = frac(0.5)),
  F_ref_threshold5_cap      = remedy("F", min_pct = 0.05, budget = 8000),
  F_ref_lower_full          = remedy("F", range = "LowerBound"),
  F_ref_lower_half          = remedy("F", range = "LowerBound", spec = frac(0.5)),
  F_ref_upper_full          = remedy("F", range = "UpperBound"),
  F_ref_upper_cap           = remedy("F", range = "UpperBound", budget = 30000),
  F_pooled_greedy_full      = remedy("F", target = "Pooled"),
  F_pooled_greedy_cap25000  = remedy("F", target = "Pooled", budget = 25000),
  F_pooled_equitable_cap    = remedy("F", target = "Pooled", strategy = "Equitable", budget = 25000),
  F_pooled_greedy_half      = remedy("F", target = "Pooled", spec = frac(0.5)),
  F_pooled_equitable_half   = remedy("F", target = "Pooled", strategy = "Equitable", spec = frac(0.5)),
  F_pooled_greedy_quarter   = remedy("F", target = "Pooled", spec = frac(0.25)),
  F_pooled_unreachable      = remedy("F", target = "Pooled", spec = list(kind = "best_plus", value = 100)),
  F_pooled_lower_half       = remedy("F", target = "Pooled", range = "LowerBound", spec = frac(0.5)),
  F_pooled_threshold5_half  = remedy("F", target = "Pooled", min_pct = 0.05, spec = frac(0.5)),
  # ---- reference raises: the line moves ----
  F_ref_both_full           = remedy("F", adjust_both = TRUE),
  F_ref_both_cap20000       = remedy("F", adjust_both = TRUE, budget = 20000),
  F_ref_both_equitable_cap  = remedy("F", adjust_both = TRUE, strategy = "Equitable", budget = 20000),
  F_ref_both_lower_full     = remedy("F", adjust_both = TRUE, range = "LowerBound"),
  F_ref_both_upper_cap      = remedy("F", adjust_both = TRUE, range = "UpperBound", budget = 40000),
  F_ref_both_threshold5     = remedy("F", adjust_both = TRUE, min_pct = 0.05),
  F_pooled_both_full        = remedy("F", target = "Pooled", adjust_both = TRUE),
  F_pooled_both_cap20000    = remedy("F", target = "Pooled", adjust_both = TRUE, budget = 20000),
  F_pooled_both_equitable   = remedy("F", target = "Pooled", adjust_both = TRUE, strategy = "Equitable", budget = 20000),
  # ---- the kink fixture (one predictor, steep overlap) ----
  K_ref_greedy_full         = remedy("K"),
  K_ref_greedy_half         = remedy("K", spec = frac(0.5)),
  K_ref_both_full           = remedy("K", adjust_both = TRUE),
  K_ref_both_cap            = remedy("K", adjust_both = TRUE, budget = 60000),
  K_pooled_greedy_half      = remedy("K", target = "Pooled", spec = frac(0.5)),
  K_pooled_both_full        = remedy("K", target = "Pooled", adjust_both = TRUE),
  # ---- the 10,000-row employers roster (aggregates only) ----
  E_ref_greedy_full         = remedy("E", rows = FALSE),
  E_ref_greedy_to_zero      = remedy("E", spec = list(kind = "abs", value = 0), rows = FALSE),
  E_ref_greedy_cap          = remedy("E", budget = 5000000, rows = FALSE),
  E_pooled_greedy_half      = remedy("E", target = "Pooled", spec = frac(0.5), rows = FALSE)
)

SCHEDULES <- list(
  F_sched_partial_first15   = schedule("F", schedule = list(kind = "first_k", k = 15)),
  F_sched_generous_x3       = schedule("F", schedule = list(kind = "scaled", factor = 3)),
  F_sched_to_upper          = schedule("F", schedule = list(kind = "to_upper", delta = 0)),
  F_sched_over_upper        = schedule("F", schedule = list(kind = "to_upper", delta = 0.02)),
  F_sched_both_groups       = schedule("F", schedule = list(kind = "both_groups", reference_factor = 0.5)),
  F_sched_both_groups_pooled = schedule("F", target = "Pooled", schedule = list(kind = "both_groups", reference_factor = 0.5)),
  F_sched_pooled_partial    = schedule("F", target = "Pooled", schedule = list(kind = "first_k", k = 20)),
  F_sched_pooled_generous   = schedule("F", target = "Pooled", schedule = list(kind = "scaled", factor = 3)),
  F_sched_override          = schedule("F", schedule = list(kind = "explicit", adjustments = list(
                                  list(index = 13, value = 800), list(index = 38, value = 1500), list(index = 3, value = 400))),
                                overrides = list(list(index = 13, column = "Experience", value = 9),
                                                 list(index = 38, column = "Level", value = 5))),
  K_sched_both_groups       = schedule("K", schedule = list(kind = "both_groups", reference_factor = 0.5)),
  K_sched_pooled_both       = schedule("K", target = "Pooled", schedule = list(kind = "both_groups", reference_factor = 0.5))
)

cases <- list()
for (nm in names(CASES)) {
  cat("remedy  ", nm, "\n")
  cases[[nm]] <- run_remedy(CASES[[nm]])
}
for (nm in names(SCHEDULES)) {
  cat("schedule", nm, "\n")
  cases[[nm]] <- run_schedule(SCHEDULES[[nm]])
}

golden <- list(
  `_meta` = list(
    generated_utc = format(Sys.time(), tz = "UTC", usetz = TRUE),
    r_version = as.character(getRversion()),
    packages = list(jsonlite = as.character(packageVersion("jsonlite")), digest = as.character(packageVersion("digest"))),
    generator_sha256 = sha(this_file),
    fixture_sha256 = fixture_hash,
    tolerances = list(dollars = 1e-6, per_person_gap = 1e-8, relative = 1e-9),
    expected_values_from_engine_output = FALSE
  ),
  cases = cases
)
writeLines(toJSON(golden, auto_unbox = TRUE, digits = I(17), pretty = TRUE, null = "null", na = "null"), GOLDEN)
cat("wrote", GOLDEN, "\n")
