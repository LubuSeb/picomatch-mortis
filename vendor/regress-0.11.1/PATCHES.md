# Local patches

This directory starts from `regress` 0.11.1 and remains under its upstream
MIT/Apache-2.0 license. Picomatch Mortis carries the source as a path dependency
because matching safety and legacy JavaScript case-folding are part of the
submission's observable contract.

The local delta is deliberately narrow:

- replace syntax newer than Rust 1.85 without changing behavior;
- correct an upstream-reversed `prohibit-unsafe` branch in Unicode folding;
- add the exact non-Unicode ECMAScript `Canonicalize` table derived from pinned
  Node 24.18.0
  for BMP code units;
- thread one deterministic execution-fuel counter through nested lookarounds;
- charge dispatch, backtracking, optimized scans, and backreference comparison;
- expose fuel exhaustion to callers instead of reporting a silent non-match;
- test that exhaustion preserves the backtracking-stack invariant.

The repository CI runs this crate's complete upstream test suite on Ubuntu and
Windows in addition to the port's compatibility proof.
