# Decision log

## D001 -- Choose the official-pool, proof-rich path

**Time:** 2026-07-31, after kickoff

**Decision:** Port `micromatch/picomatch` at commit
`4f41a8edade7a5ab19832f7b40ecce46b288767f` from JavaScript to Rust.

**Why:** It is explicitly eligible in the official pool, has a focused
2,444-line zero-dependency core, and backs that core with 38 test and fixture
files. Globstars, extglobs, braces, POSIX classes, path separators, malicious
inputs, and callback options make it a deep equivalence target rather than a
toy rewrite.

**Consequence:** Behavioral compatibility and unchanged-test proof take
priority over API polish.

## D002 -- Initially execute generated patterns with `regress` 0.10.4

**Time:** 2026-07-31, after the scanner checkpoint

**Decision:** Compile Picomatch syntax to ECMAScript-compatible regular
expressions in Rust and initially execute them with pinned `regress` 0.10.4 and
its `prohibit-unsafe` feature.

**Why:** Picomatch relies on lookarounds and backreferences that Rust's common
`regex` crate intentionally does not support. An ECMAScript engine preserves
those semantics while keeping execution native.

**Consequence:** This records the historical checkpoint, not the final engine.
The completed port vendors and patches `regress` 0.11.1; D009 supersedes the
version and adds exact legacy case folding plus deterministic execution fuel.

## D003 -- Separate public and execution regex forms

**Time:** 2026-07-31, during combined-suite integration

**Decision:** Compile an API-visible source and a private slash-safe execution
source from the same Rust compiler.

**Why:** Upstream exposes regex text, but that public representation is not
always sufficient to enforce glob path boundaries in a different engine. A
single form made either frozen source assertions or native Bash behavior fail.

**Consequence:** Public compatibility cannot silently weaken native path
behavior. Frozen source assertions pass, but equivalent public source outside
those assertions may differ structurally from upstream.

## D004 -- Keep matching semantics native; keep JavaScript at the API boundary

**Time:** 2026-07-31, before importing additional suites

**Decision:** Use a persistent Rust child and a synchronous worker bridge.
JavaScript may validate JavaScript `RegExp` flags and source, reconstruct API
objects, invoke callbacks, apply callback-provided transformations, and
preserve upstream's empty-input, exact-input, and invalid-source shortcuts. It
may not implement a fallback glob matcher or evaluate a compiled glob.

**Why:** Unchanged synchronous JavaScript tests are the clearest parity proof,
but spawning a process per assertion is noisy and slow. Some compatibility
objects and callbacks can only exist in JavaScript.

**Consequence:** Thousands of assertions reuse one native process. Main and
ignore patterns compile eagerly through Rust; every non-short-circuited Boolean
and capture search is native, and the remaining JavaScript responsibilities
are explicit and auditable.

## D005 -- Simplify risky extglobs structurally

**Time:** 2026-07-31, during the final upstream suite

**Decision:** Analyze ambiguous repeated alternatives and nested repetition in
the Rust compiler. Literalize constructs when upstream's safety rule requires
it, reduce provably safe star-only languages without losing branches, and
reduce pure nested-negation chains by parity while preserving surrounding
suffix and alternative context.

**Why:** Naive nested lookarounds can cause pathological compilation or
matching, and early nested-negation handling could stall or under-match.
Picomatch's malicious and extglob suites specify observable rewrites and
capture behavior.

**Consequence:** The nested-negation discrepancy became a fixed regression,
not a residual limitation. Ordinary Bash/minimatch extglobs remain intact, and
structural rewrites reduce risk before the execution-fuel backstop in D009.

## D006 -- Add direct provenance and deterministic differential proof

**Time:** 2026-07-31, after full frozen-suite parity; expanded 2026-08-01

**Decision:** Fetch and byte-compare the exact upstream test tree in CI, then
run a bounded differential harness against that pinned commit with five fixed
20,000-case seeds plus 107 directed cases per seed.

**Why:** A repository-owned checksum detects accidental edits but cannot prove
where the files came from. A frozen suite can also miss valid combinations even
when every included assertion passes.

**Consequence:** The final evidence is 38 provenance-checked files, 1,977
unchanged upstream tests, 28 Rust tests, 100,000 generated comparisons, and 535
directed executions with zero mismatches. Earlier discrepancies in Unicode,
case folding, nested negation, flags, captures, framing, separators, and
globstars remain as directed regressions.

## D007 -- Treat independent agents as adversarial reviewers

**Time:** 2026-07-31 through 2026-08-01, evidence passes

**Decision:** Use separate judge, Rust, provenance, fuzzing, capture, flags,
security, and presentation reviews, then verify every concern with repository
commands before changing implementation or claims.

