# Decision log

## D001 — Choose the official-pool, proof-rich path

**Time:** 2026-07-31, after kickoff

**Decision:** Port `micromatch/picomatch` at commit
`4f41a8edade7a5ab19832f7b40ecce46b288767f` from JavaScript to Rust.

**Why:** It is explicitly eligible in the official pool, has a focused
2,444-line zero-dependency core, and backs that core with 36 executable test
suites plus shared fixtures. Globstars, extglobs, braces, POSIX classes, path
separators, malicious inputs, and callback options create a deep equivalence
target rather than a toy rewrite.

**Consequence:** Behavior and unchanged-test proof take priority over API
polish.

## D002 — Execute generated patterns in a safe ECMAScript engine

**Time:** 2026-07-31, after the scanner checkpoint

**Decision:** Compile Picomatch syntax to ECMAScript-compatible regular
expressions in Rust and execute them with pinned `regress` 0.10.4 using its
`prohibit-unsafe` feature.

**Why:** Picomatch relies on lookarounds and backreferences that Rust's common
`regex` crate intentionally does not support. The ECMAScript engine preserves
those semantics while keeping execution native.

**Consequence:** The port has a small dependency tree rather than being
dependency-free. Unsafe code remains forbidden in this crate and prohibited
in the regex engine.

## D003 — Separate public and execution regex forms

**Time:** 2026-07-31, during combined-suite integration

**Decision:** Compile an API-visible source and a private slash-safe execution
source from the same Rust compiler.

**Why:** Upstream exposes JavaScript regex text that is not always sufficient
to enforce glob path boundaries when executed by a different engine. A single
form made either exact `makeRe` checks or native Bash behavior fail.

**Consequence:** Both forms are generated and tested; API compatibility cannot
silently weaken native path behavior.

## D004 — Keep the proof adapter logic-free

**Time:** 2026-07-31, before importing additional suites

**Decision:** Use a persistent Rust child and a synchronous worker bridge. The
adapter may reconstruct JS objects, invoke callbacks, and normalize JS-only
values, but it may not implement scanner or glob decisions.

**Why:** Unchanged synchronous JavaScript tests are the most legible parity
proof, but spawning a process per assertion is noisy and slow.

**Consequence:** Thousands of assertions exercise one native process while the
behavioral boundary stays auditable.

## D005 — Simplify risky extglobs structurally

**Time:** 2026-07-31, during the final upstream suite

**Decision:** Detect ambiguous repeated alternatives and nested repetition in
the Rust compiler. Literalize unsafe constructs, reduce safe star-only
languages without losing branches, and honor the upstream recursion option.

**Why:** Naively nested lookarounds can exhibit pathological compilation or
matching behavior. The upstream suite specifies exact safe rewrites and
capture behavior.

**Consequence:** The final malicious and recursion-hardening suites pass while
ordinary Bash/minimatch extglobs remain behaviorally intact.
