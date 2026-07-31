# Picomatch Mortis

A from-scratch Rust port of
[`micromatch/picomatch`](https://github.com/micromatch/picomatch), an official
Port Mortem 2026 pool repository.

- Language pair: JavaScript to Rust
- Kickoff: 2026-07-31 18:00 UTC
- Upstream commit: `4f41a8edade7a5ab19832f7b40ecce46b288767f`
- Upstream scope: 2,444 JavaScript source lines and 37 test files
- Goal: full glob behavior proved through unchanged upstream tests

This repository and every port commit were created after kickoff. The Rust
crate forbids unsafe code; implementation lands in small, auditable slices.