**Why:** A single builder is poorly positioned to notice its own credibility
gaps. Independent critiques identified the self-trusted hash,
bridge-dominated benchmark, narrow differential grammar, capture fidelity,
Unicode flags, transport ambiguity, and regex denial-of-service risk.

**Consequence:** The artifact gained direct upstream verification, native
compiled-once benchmarks, five fuzz seeds, exhaustive case-fold proof, native
match spans, typed framing, deterministic fuel, and explicit limitations.
Agent opinion is process; reproducible output is evidence.

## D008 -- Bound every native compilation dimension

**Time:** 2026-07-31, expanded during 2026-08-01 adversarial hardening

**Decision:** Enforce a 65,536-UTF-16-code-unit pattern ceiling, 64 structural
nesting frames, at most 1,024 alternation branches per parenthesis or brace
scope, and at most 512 unmatched bracket markers. Give recursive compilation
a work budget of 64 times pattern length with a 4,096-step floor.

**Why:** Input length alone did not prevent stack exhaustion, quadratic class
searches, branch explosions, or exponential suffix recompilation. Each
structural dimension needs a deterministic bound.

**Consequence:** Oversized or over-complex inputs return explicit native errors
instead of aborting or growing without bound. Extreme syntactically valid
inputs may therefore hit a disclosed safety boundary.

## D009 -- Vendor `regress` 0.11.1 for exact case folding and execution fuel

**Time:** 2026-08-01, final semantic and security hardening

**Decision:** Vendor `regress` 0.11.1, retain `prohibit-unsafe`, patch it for
Rust 1.85, add the exact legacy ECMAScript Canonicalize table, and meter regex
execution deterministically. The budget is 1,000,000 base steps plus 16 per
input code unit and 64 per compiled instruction, capped at 40,000,000; dispatch,
backtracking, optimized scans, and backreference comparisons all consume fuel.

**Why:** Unicode folding is not a substitute for legacy non-`u` JavaScript `/i`
semantics, and a compiler-only rewrite cannot bound every backtracking regex.
Depending on wall-clock cancellation would make results machine-dependent.

**Consequence:** A separate proof checks all 1,166 nonidentity BMP mappings,
2,386 ordered equivalences, and 4,772 literal/class mapping paths. A
one-million-character linear star match succeeds, while `+(a*)b` on a short
near-miss returns an explicit safe-work-limit error in milliseconds. Exhaustion
is never reported as `false`, and the next bridge call succeeds.

## D010 -- Make the proof transport typed, bounded, and recoverable

**Time:** 2026-08-01, bridge red-team pass

**Decision:** Parse command options before an explicit `--payload` marker,
hex-frame payload, response, and token fields, and tag shared-buffer exchanges
with monotonically increasing request sequences. Cap each request or response
at 4 MiB, bound startup/request/shutdown waits, reject lone UTF-16 surrogates
explicitly, and tear down a bridge that actually times out.

**Why:** Option-shaped patterns, tabs, record separators, stale responses,
oversized messages, and half-dead workers otherwise create false matches,
desynchronization, or indefinite waits.

**Consequence:** Payloads remain data regardless of spelling. Native compile or
safe-work-limit errors are recoverable and do not poison the next request;
transport timeouts fail closed and replace uncertainty with a visible error.

## D011 -- Keep scanning O(n), including UTF-16 indexes

**Time:** 2026-08-01, performance review

**Decision:** Track byte, character, and slash positions during one forward
scan, then convert all recorded positions to UTF-16 in one subsequent linear
pass instead of rescanning each position's prefix.

**Why:** The earlier conversion was correct but quadratic for slash-dense input,
especially when astral characters changed JavaScript offsets.

**Consequence:** ASCII and astral scanner regressions retain Picomatch's public
indexes while total scan work remains linear in input length.

## D012 -- Return native match spans and model `RegExp` state at the boundary

**Time:** 2026-08-01, capture and flags audit

**Decision:** Have Rust return the full match plus every capture as JavaScript
UTF-16 ranges. Reconstruct `RegExpExecArray` objects, named groups, and `d`
indices from those spans. Let JavaScript own `g`/`y` `lastIndex`, but perform
each search natively from that requested offset. Project `i`, `m`, `s`, `u`,
and `v` semantics into the native engine and let JavaScript reject illegal or
duplicate flag strings.

**Why:** Re-running the public regex in JavaScript would move matching semantics
back into the proof adapter and could hide capture-numbering, Unicode, or
Windows-normalization bugs.

**Consequence:** Wildcard, globstar, brace, bracket, extglob, nested, named, and
unmatched captures all come from native execution. Stateful flags remain API
compatible without creating a second matcher. A caller who executes the
`RegExp` returned by `makeRe` directly is outside this path and does not inherit
the native engine's fuel limit.
