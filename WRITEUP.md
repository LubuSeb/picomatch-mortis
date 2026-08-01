# I ported 1,977 observable promises: Picomatch Mortis

*Port Mortem 2026, Track F -- JavaScript to Rust*

## The bet

Picomatch looks small: roughly 2,444 lines of dependency-free JavaScript. Its
behavioral surface is not. It is a glob engine used throughout the JavaScript
toolchain, and its contract spans POSIX and Windows paths, Bash and minimatch
semantics, JavaScript regular-expression behavior, captures, callbacks, and
hostile inputs.

That made it a useful Port Mortem target. A shallow port can match `*.js`; a
faithful one must explain why nested extglobs, globstars at segment boundaries,
legacy `/i` case folding, and 65,500 backslashes behave exactly as they do.

## Proof first, claims second

The repository turns each compatibility claim into a reproducible gate:

| Evidence | Final result |
| --- | --- |
| Frozen upstream provenance | 38 files pinned to Picomatch commit `4f41a8e`, with local hashes plus a fetch-and-byte-compare check against that commit |
| Unchanged upstream suite | 1,977 tests pass |
| Native regressions | 28 Rust tests pass |
| Differential matching | 100,000 generated comparisons across five fixed 20,000-case seeds, plus 535 directed executions (107 per seed), with zero mismatches |
| Legacy case folding | All 1,169 nonidentity BMP mappings checked through 2,392 ordered equivalences and 4,784 literal/class mapping checks on pinned Node 24.18.0 |

The test manifest is verified before the suite runs. A second provenance command
fetches the exact upstream commit, verifies that the manifest names the complete
test tree, and byte-compares every LF-normalized file. This is stronger than a
repository-owned checksum: it proves both that the fixtures did not drift and
where they came from.

The deterministic differential harness then runs the pinned JavaScript release
and the Rust-backed port on the same bounded grammar. It combines paths,
extglobs, braces, classes, negation, Unicode, platform modes, and option sets.
Five independent fixed seeds make the run reproducible without letting one
friendly seed become the whole argument. The 107 directed cases are replayed
under every seed so each discovered edge case remains a permanent regression.

Legacy non-Unicode case folding gets a separate exhaustive check because random
text is poor at finding its quirks. The vendored engine contains the exact
ECMAScript Canonicalize table for all BMP code units. The proof derives every
equivalence class, checks both literal and character-class paths against Node
and pinned Picomatch, and enables `capture` to bypass the exact-input shortcut.
The runtime is pinned because JavaScript's Unicode data can change even within
a Node major release. That forces both regex engines to execute all 4,784
checks against one declared reference.

## Where the implementation lives

The scanner, glob compiler, regex execution, compiled-pattern searches,
captures, and compiled ignore searches run in Rust. The scanner records byte,
character, and slash positions in one forward pass, then converts the recorded
positions to JavaScript UTF-16 indexes in one additional linear pass. It no
longer rescans every prefix, so slash-dense and astral inputs remain O(n).

The unchanged tests still expect a synchronous Picomatch-shaped JavaScript API.
A thin adapter talks to one persistent Rust process. It validates JavaScript
`RegExp` flags and generated source, invokes JavaScript callbacks, and rebuilds
the API objects that callers expect. It also preserves upstream's empty-input,
exact-input, and invalid-source shortcuts, but contains no fallback glob
matcher. Ignore patterns are compiled eagerly at matcher construction, matching
upstream's error timing; non-short-circuited ignore searches execute natively.

The protocol separates options from data with an explicit `--payload` boundary,
then hex-frames fields whose contents may include tabs or record separators.
Inputs such as `--payload`, `--tokens`, U+001E, and U+001F therefore remain data
instead of changing the command. Sequence numbers prevent stale replies from
satisfying a later call. Requests and responses are capped at 4 MiB, and
startup, request, and shutdown waits are bounded.

## The hard parts

### Regex source is observable, but matching needs its own form

Picomatch exposes generated regexes through `parse` and `makeRe`. Some tests
inspect that source, while a native glob matcher also has to enforce rules that
are not obvious from the public text. A negated character class, for example,
must not silently cross a path separator.

The Rust compiler therefore emits a public source for the API and a private,
slash-safe execution source. The frozen source assertions pass, while native
matching can make path boundaries explicit. This is a compatibility mechanism,
not a claim that every possible public source string is byte-for-byte identical
to upstream; equivalent sources can differ structurally.

### Captures and flags cannot be bolted on afterward

With `capture: true`, compiler-generated groups change backreference numbering.
Wildcard, globstar, brace, bracket, and extglob captures therefore have to be
planned while Rust emits the regex, not recreated by a second JavaScript match.
Rust returns the full match and capture spans as UTF-16 offsets. The adapter uses
those native spans to construct real `RegExpExecArray`-compatible results,
including named groups and `d`-flag indices.

