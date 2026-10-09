#!/usr/bin/env Rscript
# =============================================================================
# gen_norm_goldens.R  --  oracle for categorical-coefficient normalisation (0120-MERIDIAN)
# =============================================================================
# REGENERATION-ONLY TOOLING. `cargo test` never runs R: the Rust tests read the committed
# fixtures and `norm_goldens_r.json` offline, and REFUSE a golden whose recorded sha256 of this
# script or of any fixture no longer matches the files on disk (a stale golden is an error).
#
# NO EXPECTED VALUE IN THE GOLDEN COMES FROM ENGINE OUTPUT. The only engine-derived input is the
# per-group RIF outcome column of `norm_skewed_rif.csv` (stage 2), and it enters R as an ordinary
# outcome vector: a RIF-OLS on a given y is plain OLS, so the package oracles apply to the
# normalisation exactly. The Rust test re-derives that column from the engine and refuses the
# file if it has drifted.
#
# Two stages, run by verification/regen_norm_goldens.sh:
#   Rscript gen_norm_goldens.R fixtures   writes the two synthetic fixtures
#   (cargo run --example emit_rif_fixture writes the RIF columns)
#   Rscript gen_norm_goldens.R goldens    writes norm_goldens_r.json
#
# Requires R, oaxaca 0.1.5, ddecompose 1.0.0, jsonlite, digest.
#   R_LIBS_USER=/path/to/lib Rscript verification/gen_norm_goldens.R goldens
#
# THREE INDEPENDENT ROUTES, cross-checked inside this script before anything is written:
#  (1) REFIT. Weighted effect coding (te Grotenhuis et al. 2017; hand-coded because the `wec`
#      package is not installed): for a factor with shares s and omitted level b, regressor j is
#      D_j - (s_j/s_b) D_b, so the fitted coefficients satisfy sum_k s_k beta_k = 0 DIRECTLY,
#      from a base-R lm() refit. With equal shares this is effect (contr.sum-style) coding.
#  (2) POST-HOC. Treatment-coded lm(), then beta_j <- beta_j - c, intercept <- intercept + c,
#      base <- -c, c = sum_{j != b} s_j beta_j. Must equal (1) at 1e-12.
#  (3) PACKAGES. ddecompose(normalize_factors = TRUE) and R oaxaca() (third formula part), which
#      implement the EQUAL-share restriction only. No package implements the population-share
#      one, so route (3) anchors the shared OB arithmetic: this script asserts that its own
#      equal-share GroupA/GroupB output equals ddecompose's at 1e-9 before any population-share
#      golden is written.
# Level names containing a '.' are unsupported by ddecompose's row naming: no fixture level has one.
# =============================================================================

user_lib <- Sys.getenv("R_LIBS_USER")
if (nzchar(user_lib)) .libPaths(c(user_lib, .libPaths()))
invisible(Sys.setlocale("LC_COLLATE", "C"))   # radix/C order == the engine's byte-wise level sort
suppressWarnings(suppressMessages({
  library(oaxaca); library(ddecompose); library(jsonlite); library(digest)
}))
options(digits = 15, width = 200)

MODE <- commandArgs(trailingOnly = TRUE)[1]
if (is.na(MODE) || !(MODE %in% c("fixtures", "goldens"))) stop("usage: gen_norm_goldens.R fixtures|goldens")

SEED <- 20261009L
RNGkind("Mersenne-Twister", "Inversion", "Rejection")

this_file <- sub("--file=", "", grep("--file=", commandArgs(FALSE), value = TRUE)[1])
REPO_ROOT <- normalizePath(file.path(dirname(this_file), ".."), mustWork = TRUE)
FIX_DIR   <- file.path(REPO_ROOT, "oaxaca_blinder/tests/fixtures")
EMPLOYERS <- file.path(FIX_DIR, "employers_trust_fixture.csv")
SKEWED    <- file.path(FIX_DIR, "norm_skewed_fixture.csv")
BALANCED  <- file.path(FIX_DIR, "norm_balanced_fixture.csv")
RIFCSV    <- file.path(FIX_DIR, "norm_skewed_rif.csv")
GOLDEN    <- file.path(FIX_DIR, "norm_goldens_r.json")

g17 <- function(x) if (is.numeric(x)) trimws(formatC(x, digits = 17, format = "g")) else as.character(x)
sha <- function(path) digest(file = path, algo = "sha256")
fct <- function(x) factor(x, levels = sort(unique(as.character(x)), method = "radix"))

