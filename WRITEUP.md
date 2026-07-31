# I ported 1,977 observable promises: Picomatch Mortis

*Port Mortem 2026, Track F -- JavaScript to Rust*

## The bet

Picomatch looks small: roughly 2,444 lines of dependency-free JavaScript. Its
behavioral surface is not small. It is the glob engine beneath a broad part of
the JavaScript toolchain, and its tests combine POSIX paths, Windows paths,
Bash semantics, minimatch compatibility, regex features, malicious inputs,
and nested extglobs.

That made it a useful Port Mortem target. A shallow port can match `*.js`; a
faithful one must explain why `!(*.d).{ts,tsx}`, `*(*(of*(o)x)o)`, and 65,500
backslashes behave the way they do.

## Building the proof before claiming the port

I pinned upstream commit `4f41a8e`, copied its test tree without edits, and
recorded canonical LF-normalized SHA-256 hashes for all 38 files. `npm test`
verifies those hashes before running anything else. A second command fetches
that exact commit, verifies that the manifest names its complete test tree,
then byte-compares every normalized file. This makes an accidental fixture
edit fail loudly and makes the parity number independently auditable instead
of asking a judge to trust a repository-owned checksum.

The first checkpoint implemented only the scanner and passed its original
suite. The next implemented a narrow compiler and 251 tests. Each later commit
expanded behavior and proof together until every executable upstream suite
was included: 1,977 tests across 36 files.

After that suite was green, I added a deterministic differential harness
rather than treating the frozen corpus as the end of the proof. It generates
bounded combinations of paths, extglobs, braces, classes, negation, platform
modes, and option sets, then executes the exact upstream commit and this port
on the same input. The first 50,000-case run exposed 87 discrepancies. They
clustered around Bash-mode negative extglobs, POSIX classes with
`literalBrackets`, optional slashes, and `contains` with terminal globstars.
I fixed those semantics, then used an adversarial review to broaden the grammar
with Unicode literals and composed glob forms and to add three alternate
seeds. That second pass exposed more real gaps rather than letting a chosen
seed become a comfort blanket. The current CI result is 80,000 comparisons
across four fixed seeds with zero mismatches.

| Discrepancy cluster | Representative failing case | Rust-side correction |
| --- | --- | --- |
| Bash negative extglobs | `!(b\|bar)` against a multi-segment path | Bash-aware consumption, dot guards, and end boundary |
| POSIX classes plus `literalBrackets` | `[[:digit:]]` in `contains` mode | Preserve upstream's POSIX/range precedence |
| `noglobstar` optional prefix | `**/*.b` against `file.b/` | Model the leading `**/` optional segment rule |
| `contains` plus terminal globstar | `b/**` before a hidden segment | Require the segment guard before the terminal body |
| Empty suffix match in negative extglob | `!(b\|c)` against `b` | Require a non-empty candidate at the search position |
| JavaScript UTF-16 literals | Emoji inside braces and extglobs | Execute non-BMP literals as two non-`u` JavaScript code units |
| Composed extglob and globstar | `@(a\|b)**` across path segments | Preserve context-specific globstar and Bash boundaries |

The original tests still call a Picomatch-shaped JavaScript API. A thin adapter
serializes calls to one persistent Rust process. It reconstructs JavaScript
objects and invokes JavaScript callbacks, but contains no scanning or glob
logic. That division matters: test parity is evidence about the Rust port, not
about a second JavaScript implementation hidden in the harness.

## What resisted a direct translation

### Regex source is part of the API

Picomatch exposes generated regexes through `parse` and `makeRe`. Tests inspect
those strings, so using a different-but-equivalent Rust regex is not enough.
At the same time, some JavaScript-visible negated character classes admit `/`
in their text even though a glob matcher must not cross path boundaries.

The compiler therefore emits two forms: an upstream-compatible public source
and a private execution source with explicit slash exclusion. Both are made by
Rust. This preserved exact API output without weakening native matching.
For example, public character-class text may contain `[^a]` because callers
inspect it, while the private matcher uses the equivalent slash-safe `[^a/]`
so the same class cannot silently cross a path boundary.

### Extglobs are a grammar, not five substitutions

The five operators `?()`, `*()`, `+()`, `@()`, and `!()` interact with
alternation, suffixes, slashes, captures, and each other. Negative extglobs in
particular need context-sensitive lookaheads: a suffix may participate for a
file-extension exclusion but not for an adjacent negative extglob.

One early version passed small extglob cases and then stalled on eight nested
negations. The fix was structural: recognize pure nested negation and reduce
it by parity before generating a regex. That turned the stress case from a
timeout into a millisecond-scale match while retaining ordinary nested forms.

### Security behavior is observable behavior

The newest upstream suite specifies defenses for repeated extglobs. The Rust
compiler detects the dangerous cases rather than relying on an engine timeout:

- ambiguous repeated alternatives such as `+(a|aa)` become literals;
- safe star-only languages such as `*(*(f)*(o))` reduce to `[fo]*`;
- multiple branches and optional captures survive the rewrite;
- bounded recursion and the explicit opt-out match upstream options.

The malicious suite also covers configurable input limits, long escape runs,
imbalanced syntax, and prototype-property names masquerading as POSIX classes.

## A useful CI failure

The first multi-OS run found two issues that a Windows-only workstation hid.
The selected regex release required a newer compiler than the declared Rust
1.85 minimum, so I pinned the last compatible safe release. Later, a
Linux-only malicious test exposed Picomatch's collapse of a 65,500-character
even backslash run. I added a native regression and kept CI on both platforms.

This is why the final evidence is not just a local green line: the full suite,
hash verification, formatting, and strict Clippy run on Ubuntu and Windows.

## Using agents as critics, not evidence

Once the implementation was green, I asked independent agent reviewers to
score it as judges, audit the Rust boundary, verify test provenance, red-team
the claims, and critique the demo. Their consensus was uncomfortable but
useful: the frozen suite was strong, while a self-owned hash, an IPC-heavy
benchmark, and no differential proof left avoidable credibility gaps.

Those critiques changed the artifact. The provenance check now reaches the
upstream commit directly; the benchmark compiles patterns once and calls the
Rust library without bridge overhead; the differential harness became a CI
gate; startup and request waits gained explicit failure bounds; and the README
now maps each claim to a command. The agents proposed challenges, but the
repository's reproducible outputs remain the evidence.

## Result and honest boundary

The result is a safe Rust scanner and matcher with full frozen-suite parity,
zero mismatches in the four-seed 80,000-case differential run, an incremental
post-kickoff history, and reproducible proof commands. It is not a formal
equivalence proof for every possible string. JavaScript callback invocation
necessarily remains in the adapter, the crate uses a pinned ECMAScript regex
dependency, and neither the Rust API nor CLI has been published as a stable
package yet.

Those limits are visible by design. The claim is precise: every unchanged
upstream executable test passes through the Rust implementation on both major
path platforms, and the separate deterministic differential corpus currently
finds no behavioral discrepancy, with unsafe code forbidden in this crate.
