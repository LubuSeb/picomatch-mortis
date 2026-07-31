use std::hint::black_box;
use std::time::Instant;

use picomatch_mortis::{GlobOptions, GlobPattern};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ("src/parser/glob.rs", "src/**/*.rs"),
        ("packages/core/index.test.js", "**/!(*.test).js"),
        ("release-042.txt", "release-{0..9}{0..9}{0..9}.txt"),
        ("foo/bar/baz.jsx", "foo/bar/**/*.+(js|jsx)"),
    ];
    let compiled = cases
        .iter()
        .map(|(_, pattern)| GlobPattern::new(pattern, GlobOptions::default()))
        .collect::<Result<Vec<_>, _>>()?;

    for index in 0..10_000 {
        black_box(compiled[index % compiled.len()].is_match(cases[index % cases.len()].0));
    }

    let iterations = 1_000_000usize;
    let mut matches = 0usize;
    let started = Instant::now();
    for index in 0..iterations {
        if black_box(compiled[index % compiled.len()].is_match(cases[index % cases.len()].0)) {
            matches += 1;
        }
    }
    let elapsed = started.elapsed();
    let operations_per_second = iterations as f64 / elapsed.as_secs_f64();

    println!("compiled patterns: {}", compiled.len());
    println!("iterations: {iterations}");
    println!("matches: {matches}");
    println!("elapsed: {elapsed:.2?}");
    println!("operations/second: {operations_per_second:.0}");
    Ok(())
}