# =============================================================================
# STAGE 1 -- fixtures
# =============================================================================
if (MODE == "fixtures") {
  emp <- read.csv(EMPLOYERS, stringsAsFactors = FALSE)
  set.seed(SEED)

  # ---- skewed: unbalanced everywhere, a tiny department, integer weights, blank Tenure cells ----
  # Male = engine group A (340 rows), Female = reference group B (260 rows).
  # Department pooled counts 360/180/52/8 (60.0/30.0/8.7/1.3 %), mix differs by 23 and 22 points
  # between the groups; Location 250/220/130 with distinct counts in each group.
  take <- function(g, n) { r <- emp[emp$Gender == g, ]; r[sample.int(nrow(r), n), c("Age", "Experience_Years", "Gender")] }
  A <- take("Male", 340); B <- take("Female", 260)
  assign_levels <- function(counts) sample(rep(names(counts), counts))
  A$Department <- assign_levels(c(Admin = 170, Eng = 135, Ops = 30, Zeta = 5))
  B$Department <- assign_levels(c(Admin = 190, Eng = 45,  Ops = 22, Zeta = 3))
  A$Location   <- assign_levels(c(Boston = 120, Chicago = 140, Denver = 80))
  B$Location   <- assign_levels(c(Boston = 130, Chicago = 80,  Denver = 50))
  sk <- rbind(A, B)
  dept_eff <- list(Male   = c(Admin = 0, Eng = .25, Ops = .10, Zeta = -.20),
                   Female = c(Admin = 0, Eng = .18, Ops = .12, Zeta = -.05))
  loc_eff  <- list(Male   = c(Boston = 0, Chicago = .06, Denver = -.04),
                   Female = c(Boston = 0, Chicago = .02, Denver = -.01))
  slope    <- list(Male = c(Age = .012, Exp = .018, b0 = 10.40), Female = c(Age = .009, Exp = .021, b0 = 10.28))
  sk$log_salary <- vapply(seq_len(nrow(sk)), function(i) {
    g <- sk$Gender[i]
    slope[[g]][["b0"]] + slope[[g]][["Age"]] * (sk$Age[i] - 40) + slope[[g]][["Exp"]] * sk$Experience_Years[i] / 10 +
      dept_eff[[g]][[sk$Department[i]]] + loc_eff[[g]][[sk$Location[i]]] }, numeric(1)) + rnorm(nrow(sk), 0, .12)
  sk$Tenure <- round(runif(nrow(sk), 0, 25), 1)
  sk$Tenure[c(6, 78, 141, 302, 451, 600)] <- NA                  # blank cells in a non-outcome predictor
  sk$w <- ifelse(sk$Gender == "Male", sample(1:4, nrow(sk), TRUE, c(.4, .3, .2, .1)), sample(1:4, nrow(sk), TRUE, c(.1, .2, .3, .4)))
  sk <- sk[sample.int(nrow(sk)), c("log_salary", "Age", "Experience_Years", "Gender", "Department", "Location", "Tenure", "w")]
  rownames(sk) <- NULL
  out <- sk; for (nm in c("log_salary", "Tenure")) out[[nm]] <- ifelse(is.na(sk[[nm]]), "", g17(sk[[nm]]))
  write.table(out, SKEWED, sep = ",", row.names = FALSE, quote = FALSE)

  # ---- balanced: every factor EXACTLY balanced in the POOLED sample, unbalanced inside each group ----
  # So population-share == equal-share on this file; a share vector taken from one group, or the
  # wrong rows, breaks the identity. 300 + 300 rows.
  take2 <- function(g, n) { r <- emp[emp$Gender == g, ]; r[sample.int(nrow(r), n), c("Age", "Experience_Years", "Gender")] }
  A2 <- take2("Male", 300); B2 <- take2("Female", 300)
  A2$Department <- assign_levels(c(Alpha = 120, Bravo = 90, Charlie = 60, Delta = 30))
  B2$Department <- assign_levels(c(Alpha = 30,  Bravo = 60, Charlie = 90, Delta = 120))
  A2$Location   <- assign_levels(c(Lisbon = 130, Madrid = 100, Oslo = 70))
  B2$Location   <- assign_levels(c(Lisbon = 70,  Madrid = 100, Oslo = 130))
  A2$Union      <- assign_levels(c(Member = 210, NonMember = 90))
  B2$Union      <- assign_levels(c(Member = 90,  NonMember = 210))
  ba <- rbind(A2, B2)
  ba$log_salary <- 10.3 + ifelse(ba$Gender == "Male", .011, .008) * (ba$Age - 40) + .015 * ba$Experience_Years / 10 +
    c(Alpha = 0, Bravo = .08, Charlie = .21, Delta = .30)[ba$Department] * ifelse(ba$Gender == "Male", 1, .7) +
    c(Lisbon = 0, Madrid = .05, Oslo = -.06)[ba$Location] + ifelse(ba$Union == "Member", .09, 0) * ifelse(ba$Gender == "Male", 1, 1.4) +
    rnorm(nrow(ba), 0, .11)
  ba <- ba[sample.int(nrow(ba)), c("log_salary", "Age", "Experience_Years", "Gender", "Department", "Location", "Union")]
  rownames(ba) <- NULL
  out <- ba; out$log_salary <- g17(ba$log_salary)
  write.table(out, BALANCED, sep = ",", row.names = FALSE, quote = FALSE)
  cat("wrote", SKEWED, "\nwrote", BALANCED, "\n")
  quit(save = "no", status = 0)
}

