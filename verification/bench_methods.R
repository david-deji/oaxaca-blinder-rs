#!/usr/bin/env Rscript
# Rust-vs-R timing harness (R side). Serial, warm-up discarded, median of N.
# Same fixture + model as the engine. NOT a golden — a benchmark. (0014-MERIDIAN perf Q.)
suppressWarnings(suppressMessages({ library(quantreg); library(ddecompose) }))
FX <- file.path(dirname(sub("--file=", "", grep("--file=", commandArgs(FALSE), value=TRUE)[1])),
                "..", "oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv")
fx <- read.csv(FX, stringsAsFactors = FALSE)
for (c in c("Education_Level","Department","Location","Gender")) fx[[c]] <- factor(fx[[c]])
fml <- log_salary ~ Age + Experience_Years + Education_Level + Department + Location
A <- droplevels(fx[fx$Gender=="Male",]); B <- droplevels(fx[fx$Gender=="Female",])

# mean OB (GroupB) via lm + arithmetic — the golden's method
ob_point <- function(a, b) {
  fa<-lm(fml,a); fb<-lm(fml,b); ma<-model.matrix(fml,a); mb<-model.matrix(fml,b)
  tr<-colnames(ma); ca<-coef(fa)[tr]; cb<-coef(fb)[tr]; xa<-colMeans(ma); xb<-colMeans(mb)
  ex<-sum((xa-xb)*cb); un<-sum(xa*(ca-cb)); c(ex,un)
}
bench <- function(expr, n=7, warm=1) {
  # substitute+eval re-runs the expression each iteration; a plain promise would memoize
  # (evaluate once, then return the cached value -> 0 ms timings).
  e <- substitute(expr); p <- parent.frame()
  for (i in seq_len(warm)) eval(e, p)
  t <- vapply(seq_len(n), function(i) system.time(eval(e, p))[["elapsed"]], numeric(1))
  median(t) * 1000
}

r1 <- bench(ob_point(A, B))
r2 <- bench({ set.seed(1); for (r in 1:100) { ia<-sample.int(nrow(A),replace=TRUE)
  ib<-sample.int(nrow(B),replace=TRUE); ob_point(A[ia,], B[ib,]) } }, n=3)
r3 <- bench(ddecompose::ob_decompose(fml, data=fx, group=fx$Gender, reweighting=FALSE,
  normalize_factors=FALSE, rifreg_statistic="quantiles", rifreg_probs=0.5, bootstrap=FALSE), n=3)
r4 <- bench(coef(rq(log_salary ~ Age, tau=0.5, data=fx)))

cat(sprintf("R_mean_ob_point_ms %.2f\n", r1))
cat(sprintf("R_mean_ob_boot100_ms %.2f\n", r2))
cat(sprintf("R_rif_quantile_point_ms %.2f\n", r3))
cat(sprintf("R_qr_tau50_ms %.2f\n", r4))
