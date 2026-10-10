//! 0122-MERIDIAN: the committed files the remedy oracles read.
//!
//! `fixtures/0122-fixture-f-noisy.csv` is Fixture F with reference pay off the formula by a known
//! residual (`FixtureF::noisy()`), 60 reference and 40 compared rows and no blank cell. The
//! committed Fixture F (`0118-fixture-f.csv`) has an exact reference line, so every prediction
//! interval on it has zero width; the remedy's range, position and group-test oracles need a line
//! with residual variance. `fixtures/0122-remedy-tiny.csv` is six rows with an exact reference
//! line, small enough to price by hand.
//!
//! This test fails if the committed noisy file and the builder drift apart. To regenerate after
//! an intentional change:
//!
//!   MERIDIAN_WRITE_FIXTURE=1 cargo test -p pay-equity-engine --test remedy_fixture_test

mod support;

use support::FixtureF;

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn the_committed_noisy_csv_is_what_the_builder_produces() {
    let want = FixtureF::noisy().csv();
    if std::env::var("MERIDIAN_WRITE_FIXTURE").as_deref() == Ok("1") {
        std::fs::write(path("0122-fixture-f-noisy.csv"), &want).unwrap();
    }
    let got = std::fs::read_to_string(path("0122-fixture-f-noisy.csv")).expect(
        "fixtures/0122-fixture-f-noisy.csv must exist (regenerate with MERIDIAN_WRITE_FIXTURE=1)",
    );
    assert_eq!(got, want, "committed noisy fixture drifted from FixtureF");
    assert!(got.starts_with("Name,Salary,Gender,Experience,Level\n"));
    assert_eq!(got.lines().count(), support::N_ROWS + 1);
}

#[test]
fn the_tiny_remedy_fixture_is_the_six_rows_the_hand_figures_use() {
    let got = std::fs::read_to_string(path("0122-remedy-tiny.csv"))
        .expect("fixtures/0122-remedy-tiny.csv must exist");
    let want = "wage,group,x\n\
                40000,R,0\n\
                45000,R,5\n\
                50000,R,10\n\
                40000,T,2\n\
                47000,T,6\n\
                49000,T,12\n";
    assert_eq!(got, want);
}