# =============================================================================
# STAGE 2 -- goldens
# =============================================================================
read_fix <- function(path) {
  d <- read.csv(path, stringsAsFactors = FALSE, na.strings = c("", "NA"))
  d
}

# ---- shares of the model.frame rows (rows with a blank in ANY model column are gone; see run_case) ----
level_shares <- function(d, v, w = NULL, equal = FALSE) {
  lv <- sort(unique(as.character(d[[v]])), method = "radix")
  mass <- vapply(lv, function(l) if (is.null(w)) sum(d[[v]] == l) else sum(w[d[[v]] == l]), numeric(1))
  s <- if (equal) rep(1 / length(lv), length(lv)) else mass / sum(mass)
  names(s) <- lv
  s
}

# ---- route (1): weighted-effect-coding refit ----
make_design <- function(d, nums, cats, shares, ind = NULL) {
  cols <- list(`__ob_intercept__` = rep(1, nrow(d)))
  for (n in nums) cols[[n]] <- as.numeric(d[[n]])
  for (v in cats) {
    lv <- names(shares[[v]]); b <- lv[1]
    for (j in lv[-1]) cols[[paste0(v, "_", j)]] <- as.numeric(d[[v]] == j) - (shares[[v]][[j]] / shares[[v]][[b]]) * as.numeric(d[[v]] == b)
  }
  if (!is.null(ind)) cols[["__ob_group_indicator__"]] <- ind
  as.data.frame(cols, check.names = FALSE)
}
fit_cols <- function(X, y, w = NULL) {
  nm <- names(X); X2 <- X; names(X2) <- paste0("c", seq_along(nm))
  dat <- cbind(y = y, X2)
  f <- as.formula(paste("y ~ 0 +", paste(names(X2), collapse = " + ")))
  m <- if (is.null(w)) lm(f, data = dat) else lm(f, data = dat, weights = w)
  b <- unname(coef(m)); names(b) <- nm; b
}
full_names <- function(nums, cats, shares) c("__ob_intercept__", nums, unlist(lapply(cats, function(v) paste0(v, "_", names(shares[[v]])))))
effects_from_wec <- function(b, nums, cats, shares) {
  out <- c(`__ob_intercept__` = b[["__ob_intercept__"]]); for (n in nums) out[n] <- b[[n]]
  for (v in cats) {
    lv <- names(shares[[v]]); base <- lv[1]
    for (j in lv[-1]) out[paste0(v, "_", j)] <- b[[paste0(v, "_", j)]]
    out[paste0(v, "_", base)] <- -sum(vapply(lv[-1], function(j) shares[[v]][[j]] / shares[[v]][[base]] * b[[paste0(v, "_", j)]], numeric(1)))
  }
  out[full_names(nums, cats, shares)]
}
means_full <- function(d, nums, cats, shares, w = NULL) {
  wm <- function(x) if (is.null(w)) mean(x) else sum(w * x) / sum(w)
  out <- c(`__ob_intercept__` = 1); for (n in nums) out[n] <- wm(as.numeric(d[[n]]))
  for (v in cats) for (l in names(shares[[v]])) out[paste0(v, "_", l)] <- wm(as.numeric(d[[v]] == l))
  out[full_names(nums, cats, shares)]
}

# ---- route (2): treatment-coded lm() + post-hoc shift ----
treat_fit <- function(d, y, nums, cats, w = NULL, ind = NULL) {
  dd <- d; for (v in cats) dd[[v]] <- fct(d[[v]])
  dd$y_ <- y; if (!is.null(ind)) dd$ind_ <- ind
  f <- as.formula(paste("y_ ~", paste(c(nums, cats, if (!is.null(ind)) "ind_"), collapse = " + ")))
  m <- if (is.null(w)) lm(f, data = dd) else lm(f, data = dd, weights = w)
  b <- coef(m)
  nm <- names(b)
  eng <- vapply(nm, function(t) { if (t == "(Intercept)") return("__ob_intercept__"); if (t == "ind_") return("__ob_group_indicator__")
    for (v in cats) if (startsWith(t, v) && !(t %in% nums)) return(paste0(v, "_", substring(t, nchar(v) + 1))); t }, character(1))
  names(b) <- eng
  b
}
posthoc_effects <- function(b, nums, cats, shares) {
  out <- b
  res <- c(`__ob_intercept__` = b[["__ob_intercept__"]]); for (n in nums) res[n] <- b[[n]]
  for (v in cats) {
    lv <- names(shares[[v]]); base <- lv[1]
    raw <- c(0, vapply(lv[-1], function(j) b[[paste0(v, "_", j)]], numeric(1))); names(raw) <- lv
    cc <- sum(shares[[v]][lv] * raw)
    res[["__ob_intercept__"]] <- res[["__ob_intercept__"]] + cc
    for (j in lv) res[paste0(v, "_", j)] <- raw[[j]] - cc
  }
  res[full_names(nums, cats, shares)]
}

