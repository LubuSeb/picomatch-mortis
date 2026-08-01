# Picomatch Mortis

[![proof](https://github.com/LubuSeb/picomatch-mortis/actions/workflows/ci.yml/badge.svg)](https://github.com/LubuSeb/picomatch-mortis/actions/workflows/ci.yml)

[Benchmark report](BENCHMARK.md) | [2-3 minute demo runbook](DEMO_SCRIPT.md)

Picomatch Mortis is a standalone Rust implementation of
[`micromatch/picomatch`](https://github.com/micromatch/picomatch)'s scanner,
compiler, and matcher. Its CLI runs without Node, and its proof harness has no
JavaScript fallback glob matcher. If deterministic regex fuel is exhausted, the
native API returns an explicit, recoverable error rather than `false`, and the
same persistent process handles the next ordinary match.

The project was selected from the official Port Mortem 2026 pool for **Track F:
JavaScript to Go/Rust**. Glob compatibility is consequential: one edge-case
mismatch can select the wrong files in a build, test, or packaging pipeline.

## Result

- **1,977/1,977 unchanged upstream tests pass** from a SHA-256-locked snapshot
  of 38 files at Picomatch commit `4f41a8e`, alongside **28 native Rust tests**.
- Five fixed differential seeds exercise **100,000 generated comparisons plus
  535 directed executions: 0 mismatches**.
- A separate legacy-ignore-case proof checks all **1,169 nonidentity BMP mappings**:
  2,392 ordered equivalences and 4,784 literal/class checks in 38 batches.
- The full vendored regex-engine suite passes, Rust 1.85 Clippy is clean, and
  `npm audit` reports zero vulnerabilities.
- CI runs the proof on Ubuntu and Windows. The crate uses
  `#![forbid(unsafe_code)]`, and its regex engine uses `prohibit-unsafe`.

The native port covers scan, compile, match and capture behavior: literals,
stars, qmarks, globstars, braces, ranges, brackets, POSIX classes, dotfiles,
path negation, Windows/POSIX separators, lookarounds, backreferences, every
extglob operator, and Picomatch's supported regex flags. Native UTF-16 spans
are reconstructed by the proof adapter as real RegExp-compatible match arrays,
including named groups and `d` indices, with `g`/`y` state behavior preserved.

## Judge it in 60 seconds

| Claim | Reproducible evidence |
| --- | --- |
| The tests are really upstream's | `npm run verify:upstream` fetches commit `4f41a8e` and byte-compares the complete 38-file test tree after LF normalization |
| Frozen behavior is preserved | `npm test` verifies the snapshot, runs 28 native tests, then passes all 1,977 unchanged upstream tests and the adapter checks |
| Broad generated behavior agrees | `npm run fuzz:ci` runs five fixed seeds x 20,000 generated comparisons plus 107 directed cases per seed: 0 mismatches |
| Legacy JavaScript case folding is exact | `npm run test:casefold` exhaustively checks the Node 24.18.0-derived 1,169-entry legacy BMP mapping through literal and class execution |
| Failure stays safe and visible | `npm run test:bridge-timeout` proves bounded bridge teardown; native regex work exhaustion returns a recoverable error rather than a false non-match |
| Dependencies and native code are clean | `npm audit --audit-level=high`, the vendored `regress` suite, and Rust 1.85 Clippy all pass |

## Reproduce it

Requirements: Rust 1.85 or newer, the proof-pinned Node.js 24.18.0, and Git.

```sh
npm ci
cargo build --release
npm run verify:upstream
npm test
npm run test:casefold
npm run fuzz:ci
npm run test:bridge-timeout
npm run demo
```

`npm test` checks the frozen manifest before running the native and unchanged
upstream suites. `verify:upstream` adds the network-backed provenance check.
For the three benchmark paths:

```sh
npm run bench:reference
npm run bench
npm run bench:bridge
```

## Use the Rust API

```rust
use picomatch_mortis::{GlobOptions, is_match};

let matched = is_match(
    "src/parser/glob.rs",
    "src/**/*.rs",
    GlobOptions::default(),
)?;
assert!(matched);
# Ok::<(), picomatch_mortis::GlobError>(())
```

The repository also includes a CLI and a persistent typed protocol used by the
proof harness. `--payload` keeps pattern/input data distinct from options:

```sh
cargo run --release --quiet -- is-match --payload "src/**/*.rs" "src/glob.rs"
cargo run --release --quiet -- source --payload "!(*.test).js"
cargo run --release --quiet -- scan --parts --tokens --payload "src/**/*.rs"
```

## Architecture

```text
unchanged JavaScript tests
        |
        v
thin JS adapter -- callbacks, API shortcuts, RegExp reconstruction
        |
        v
typed persistent bridge -- framing and lifecycle, no fallback matcher
        |
        v
safe Rust scanner + compiler + matcher
        |
        `--> patched, vendored regress 0.11.1 engine
```

The scanner is O(n): one structural pass plus one linear UTF-16 index-conversion
pass. The compiler emits a public ECMAScript-compatible source and a private
native execution form where those responsibilities differ. The patched `regress` 0.11.1 engine supplies native
execution, exact Node 24.18.0-derived legacy non-`u` case folding, and deterministic
work fuel across dispatch, backtracking, optimized scans, backreferences and
lookarounds.

The bridge is deliberately boring: typed hex framing, bounded buffers,
sequence-checked responses, and explicit close/timeout behavior. JavaScript
still orchestrates values that only exist in its API, such as callbacks,
arrays and `RegExp` objects. It also preserves Picomatch's explicit empty-input,
exact-input and invalid-source shortcuts; every compiled-pattern search lives
in Rust.

## Hard parts

1. **Extglob structure.** Nested negation, adjacent extglobs, slash-containing
   bodies, Bash backtracking and suffix-sensitive lookaheads required a real
   compiler rather than string replacement.
2. **JavaScript regex fidelity.** Legacy ignore-case behavior is neither ASCII
   folding nor modern Unicode folding, so the engine uses a generated table
   derived from the pinned Node 24.18.0 runtime and exhaustively rechecks it.
3. **Captures and flags.** Match spans use JavaScript UTF-16 offsets, absent
   captures stay absent, named groups and `d` indices survive the bridge, and
   `d/i/m/s/u/v/g/y` semantics are covered without moving glob logic into JS.
4. **Adversarial safety.** Compilation and execution have separate structural
   and work budgets. Execution fuel is shared through lookarounds, so costly
   expressions cannot reset the allowance by entering a nested assertion.
5. **Cross-platform paths.** Windows normalization, separator boundaries and
   Picomatch's unusual long-backslash behavior are verified on both operating
   systems in CI.

## Safety boundaries and honest scope

Native compilation enforces these hard limits:

- 65,536 UTF-16 code units per pattern
- nesting depth 64
- 1,024 alternation branches per scope
- 512 unmatched bracket markers
- a pattern-proportional compile-work budget
- a 4 MiB proof-bridge buffer

The proof bridge explicitly rejects lone UTF-16 surrogates instead of silently
replacing them. Completed native matches preserve the tested semantics; if
deterministic regex fuel is exhausted, the native API returns an explicit,
recoverable error rather than `false`. A `RegExp` obtained from `makeRe` and
then executed directly by caller JavaScript runs in the caller's engine and
therefore does not inherit the native fuel limit.

Public regex source is API-visible and can differ structurally from the
reference, especially for complex globstars, even where observed execution and
captures agree. `scan`, `parse`, `makeRe`, options, callbacks and match arrays
are covered by the frozen suites and focused checks; the randomized corpus is
a broad bounded Boolean-match comparison, not a proof over the entire input
space.

## Provenance

- Kickoff: 2026-07-31 18:00 UTC
- Upstream commit: [`4f41a8e`](https://github.com/micromatch/picomatch/tree/4f41a8edade7a5ab19832f7b40ecce46b288767f)
- Upstream implementation: 2,444 JavaScript source lines
- First port commit: [`eb06bd8`](https://github.com/LubuSeb/picomatch-mortis/commit/eb06bd8), created after kickoff
- Incremental history: scanner, core matcher, multi-OS proof, hardening, full
  extglob/capture/flag parity, then deterministic execution limits

The local direct-native benchmark reaches a median **395,048 matches/second**
for four precompiled representative patterns. Picomatch is much faster in the
same microbenchmark; the result is a transparent cost measurement, not a
cross-language speed claim. See [BENCHMARK.md](BENCHMARK.md) for all runs and
methodology, [WRITEUP.md](WRITEUP.md) for the porting narrative,
[DECISIONS.md](DECISIONS.md) for the engineering log, and
[DEMO_SCRIPT.md](DEMO_SCRIPT.md) for the judge demo.
