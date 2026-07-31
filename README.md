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

## Proof status

- Upstream scanner and core glob compiler/matcher: implemented in safe Rust
- Unchanged upstream proof: **251/251 passing** across 15 original test files
- Covered behavior includes literals, stars, question marks, globstars,
  braces, brackets, dotfiles, negation, POSIX classes, Windows/POSIX paths,
  strict syntax errors, special characters, and option aliases
- Frozen upstream test snapshot: 38 files, verified against canonical SHA-256
  hashes before every JavaScript proof run

Run the current proof locally:

```sh
npm ci
npm test
```

The adapter contains no glob or scanner logic. It keeps one native Rust
process alive, serializes synchronous calls, and reconstructs the
JavaScript-shaped API result. Rust-generated regex sources are used for the
upstream `parse` and `makeRe` API checks.