# ---- the OB arithmetic, scheme-parametric ----
ob_table <- function(bA, bB, bs, xA, xB) {
  nm <- names(xA)
  ex <- (xA - xB) * bs[nm]
  un <- xA * (bA[nm] - bs[nm]) + xB * (bs[nm] - bB[nm])
  list(explained = ex, unexplained = un)
}

# One full dataset x convention case: every scheme, both routes.
run_case <- function(d, y, nums, cats, group_col, ref, w = NULL, equal = FALSE) {
  cols <- c(nums, cats, group_col)
  keep <- stats::complete.cases(d[, cols, drop = FALSE]) & !is.na(y)
  d <- d[keep, , drop = FALSE]; y <- y[keep]; if (!is.null(w)) w <- w[keep]
  shares <- setNames(lapply(cats, function(v) level_shares(d, v, w, equal)), cats)
  isA <- d[[group_col]] != ref
  dA <- d[isA, , drop = FALSE]; dB <- d[!isA, , drop = FALSE]
  yA <- y[isA]; yB <- y[!isA]; wA <- if (is.null(w)) NULL else w[isA]; wB <- if (is.null(w)) NULL else w[!isA]

  fit_eff <- function(dd, yy, ww, ind = NULL) {
    X <- make_design(dd, nums, cats, shares, ind)
    b <- fit_cols(X, yy, ww)
    list(b = b, eff = effects_from_wec(b, nums, cats, shares), ind = if (is.null(ind)) NA_real_ else b[["__ob_group_indicator__"]])
  }
  fA <- fit_eff(dA, yA, wA); fB <- fit_eff(dB, yB, wB)
  indv <- as.numeric(isA)
  fP  <- fit_eff(d, y, w, indv)
  X0 <- make_design(d, nums, cats, shares); fN <- list(eff = effects_from_wec(fit_cols(X0, y, w), nums, cats, shares))

  xA <- means_full(dA, nums, cats, shares, wA); xB <- means_full(dB, nums, cats, shares, wB)
  nA <- if (is.null(wA)) nrow(dA) else sum(wA); nB <- if (is.null(wB)) nrow(dB) else sum(wB)
  shareA <- nA / (nA + nB)
  schemes <- list(GroupA = fA$eff, GroupB = fB$eff,
                  Weighted = shareA * fA$eff + (1 - shareA) * fB$eff,
                  Pooled = fP$eff, PooledNoIndicator = fN$eff)
  gap <- (if (is.null(wA)) mean(yA) else sum(wA * yA) / sum(wA)) - (if (is.null(wB)) mean(yB) else sum(wB * yB) / sum(wB))

  res <- list()
  for (sc in names(schemes)) {
    tb <- ob_table(fA$eff, fB$eff, schemes[[sc]], xA, xB)
    stopifnot(abs(sum(tb$explained) + sum(tb$unexplained) - gap) < 1e-10)
    res[[sc]] <- list(
      detailed_explained = as.list(tb$explained), detailed_unexplained = as.list(tb$unexplained),
      explained = sum(tb$explained), unexplained = sum(tb$unexplained))
    if (sc == "Pooled") {
      res[[sc]]$indicator_coefficient <- fP$ind
      stopifnot(abs(sum(tb$unexplained) - fP$ind) < 1e-10)              # Elder, Goddeeris & Haider identity
    }
  }

  # route (2) must agree with route (1) at 1e-12
  tA <- treat_fit(dA, yA, nums, cats, wA); tB <- treat_fit(dB, yB, nums, cats, wB)
  tP <- treat_fit(d, y, nums, cats, w, indv); tN <- treat_fit(d, y, nums, cats, w)
  max_post <- max(
    abs(posthoc_effects(tA, nums, cats, shares) - fA$eff), abs(posthoc_effects(tB, nums, cats, shares) - fB$eff),
    abs(posthoc_effects(tP[names(tP) != "__ob_group_indicator__"], nums, cats, shares) - fP$eff),
    abs(posthoc_effects(tN, nums, cats, shares) - fN$eff))
  stopifnot(max_post < 1e-12)

  # three-fold from the TREATMENT-CODED vectors (invariant to the coding): k-1 dummies
  tn <- names(tA); xAt <- means_full(dA, nums, cats, shares, wA)[tn]
  # treatment design means: dummy columns of non-base levels, intercept, nums
  xAt <- xAt; xBt <- means_full(dB, nums, cats, shares, wB)[tn]
  E <- sum((xAt - xBt) * tB[tn]); C <- sum(xBt * (tA[tn] - tB[tn])); I <- sum((xAt - xBt) * (tA[tn] - tB[tn]))
  stopifnot(abs(E + C + I - gap) < 1e-10)

  list(shares = lapply(shares, as.list), n_a = nrow(dA), n_b = nrow(dB), total_gap = gap,
       schemes = res, three_fold_raw = list(endowments = E, coefficients = C, interaction = I),
       posthoc_vs_refit_max_abs_diff = max_post)
}

