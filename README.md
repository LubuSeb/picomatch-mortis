# Picomatch Mortis

[![proof](https://github.com/LubuSeb/picomatch-mortis/actions/workflows/ci.yml/badge.svg)](https://github.com/LubuSeb/picomatch-mortis/actions/workflows/ci.yml)

A from-scratch JavaScript-to-Rust port of
[`micromatch/picomatch`](https://github.com/micromatch/picomatch), selected from
the official Port Mortem 2026 pool.

## Result

- **1,977/1,977 unchanged upstream tests pass** across all 36 executable
  suites.
- The 38-file upstream snapshot is frozen and SHA-256 checked before every
  proof run.
- Eight native Rust regressions cover the scanner, core matcher, dotfiles,
  globstars, extglobs, and the 65,500-backslash stress case.
- CI runs the full proof and strict Clippy on both Ubuntu and Windows with
  Rust 1.85.
- The crate uses `#![forbid(unsafe_code)]`; its ECMAScript regex dependency is
  built with `prohibit-unsafe`.

The port covers scanning and matching; literals, stars, qmarks and globstars;
braces and ranges; brackets and POSIX classes; dotfile rules; path negation;
Windows and POSIX separators; captures, lookarounds and backreferences; all
five extglob operators; Bash and minimatch compatibility matrices; callbacks
and ignore/format hooks; and malicious-pattern safeguards.

## Reproduce it

Requirements: Rust 1.85 or newer and Node.js 22.

```sh
npm ci
npm test
npm run demo
```

The test command first verifies every frozen upstream file, runs the native
Rust suite, builds the port, and finally executes all original JavaScript
tests unchanged. A non-comparative local throughput check is also available:

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

The repository also contains a small CLI and a persistent line protocol used
by the proof harness:

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
matching must still enforce path-segment boundaries. Keeping the public form
and the private execution form separate preserves both observable API output
and correct native behavior.

The JavaScript adapter contains no scanning or glob-compilation logic. It
keeps one Rust process alive and uses a synchronous worker/`SharedArrayBuffer`
bridge because Picomatch's public API is synchronous. JavaScript-only values
such as callback functions are orchestrated in that adapter; every pattern
decision is made by Rust.

## Hard parts

1. **Extglob semantics.** Nested negation, adjacent extglobs, slash-containing
   bodies, Bash backtracking cases, and suffix-sensitive negative lookaheads
   required structural compilation rather than string replacement.
2. **ReDoS safeguards.** Risky repeated alternations are literalized, safe
   star-only languages are reduced to character repetitions, and bounded
   nested recursion matches upstream's `maxExtglobRecursion` behavior without
   dropping branches or captures.
3. **Cross-platform escapes.** Windows normalization and Picomatch's unusual
   collapse of very long even backslash runs needed explicit behavior, caught
   by the Linux half of CI.
4. **API-visible regexes.** `parse`/`makeRe` source parity and native matching
   sometimes need different internal expressions, so neither is used as a
   shortcut for the other.

## Provenance and scope

- Kickoff: 2026-07-31 18:00 UTC
- Upstream commit: `4f41a8edade7a5ab19832f7b40ecce46b288767f`
- Upstream implementation: 2,444 JavaScript source lines
- First port commit: `eb06bd8`, created after kickoff
- Incremental history: scanner, core matcher, multi-OS proof, hardening, then
  complete extglob and recursion parity

Passing the complete frozen suite is strong observable evidence, not a formal
proof for every possible string. The Rust API and CLI are not yet published as
a stable crate or npm package. The proof adapter necessarily owns JS-only
callback invocation and array orchestration, and the Rust implementation has
a small pinned regex dependency rather than being dependency-free.

See [WRITEUP.md](WRITEUP.md) for the porting narrative and [DECISIONS.md](DECISIONS.md)
for the engineering decision log.