The same boundary preserves `g` and `y` `lastIndex` behavior: JavaScript owns
the stateful object, but each search is performed by Rust from the requested
UTF-16 position. Rust executes the semantic parts of `i`, `m`, `s`, `u`, and
`v`; JavaScript validates invalid or duplicate flag strings and shapes `d`,
`g`, and `y` API state. Windows separator normalization, doubled separators,
globstar boundaries, basename matching, and capture offsets are covered by the
same native path.

### Extglobs are a grammar, not five substitutions

The operators `?()`, `*()`, `+()`, `@()`, and `!()` interact with alternation,
suffixes, slashes, captures, and each other. One early version handled shallow
cases but stalled or under-matched nested negation. The correction was
structural: reduce pure nested-negation chains by parity, preserve suffix and
alternative context, and rewrite only repeated languages that can be proven
safe. Ambiguous repeated alternatives are literalized where upstream requires
it; star-only languages are reduced without dropping branches.

### Safety must fail explicitly

Pattern compilation has visible structural limits: 65,536 UTF-16 code units,
64 nested frames, 1,024 alternation branches per scope, 512 unmatched bracket
markers, and a proportional compile-work budget with a 4,096-step floor. These
limits prevent valid-looking syntax from turning into stack exhaustion or
exponential suffix compilation.

Regex execution uses a vendored, Rust-1.85-compatible `regress` 0.11.1. The
patch adds deterministic fuel to instruction dispatch, backtracking, optimized
scans, and backreference comparisons. Each match receives one million base
steps, 16 per input code unit, and 64 per compiled instruction, capped at 40
million. A completed result preserves matcher semantics. Exhaustion never
becomes a plausible-looking `false`; it returns an explicit, recoverable
`safe work limit` error.

That distinction matters in practice. A one-million-character linear `*`
match completes successfully. The hostile pattern `+(a*)b` against 30 `a`
characters followed by `y` reaches the safe-work error in milliseconds, and
the next `a` against `a` call succeeds through the same bridge.

## What differential testing found

The discrepancy ledger is part of the result, not an embarrassment hidden by
the final zero:

| Discrepancy cluster | Representative failure | Rust-side correction |
| --- | --- | --- |
| Nested negative extglobs | Deep `!(!(...))` chains and negation inside alternatives | Reduce pure chains by parity while retaining suffix, alternative, and capture context |
| Legacy case folding | `k` versus Kelvin sign under `i` compared with `iu` | Add the exact legacy BMP Canonicalize table; keep Unicode folding for `u`/`v` |
| Flag projection and state | `s` over newlines, `gu` over astral text, and `y` from a nonzero `lastIndex` | Project `i/m/s/u/v` into native flags and drive native searches from JS-managed `g/y` state; construct `d` indices from native spans |
| Capture numbering | Wildcard/extglob captures combined with nested and named groups | Emit capture-sensitive native regexes and return every UTF-16 group span, including unmatched groups |
| Typed payloads and token framing | Patterns named `--payload` or `--tokens`, and tokens containing U+001E/U+001F | Parse options before a payload marker and hex-frame request, response, and token fields |
| Windows separators and globstars | Backslashes, doubled separators, trailing slashes, and stars after extglobs | Normalize separators without shifting UTF-16 captures and preserve context-specific globstar boundaries |
| Execution fuel | `+(a*)b` on a near-miss caused explosive backtracking | Charge all major execution paths and return a typed, recoverable limit error instead of hanging or returning `false` |
| Eager ignore behavior | An invalid ignore pattern failed only on first use | Compile ignore patterns at matcher construction while keeping non-short-circuited ignore searches native |

The first deterministic run exposed dozens of real differences. Expanding the
grammar and asking independent reviewers for adversarial cases found more in
Unicode, flags, captures, framing, and Windows behavior. Every one above is now
a directed regression; the five-seed run reports zero mismatches.

## Result and honest boundary

The result is a safe Rust scanner, compiler, and matcher backed by 1,977
unchanged upstream tests, 28 native tests, direct fixture provenance, exhaustive
legacy case-fold evidence, and 100,535 deterministic differential executions.
Unsafe code is forbidden in this crate. The vendored engine runs with its
`prohibit-unsafe` feature, including a local correction to the feature's
Unicode-fold lookup branch.

This is strong reproducible evidence, not a formal proof over every string.
Completed native matches preserve the implemented semantics; a match that
exhausts deterministic fuel returns an error. `makeRe` must return a JavaScript
`RegExp` for API compatibility, so a caller who executes that returned object
directly is running Node's engine and does not inherit native fuel limits.
Likewise, public regex source may be structurally different even where it is
behaviorally equivalent. Callback invocation, JS object reconstruction, and
JS validation of flags and source necessarily remain in the proof adapter.

Those boundaries are explicit because the claim is precise: all 1,977 frozen
upstream tests pass with Rust owning scan and compile plus every
non-short-circuited match, capture, and ignore search, and the reproducible
differential corpus currently finds no mismatch.