strip_internal <- function(x) x
tab_max_diff <- function(a, b, scheme) max(abs(unlist(a$schemes[[scheme]]$detailed_unexplained) - unlist(b$schemes[[scheme]]$detailed_unexplained)))

# ---- package blocks ----
pkg_dd <- function(d, y_name, nums, cats, group_col, ref, other) {
  d2 <- d; for (v in cats) d2[[v]] <- fct(d[[v]])
  grp <- factor(d[[group_col]], levels = c(ref, other))
  fml <- as.formula(paste(y_name, "~", paste(c(nums, cats), collapse = " + ")))
  out <- list()
  for (sc in c("GroupB", "GroupA")) {
    dd <- suppressMessages(ob_decompose(fml, data = d2, group = grp, reference_0 = (sc == "GroupB"), normalize_factors = TRUE, bootstrap = FALSE))
    tt <- dd$ob_decompose$decomposition_terms
    rows <- tt[tt$Variable != "Total", , drop = FALSE]
    nm <- vapply(rows$Variable, function(t) { if (t == "(Intercept)") return("__ob_intercept__")
      for (v in cats) if (startsWith(t, v)) return(paste0(v, "_", substring(t, nchar(v) + 1))); t }, character(1))
    tot <- tt[tt$Variable == "Total", ]
    out[[sc]] <- list(detailed_explained = as.list(setNames(rows$Composition_effect, nm)),
                      detailed_unexplained = as.list(setNames(rows$Structure_effect, nm)),
                      explained = tot$Composition_effect, unexplained = tot$Structure_effect, total_gap = tot$Observed_difference)
  }
  out
}
pkg_oaxaca_single <- function(d, y_name, nums, cat, group_col, ref) {
  d2 <- d; d2[[cat]] <- fct(d[[cat]]); levs <- levels(d2[[cat]])
  for (l in levs) d2[[paste0(cat, "_", l)]] <- as.numeric(d2[[cat]] == l)
  dums <- paste0(cat, "_", levs[-1])
  d2$zB <- as.numeric(d2[[group_col]] == ref)
  fml <- as.formula(paste(y_name, "~", paste(c(nums, dums), collapse = " + "), "| zB |", paste(dums, collapse = " + ")))
  wA <- sum(d2$zB == 0) / nrow(d2)
  res <- suppressMessages(oaxaca(fml, data = d2, R = 2, group.weights = c(0, 1, wA)))
  ov <- res$twofold$overall
  out <- list(group_weights = list(GroupB = 0, GroupA = 1, Weighted = wA))
  rowname <- function(r) if (r == "(Base)") paste0(cat, "_", levs[1]) else if (r == "(Intercept)") "__ob_intercept__" else sub(paste0("^", cat, "_"), paste0(cat, "_"), r)
  for (sc in c("GroupB", "GroupA", "Weighted")) {
    gw <- out$group_weights[[sc]]
    idx <- which(abs(ov[, "group.weight"] - gw) < 1e-12)[1]
    v <- res$twofold$variables[[idx]]
    nm <- vapply(rownames(v), rowname, character(1))
    out[[sc]] <- list(detailed_explained = as.list(setNames(v[, "coef(explained)"], nm)),
                      detailed_unexplained = as.list(setNames(v[, "coef(unexplained)"], nm)),
                      explained = ov[idx, "coef(explained)"], unexplained = ov[idx, "coef(unexplained)"])
  }
  # raw aggregate rows for the pooled schemes (R oaxaca does NOT re-estimate beta* for these; aggregates only)
  for (nmw in c("PooledNoIndicator", "Pooled")) {
    gw <- if (nmw == "PooledNoIndicator") -1 else -2
    idx <- which(abs(ov[, "group.weight"] - gw) < 1e-12)[1]
    out[[paste0("aggregate_", nmw)]] <- list(explained = ov[idx, "coef(explained)"], unexplained = ov[idx, "coef(unexplained)"])
  }
  tf <- res$threefold
  out$threefold_overall <- list(endowments = unname(tf$overall[["coef(endowments)"]]), coefficients = unname(tf$overall[["coef(coefficients)"]]),
                                interaction = unname(tf$overall[["coef(interaction)"]]))
  out
}
cmp_pkg_to_case <- function(pkg_sc, case_sc) {
  a <- unlist(pkg_sc$detailed_explained); b <- unlist(case_sc$detailed_explained)
  stopifnot(setequal(names(a), names(b)))
  u1 <- unlist(pkg_sc$detailed_unexplained); u2 <- unlist(case_sc$detailed_unexplained)
  max(abs(a - b[names(a)]), abs(u1 - u2[names(u1)]))
}

