# Picomatch Mortis

[![proof](https://github.com/LubuSeb/picomatch-mortis/actions/workflows/ci.yml/badge.svg)](https://github.com/LubuSeb/picomatch-mortis/actions/workflows/ci.yml)

A from-scratch JavaScript-to-Rust port of
[`micromatch/picomatch`](https://github.com/micromatch/picomatch), selected from
the official Port Mortem 2026 pool. **Track F: JavaScript to Go/Rust.**
Glob compatibility is deceptively consequential: one edge-case mismatch can
select the wrong files in a build, test, or packaging pipeline.

## Result

- **1,977/1,977 unchanged upstream tests pass** across all 36 executable
  suites, plus 15 native Rust regressions.
- A seeded differential harness compares the original and port on **80,000
  bounded Boolean-match cases across four seeds: 0 mismatches**.
- The 38-file snapshot is SHA-256 checked offline and can be fetched and
  byte-compared directly with the exact upstream commit.
- CI runs the complete proof and strict Clippy on Ubuntu and Windows with
  Rust 1.85.
- This crate uses `#![forbid(unsafe_code)]`; its ECMAScript regex dependency
  is built with `prohibit-unsafe`.

The port covers scanning and matching; literals, stars, qmarks and globstars;
braces and ranges; brackets and POSIX classes; dotfile rules; path negation;
Windows and POSIX separators; captures, lookarounds and backreferences; all
five extglob operators; Bash and minimatch compatibility matrices; callbacks
and ignore/format hooks; and malicious-pattern safeguards.

## Judge it in 60 seconds

| Claim | Reproducible evidence |
| --- | --- |
| The tests are really upstream's | `npm run verify:upstream` fetches commit `4f41a8e` and compares the complete 38-file test tree after LF normalization |
| The frozen behavior is preserved | `npm test` runs 1,977 unchanged upstream tests and 15 native regressions |
| The bounded Boolean matcher corpus agrees | `npm run fuzz:ci` reports 80,000 comparisons across four fixed seeds and 0 mismatches |
| The implementation is native and safe | Core scan/compile/match code is Rust; this crate forbids unsafe code and `regress` is built with `prohibit-unsafe` |
| It works on both path platforms | [CI](https://github.com/LubuSeb/picomatch-mortis/actions/workflows/ci.yml) runs every proof gate on Ubuntu and Windows |

## Reproduce it

Requirements: Rust 1.85 or newer, Node.js 22, and Git.

```sh
npm ci
npm run verify:upstream
npm test
npm run fuzz:ci
npm run demo
```

`npm test` verifies the frozen SHA-256 manifest, runs the native Rust suite,
builds the port, and executes all original JavaScript tests unchanged.
`verify:upstream` adds the network-backed provenance check. A compiled-once,
direct-native throughput smoke test is also available:

```sh
npm run bench
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

The repository also contains a CLI and a persistent line protocol used by the
proof harness:

```sh
cargo run -- is-match "src/**/*.rs" "src/glob.rs"
cargo run -- source "!(*.test).js"
cargo run -- scan "src/**/*.rs" --parts --tokens
```

## Architecture

```text
unchanged JS tests
        |
        v
thin JS API adapter -- callbacks, arrays, JS RegExp reconstruction
        |
        v
persistent native bridge -- serialization only, no glob logic
        |
        v
safe Rust scanner + glob compiler
        |
        +--> public ECMAScript-compatible source
        |
        `--> slash-safe native execution source --> regress engine
```

The two generated regex forms solve a subtle compatibility problem. Picomatch
exposes JavaScript regex text whose negated classes can admit `/`, while glob
matching must still enforce path-segment boundaries. Keeping a public form and
a private execution form preserves both observable API output and correct
native behavior.

The JavaScript adapter contains no built-in matching engine. It keeps one Rust
process alive and uses a synchronous worker/`SharedArrayBuffer` bridge because
Picomatch's public API is synchronous. JavaScript-only values such as
callbacks (including `expandRange` transformation), arrays, and `RegExp`
reconstruction are necessarily orchestrated there; scanner, compiler, and
Boolean matcher behavior otherwise run in Rust.

## Hard parts

1. **Extglob semantics.** Nested negation, adjacent extglobs, slash-containing
   bodies, Bash backtracking cases, and suffix-sensitive negative lookaheads
   required structural compilation rather than string replacement.
2. **ReDoS safeguards.** Risky repeated alternatives are literalized, safe
   star-only languages are reduced to character repetitions, and bounded
   nested recursion preserves upstream's `maxExtglobRecursion` behavior.
3. **Cross-platform escapes.** Windows normalization and Picomatch's unusual
   collapse of very long even backslash runs required explicit behavior and
   multi-OS CI.
4. **API-visible regexes.** `parse`/`makeRe` source parity and native matching
   sometimes need different internal expressions, so neither substitutes for
   the other.

## Provenance and scope

- Kickoff: 2026-07-31 18:00 UTC
- Upstream commit: [`4f41a8e`](https://github.com/micromatch/picomatch/tree/4f41a8edade7a5ab19832f7b40ecce46b288767f)
- Upstream implementation: 2,444 JavaScript source lines
- First port commit: [`eb06bd8`](https://github.com/LubuSeb/picomatch-mortis/commit/eb06bd8), created after kickoff
- Incremental history: scanner, core matcher, multi-OS proof, hardening, then
  complete extglob and recursion parity

| Surface | Status |
| --- | --- |
| Core Boolean `picomatch()` / `isMatch` behavior | Frozen-suite parity plus an 80,000-case, four-seed bounded differential corpus |
| `scan`, `parse`, `makeRe`, options and callbacks used upstream | Covered by unchanged upstream suites |
| Rust library and CLI | Implemented and tested; not yet published as a stable package |
| JS-only callback execution and array orchestration | Intentionally remains in the thin adapter |
| Raw user regex with ambiguous nested repetition | The bridge has a bounded failure-and-teardown path; no formal worst-case guarantee is claimed |
| Differential surface | Boolean results and error classes only; `scan`/`parse`/`makeRe` and callback objects rely on the frozen suites |
| Entire input space | Not formally proven; the claim is limited to the evidence above |

The local direct-native benchmark reports hundreds of thousands of
matches/second for four precompiled representative patterns on the development
machine, depending on system load. It is a smoke measurement, not a
cross-language victory claim.

See [WRITEUP.md](WRITEUP.md) for the publishable porting narrative,
[DECISIONS.md](DECISIONS.md) for the engineering decision log, and
[DEMO_SCRIPT.md](DEMO_SCRIPT.md) for the 2--3 minute judge demo runbook.
