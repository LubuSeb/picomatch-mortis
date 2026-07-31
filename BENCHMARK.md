# Benchmark report

This report separates native matcher throughput from the JavaScript proof
bridge. It is a reproducible local measurement, not a claim that the Rust port
is faster than Picomatch.

## Environment

- Date: 2026-07-31
- OS: Windows 11 Home 64-bit, build 10.0.26200
- CPU: AMD Ryzen 9 3950X, 16 cores / 32 logical processors
- Rust: 1.97.1
- Node.js: 24.13.0
- Reference: `micromatch/picomatch` commit `4f41a8edade7a5ab19832f7b40ecce46b288767f`

## Method

All measurements rotate through the same four input/pattern pairs: a globstar,
a negative extglob, a three-digit brace range, and an extglob suffix. Patterns
are compiled before timing. The native and reference loops each warm up for
10,000 calls and then perform 1,000,000 matches. The bridge measurement warms
up for 1,000 calls and performs 25,000 synchronous calls.

```sh
cargo build --release
npm run bench
npm run bench:bridge
```

The pinned JavaScript reference comparison uses the equivalent precompiled
matcher loop under Node.js 24.

## Results

| Path | Iterations | Throughput | What it measures |
| --- | ---: | ---: | --- |
| Pinned Picomatch / Node.js | 1,000,000 | 7,967,029 ops/s | Precompiled V8 RegExp matching |
| Rust API, direct native (median of 3) | 1,000,000 | 590,841 ops/s | Precompiled `GlobPattern::is_match` |
| Rust through synchronous proof bridge | 25,000 | 8,118 ops/s | Adapter + worker + IPC + native matching |

The three direct-native Rust runs were 576,019, 590,841, and 593,610 ops/s.
Each path returned 75% matches for the same rotating cases.

## Interpretation

Picomatch is substantially faster in this microbenchmark. The Rust port uses
the safe, ECMAScript-compatible `regress` interpreter, while the reference
benefits from V8's optimized RegExp engine. The persistent bridge is designed
for synchronous compatibility proof, not production throughput, and its IPC
cost dominates.

The performance result is therefore not a win claim. It establishes that the
native matcher is usable at roughly 0.59 million matches per second on this
machine, makes the bridge overhead explicit, and leaves optimization as honest
future work. Behavioral preservation and reproducible proof are this entry's
primary result.