# =============================================================================
emp <- read_fix(EMPLOYERS); sk <- read_fix(SKEWED); ba <- read_fix(BALANCED)
for (nm in c("Education_Level", "Department", "Location")) stopifnot(!any(grepl(".", unique(emp[[nm]]), fixed = TRUE)))
EMP_NUMS <- c("Age", "Experience_Years"); EMP_CATS <- c("Education_Level", "Department", "Location")
SK_NUMS <- c("Age", "Experience_Years"); SK_CATS <- c("Department", "Location")
BA_CATS <- c("Department", "Location", "Union")

cases <- list(); anchor <- list()

# ---- employers (10k, near-balanced): equal-share anchors the arithmetic to the packages ----
emp_eq <- run_case(emp, emp$log_salary, EMP_NUMS, EMP_CATS, "Gender", "Female", equal = TRUE)
pk_emp <- pkg_dd(emp, "log_salary", EMP_NUMS, EMP_CATS, "Gender", "Female", "Male")
anchor$employers_vs_ddecompose <- max(cmp_pkg_to_case(pk_emp$GroupB, emp_eq$schemes$GroupB), cmp_pkg_to_case(pk_emp$GroupA, emp_eq$schemes$GroupA))
stopifnot(anchor$employers_vs_ddecompose < 1e-9)
cases$employers_equal <- strip_internal(emp_eq)
cases$employers_popshare <- strip_internal(run_case(emp, emp$log_salary, EMP_NUMS, EMP_CATS, "Gender", "Female"))

# R oaxaca on a single categorical (the only model oaxaca() can normalise): Department only
emp_dept_eq <- run_case(emp, emp$log_salary, EMP_NUMS, "Department", "Gender", "Female", equal = TRUE)
pk_ox <- pkg_oaxaca_single(emp, "log_salary", EMP_NUMS, "Department", "Gender", "Female")
anchor$employers_dept_vs_oaxaca <- max(cmp_pkg_to_case(pk_ox$GroupB, emp_dept_eq$schemes$GroupB), cmp_pkg_to_case(pk_ox$GroupA, emp_dept_eq$schemes$GroupA),
                                       cmp_pkg_to_case(pk_ox$Weighted, emp_dept_eq$schemes$Weighted))
stopifnot(anchor$employers_dept_vs_oaxaca < 1e-9)
stopifnot(abs(pk_ox$aggregate_Pooled$unexplained - emp_dept_eq$schemes$Pooled$unexplained) < 1e-9,
          abs(pk_ox$aggregate_PooledNoIndicator$unexplained - emp_dept_eq$schemes$PooledNoIndicator$unexplained) < 1e-9)
stopifnot(abs(pk_ox$threefold_overall$endowments - emp_dept_eq$three_fold_raw$endowments) < 1e-9)
cases$employers_dept_equal <- strip_internal(emp_dept_eq)
cases$employers_dept_popshare <- strip_internal(run_case(emp, emp$log_salary, EMP_NUMS, "Department", "Gender", "Female"))

# carve-out: the first 20 Engineering rows (file order) become a new "Legal" department
emp_legal <- emp; eng_rows <- which(emp$Department == "Engineering")[1:20]; emp_legal$Department[eng_rows] <- "Legal"
cases$employers_legal_dept_popshare <- strip_internal(run_case(emp_legal, emp_legal$log_salary, EMP_NUMS, "Department", "Gender", "Female"))
cases$employers_legal_dept_equal <- strip_internal(run_case(emp_legal, emp_legal$log_salary, EMP_NUMS, "Department", "Gender", "Female", equal = TRUE))

