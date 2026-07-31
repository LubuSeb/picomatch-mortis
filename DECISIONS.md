# Decision log

## D001 — Choose the official-pool, proof-rich path

**Time:** 2026-07-31, after kickoff

**Decision:** Port `micromatch/picomatch` at commit
`4f41a8edade7a5ab19832f7b40ecce46b288767f` from JavaScript to Rust.

**Why:** It is explicitly eligible in the official pool, has a focused
2,444-line zero-dependency core, and backs that core with 37 files / roughly
17,000 lines of tests. Globstars, extglobs, braces, POSIX classes, path
separators, malicious inputs, and callback options create a deep equivalence
target rather than a toy rewrite.

**Consequence:** Behavior and unchanged-test proof take priority over API
polish. Generated-regex compatibility and native matching will be kept
separate so neither can silently substitute for the other.

## D002 — Execute generated patterns in a safe ECMAScript engine

**Time:** 2026-07-31, after the scanner checkpoint

**Decision:** Compile Picomatch syntax to ECMAScript-compatible regular
expressions in Rust and execute them with `regress` 0.11.1 using its
`prohibit-unsafe` feature.

**Why:** Picomatch relies on lookarounds that Rust's standard `regex` syntax
does not support. An ECMAScript engine preserves those semantics and lets the
same Rust compiler output drive both native matching and `makeRe` proof.

**Consequence:** The port has a small dependency tree rather than being
dependency-free. Unsafe code remains forbidden in this crate, and the regex
dependency is compiled in its explicit no-unsafe mode.
