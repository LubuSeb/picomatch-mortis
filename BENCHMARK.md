# Benchmark report

This report separates direct native matching from the JavaScript compatibility
bridge and compares both with the pinned Picomatch reference. It is a
reproducible cost measurement, not a claim that the Rust port is faster.

## Environment

- Date: 2026-08-01
- OS: Windows 11, build 10.0.26200
- CPU: AMD Ryzen 9 3950X
- Node.js: 24.18.0
- Active rustc: 1.97.1
- Build profile: release
- Reference: `micromatch/picomatch` commit `4f41a8edade7a5ab19832f7b40ecce46b288767f`

## Method

Every path rotates through the same four input/pattern pairs: a globstar, a
negative extglob, a three-digit brace range, and an extglob suffix. Patterns
are compiled before timing, and every run returns 75% matches.

- Reference and direct native: 10,000 warm-up calls, then 1,000,000 matches.
- Proof bridge: 1,000 warm-up calls, then 25,000 synchronous matches.
- Three independent timed runs per path; the reported result is the median.

Reproduce all three paths after installing the locked dependencies:

```sh
npm ci
npm run bench:reference
npm run bench
npm run bench:bridge
```

`bench:reference` uses the pinned package and its precompiled matcher under
Node. `bench` calls a precompiled `GlobPattern` directly in Rust. `bench:bridge`
includes the synchronous adapter, worker, typed IPC, cache lookup and native
match.

## Results

| Path | Timed calls/run | Run 1 | Run 2 | Run 3 | Median |
| --- | ---: | ---: | ---: | ---: | ---: |
| Pinned Picomatch / Node.js | 1,000,000 | 7,184,496 ops/s | 7,153,342 ops/s | 7,255,715 ops/s | **7,184,496 ops/s** |
| Rust API, direct native | 1,000,000 | 395,936 ops/s | 395,048 ops/s | 393,696 ops/s | **395,048 ops/s** |
| Rust through proof bridge | 25,000 | 9,855 ops/s | 11,602 ops/s | 12,144 ops/s | **11,602 ops/s** |

## Interpretation

Picomatch is about 18.2x faster than the direct native path in this
microbenchmark. It benefits from V8's optimized RegExp engine; the Rust port
uses the safe, ECMAScript-compatible `regress` interpreter, including explicit
execution-fuel bookkeeping. The synchronous proof bridge is another roughly
34x below direct native throughput because worker wakeups, framing, IPC and JS
API reconstruction dominate such tiny matches.

These numbers establish three useful facts: the native matcher is practical at
roughly 0.40 million matches/second for this workload, the compatibility bridge
has a visible and measured cost, and performance is not this submission's win
claim. Its primary result is behavior preservation backed by reproducible
provenance, differential testing and explicit failure boundaries.