# ---- skewed: the fixture the share source can be told apart on ----
sk_pop <- run_case(sk, sk$log_salary, SK_NUMS, SK_CATS, "Gender", "Female")
sk_eq  <- run_case(sk, sk$log_salary, SK_NUMS, SK_CATS, "Gender", "Female", equal = TRUE)
cases$skewed_popshare <- strip_internal(sk_pop); cases$skewed_equal <- strip_internal(sk_eq)
# the gate must be able to fail: every share source gives a different table on this fixture
anchor$skewed_popshare_vs_equal_max_abs <- tab_max_diff(sk_pop, sk_eq, "GroupB")
stopifnot(anchor$skewed_popshare_vs_equal_max_abs > 1e-3)
# permuted share vector (planted wrong value): re-run with the Department shares reversed
perm_run <- (function() {
  d <- sk; w <- NULL; shares <- setNames(lapply(SK_CATS, function(v) level_shares(d, v, w, FALSE)), SK_CATS)
  shares$Department[] <- rev(shares$Department)                         # same numbers, wrong levels
  isA <- d$Gender != "Female"; dA <- d[isA, ]; dB <- d[!isA, ]
  fe <- function(dd, yy) effects_from_wec(fit_cols(make_design(dd, SK_NUMS, SK_CATS, shares), yy), SK_NUMS, SK_CATS, shares)
  bA <- fe(dA, d$log_salary[isA]); bB <- fe(dB, d$log_salary[!isA])
  tb <- ob_table(bA, bB, bB, means_full(dA, SK_NUMS, SK_CATS, shares), means_full(dB, SK_NUMS, SK_CATS, shares))
  unlist(tb$unexplained)
})()
anchor$skewed_permuted_shares_max_abs <- max(abs(perm_run - unlist(sk_pop$schemes$GroupB$detailed_unexplained)[names(perm_run)]))
stopifnot(anchor$skewed_permuted_shares_max_abs > 1e-3)

# integer weights: weighted == lm() on the expanded rows
sk_w <- run_case(sk, sk$log_salary, SK_NUMS, SK_CATS, "Gender", "Female", w = sk$w)
ex_idx <- rep(seq_len(nrow(sk)), sk$w)
sk_x <- run_case(sk[ex_idx, ], sk$log_salary[ex_idx], SK_NUMS, SK_CATS, "Gender", "Female")
anchor$weighted_vs_expanded_rows <- max(vapply(names(sk_w$schemes), function(sc) cmp_pkg_to_case(sk_x$schemes[[sc]], sk_w$schemes[[sc]]), numeric(1)))
stopifnot(anchor$weighted_vs_expanded_rows < 1e-9)
cases$skewed_weighted_popshare <- strip_internal(sk_w)
cases$skewed_weighted_equal <- strip_internal(run_case(sk, sk$log_salary, SK_NUMS, SK_CATS, "Gender", "Female", w = sk$w, equal = TRUE))

# a row dropped for a blank in a non-outcome predictor (Tenure): shares come from the model.frame rows
sk_t <- run_case(sk, sk$log_salary, c(SK_NUMS, "Tenure"), SK_CATS, "Gender", "Female")
mf_rows <- stats::complete.cases(sk[, c("log_salary", SK_NUMS, "Tenure", SK_CATS, "Gender")])
anchor$dropped_rows <- sum(!mf_rows)
stopifnot(anchor$dropped_rows == 6)
mf <- sk[mf_rows, ]
stopifnot(isTRUE(all.equal(unlist(sk_t$shares$Department), unlist(as.list(level_shares(mf, "Department"))))))
cases$skewed_dropped_popshare <- strip_internal(sk_t)

# weights AND dropped rows together (E-REV F2): the share counts must be the sum of the weights of
# the rows that survive cleaning. Blank Tenure removes Eng x4 and Admin x2 (weights 1,1,1,3,3,2).
sk_wt <- run_case(sk, sk$log_salary, c(SK_NUMS, "Tenure"), SK_CATS, "Gender", "Female", w = sk$w)
mf_keep <- stats::complete.cases(sk[, c("log_salary", SK_NUMS, "Tenure", SK_CATS, "Gender")])
stopifnot(sum(!mf_keep) == 6, sum(sk$w[!mf_keep]) > 0)
# weighted-and-dropped == lm() on the surviving rows repeated by their weights
ex_keep <- rep(which(mf_keep), sk$w[mf_keep])
sk_wtx <- run_case(sk[ex_keep, ], sk$log_salary[ex_keep], c(SK_NUMS, "Tenure"), SK_CATS, "Gender", "Female")
anchor$weighted_dropped_vs_expanded_rows <- max(vapply(names(sk_wt$schemes), function(sc) cmp_pkg_to_case(sk_wtx$schemes[[sc]], sk_wt$schemes[[sc]]), numeric(1)))
stopifnot(anchor$weighted_dropped_vs_expanded_rows < 1e-9)
# the gate can fail: weights taken over the PRE-cleaning frame give other shares, so another table
pre <- unlist(level_shares(sk, "Department", sk$w)); post <- unlist(level_shares(sk[mf_keep, ], "Department", sk$w[mf_keep]))
anchor$weighted_shares_pre_vs_post_cleaning_max_abs <- max(abs(pre - post[names(pre)]))
stopifnot(anchor$weighted_shares_pre_vs_post_cleaning_max_abs > 1e-3)
# ...and the unweighted count of the surviving rows is another source again
post_rows <- unlist(level_shares(sk[mf_keep, ], "Department"))
anchor$weighted_vs_row_count_shares_max_abs <- max(abs(post - post_rows[names(post)]))
stopifnot(anchor$weighted_vs_row_count_shares_max_abs > 1e-3)
cases$skewed_weighted_dropped <- strip_internal(sk_wt)

# ---- balanced pooled / unbalanced within group: population-share == equal-share == ddecompose ----
ba_pop <- run_case(ba, ba$log_salary, SK_NUMS, BA_CATS, "Gender", "Female")
ba_eq  <- run_case(ba, ba$log_salary, SK_NUMS, BA_CATS, "Gender", "Female", equal = TRUE)
anchor$balanced_pop_vs_equal <- max(vapply(names(ba_pop$schemes), function(sc) cmp_pkg_to_case(ba_pop$schemes[[sc]], ba_eq$schemes[[sc]]), numeric(1)))
stopifnot(anchor$balanced_pop_vs_equal < 1e-12)
pk_ba <- pkg_dd(ba, "log_salary", SK_NUMS, BA_CATS, "Gender", "Female", "Male")
anchor$balanced_vs_ddecompose <- max(cmp_pkg_to_case(pk_ba$GroupB, ba_pop$schemes$GroupB), cmp_pkg_to_case(pk_ba$GroupA, ba_pop$schemes$GroupA))
stopifnot(anchor$balanced_vs_ddecompose < 1e-9)
cases$balanced_popshare <- strip_internal(ba_pop)

# ---- RIF path: the engine's own per-group RIF column as an ordinary outcome ----
rif <- list(available = FALSE)
if (file.exists(RIFCSV)) {
  rf <- read_fix(RIFCSV)
  rif <- list(available = TRUE, source = "norm_skewed_rif.csv")
  for (tau in c("q10", "q50", "q90")) {
    y <- rf[[paste0("rif_", tau)]]
    pk <- pkg_dd(transform(rf, y_ = y), "y_", SK_NUMS, SK_CATS, "Gender", "Female", "Male")
    ce <- run_case(rf, y, SK_NUMS, SK_CATS, "Gender", "Female", equal = TRUE)
    anchor[[paste0("rif_", tau, "_vs_ddecompose")]] <- max(cmp_pkg_to_case(pk$GroupB, ce$schemes$GroupB), cmp_pkg_to_case(pk$GroupA, ce$schemes$GroupA))
    stopifnot(anchor[[paste0("rif_", tau, "_vs_ddecompose")]] < 1e-9)
    rif[[paste0(tau, "_equal")]] <- strip_internal(ce)
    rif[[paste0(tau, "_popshare")]] <- strip_internal(run_case(rf, y, SK_NUMS, SK_CATS, "Gender", "Female"))
  }
}

golden <- list(
  `_meta` = list(
    generated_utc = format(Sys.time(), tz = "UTC", usetz = TRUE),
    r_version = as.character(getRversion()),
    packages = list(oaxaca = as.character(packageVersion("oaxaca")), ddecompose = as.character(packageVersion("ddecompose")),
                    jsonlite = as.character(packageVersion("jsonlite")), digest = as.character(packageVersion("digest"))),
    seed = SEED, rngkind = paste(RNGkind(), collapse = ","),
    generator_sha256 = sha(this_file),
    fixture_sha256 = list(
      employers_trust_fixture.csv = sha(EMPLOYERS), norm_skewed_fixture.csv = sha(SKEWED), norm_balanced_fixture.csv = sha(BALANCED),
      norm_skewed_rif.csv = if (file.exists(RIFCSV)) sha(RIFCSV) else NA_character_),
    conventions = list(`population-share` = "level share of the pooled analysed rows (sum of weights when weighted), base level included",
                       `equal-share` = "1/m per level: Stata categorical(), R oaxaca third formula part, ddecompose normalize_factors=TRUE"),
    scheme_names = c("GroupA", "GroupB", "Pooled", "PooledNoIndicator", "Weighted"),
    group_a = "Male (non-reference)", group_b = "Female (reference)",
    tolerances = list(package_equal_share = 1e-10, refit_population_share = 1e-9, identity = 1e-12),
    cross_checks_passed_in_generator = anchor,
    expected_values_from_engine_output = FALSE
  ),
  packages = list(employers_ddecompose_equal = pk_emp, balanced_ddecompose_equal = pk_ba, employers_dept_oaxaca_equal = pk_ox),
  cases = cases,
  rif = rif
)
writeLines(toJSON(golden, auto_unbox = TRUE, digits = I(17), pretty = TRUE, null = "null", na = "null"), GOLDEN)
cat("wrote", GOLDEN, "\n")
for (k in names(anchor)) cat(sprintf("  %-40s %.3e\n", k, anchor[[k]]))
