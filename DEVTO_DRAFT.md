---
title: I ported Picomatch to Rust. It passed 1,977 tests and lost the benchmark by 18x
published: true
description: How one Unicode mismatch, 100,535 differential checks, and an honest benchmark changed the way I test AI-assisted ports.
tags: rust, javascript, testing, hackathon
---

*A Port Mortem 2026 write-up about a JavaScript to Rust port, one strange Unicode character, and what green tests do not tell you.*

The first benchmark result was not flattering.

Picomatch, running under Node, handled about 7.18 million matches per second in my test. The direct Rust implementation managed about 395,000. That made the Rust version roughly 18.2 times slower.

I kept the result because it makes the point of the project clearer. The hard part was verification: how much evidence would it take to trust an AI-assisted rewrite beyond the obvious cases?

## The small library that was not a small target

Picomatch is a dependency-free glob matcher used across the JavaScript ecosystem. Its committed runtime is only 2,444 lines of JavaScript, which made it look like a sensible target for a 72-hour port.

The scope was reasonable for 72 hours, but I underestimated the compatibility surface.

Matching `*.js` is easy. Matching Picomatch means dealing with globstars at path boundaries, nested extglobs, braces, negation, captures, callbacks, Bash and minimatch options, Windows separators, JavaScript regex flags, UTF-16 indexes, and inputs designed to make a parser or regex engine suffer.

I chose the official Picomatch snapshot at commit `4f41a8e` and wrote a standalone Rust scanner, compiler, and matcher. The command-line program runs without Node. A small JavaScript adapter exists for the unchanged upstream test suite because those tests expect JavaScript callbacks, `RegExp` objects, and synchronous API behavior. It does not contain a fallback glob matcher. Every search that is not an API shortcut runs in Rust.

Getting that boundary right mattered. Otherwise I could have produced a convincing test report while quietly letting JavaScript do the difficult work.

## All 1,977 tests were green. The port was still wrong

The inherited Picomatch suite eventually passed in full. All 1,977 tests were green, and the repository also had 28 native Rust tests.

Then differential testing found more problems.

One of the clearest mismatches involved `ſ`, the Latin long s. It looks a little like an `f` without the full crossbar. JavaScript treats it differently depending on whether a regular expression uses legacy case-insensitive matching or Unicode-aware case-insensitive matching:

```js
/s/i.test('ſ')   // false
/s/iu.test('ſ')  // true

/k/i.test('K')   // false
/k/iu.test('K')  // true
```

My early implementation treated case-insensitive matching as one Unicode problem. JavaScript actually has two relevant behaviors here. Legacy `/i` uses its own Canonicalize rules. Adding `u` changes the result for characters such as long s and the Kelvin sign.

An engine can call itself ECMAScript-compatible and still miss exactly this sort of historical corner.

Instead of adding only those two characters as regressions, I pinned Node 24.18.0, derived the complete legacy Canonicalize table for the Basic Multilingual Plane, and added it to the native engine. The proof walks all 65,536 BMP code units, finds every nonidentity mapping, and checks both literal and character-class matching against Node and the pinned Picomatch version.

That produced 1,169 nonidentity mappings, 2,392 ordered equivalences, and 4,784 literal and class checks. The odd character that exposed the bug became a test for the whole defined scope.

The upstream suite was useful, but I did not want the final claim to depend on one set of examples or on hashes created inside my own repository.

The proof grew in layers:

| Evidence | Result |
| --- | --- |
| Upstream provenance | 38 test and fixture files pinned to Picomatch commit `4f41a8e`, fetched and byte-compared |
| Original suite | 1,977 unchanged tests passed |
| Native regressions | 28 Rust tests passed |
| Generated differential cases | 100,000 comparisons across five fixed seeds |
| Directed differential cases | 535 executions, replaying 107 known edge cases under every seed |
| Legacy case folding | 4,784 literal and class checks over the derived BMP mappings |

The provenance check deserves a little explanation. A local checksum only proves that files did not change after I hashed them. It does not prove that I started with the upstream files. The stronger check fetches the exact upstream commit, confirms that the manifest covers the complete test tree, normalizes line endings, and byte-compares every file.

The differential harness runs the pinned JavaScript implementation and the Rust port on the same generated patterns, paths, platform modes, and options. It includes extglobs, braces, classes, negation, Unicode, separators, and captures. Five fixed seeds keep it reproducible. Known failures are kept as directed cases instead of being left to chance.

The final run reported zero mismatches across 100,535 differential executions.

That is not a formal proof over every possible string. It is bounded, reproducible evidence. I think saying where the evidence stops is part of making it useful.

## Extglobs needed a compiler, not substitutions

Another early mistake was thinking too locally about extglobs. Operators such as `?()`, `*()`, `+()`, `@()`, and `!()` look like five pieces of syntax that can be translated one at a time.

They cannot. Nesting changes the meaning. So do suffixes, alternation, path separators, captures, and repetition. A shallow implementation handled ordinary examples, then stalled or under-matched cases with nested negative extglobs.

The correction was structural. The compiler now reduces pure chains of nested negation by parity while preserving the surrounding suffix and alternative context. It rewrites only repeated languages that it can show are safe, and it keeps the ambiguous cases literal when that is what Picomatch requires.

Earlier adversarial and differential runs were more useful than the final zero-mismatch total because they supplied the cases that became permanent regressions. Those cases include nested negation, legacy case folding, capture numbering, regex flags, typed transport fields, Windows separators, globstar boundaries, and eager ignore behavior.

A second compiler problem came from Picomatch's public regex source. APIs such as `parse` and `makeRe` expose the generated regular expression, and some upstream tests inspect it. At the same time, the Rust matcher needs an execution form that makes path-separator rules explicit.

I initially tried to make one generated regex serve both purposes. That forced a bad choice. I could satisfy observable source assertions, or I could enforce native path behavior, but not always both with the same representation.

The final compiler emits two related forms. One is the public source used by the compatibility API. The other is a private, slash-safe execution source used by Rust. Both come from the same compiler, but they have different jobs.

Captures made the same lesson harder to ignore. With `capture: true`, compiler-generated groups affect backreference numbering. Wildcards, braces, brackets, extglobs, named groups, and unmatched groups all need to be planned while the regex is emitted. Rust now returns the full match and every capture as UTF-16 spans. The adapter uses those spans to reconstruct the JavaScript result, including named groups and `d`-flag indices. JavaScript owns state such as `g` and `y` `lastIndex`, but Rust performs each search from the requested position.

## A safety limit must not look like a non-match

Glob patterns eventually become regular expressions, and regular expressions can backtrack badly. Limiting the parser was not enough. A valid-looking pattern could still consume unreasonable execution work.

The compiler now bounds pattern length, nesting depth, branch count, unmatched bracket markers, and total compile work. The vendored regex engine also has deterministic execution fuel. Dispatch, scans, backtracking, and backreference comparisons all spend from the same budget.

When that budget runs out, the matcher returns a recoverable `safe work limit` error. It does not return `false`.

That distinction matters. A non-match says the engine finished and found no match. Exhaustion says the engine could not safely finish. Treating those as the same result could select or skip the wrong files in a build pipeline.

The safety tests include both sides. A linear match against a one-million-character input succeeds. The hostile pattern `+(a*)b` against a short near-miss reaches the work limit and returns an explicit error. The next ordinary request still succeeds through the same persistent process.

## What the benchmark measured

I benchmarked three paths using the same four precompiled pattern and input pairs. Each path returned the same 75 percent match rate. The figures below are medians from three timed runs after warm-up.

| Path | Median throughput |
| --- | ---: |
| Picomatch on Node | 7,184,496 ops/s |
| Direct native Rust API | 395,048 ops/s |
| Rust through the synchronous proof adapter | 11,602 ops/s |

V8's optimized regular-expression engine is extremely fast. The Rust implementation uses a safe ECMAScript interpreter with explicit fuel accounting. The proof adapter adds worker wakeups, framing, IPC, cache lookup, and JavaScript object reconstruction on top of that.

If throughput were the only goal, this port would lose. The direct native matcher is still practical at roughly 0.40 million matches per second for this workload, but I am not presenting it as a speed improvement.

So the port's value is not higher throughput. The Rust API and CLI run independently of Node, expensive work fails explicitly, and the repository provides a reproducible compatibility check. Publishing only the flattering measurements would weaken that claim.

## What I would do differently

If I started again, I would build a small JavaScript regex conformance probe before committing to the first execution-engine design.

It would cover legacy `/i` versus `/iu`, UTF-16 offsets, named and unmatched captures, `d` indices, `g` and `y` state, lookarounds, and backreferences. I would also separate the public regex source from the private execution form on day one.

Instead, those facts arrived as failures during integration. The failures led to a better design, but they cost time late in the build when every change touched the compiler, matcher, adapter, and proof harness.

I would also add direct upstream byte comparison earlier. Repository-owned hashes felt reassuring until an independent review pointed out that the repository was effectively certifying itself.

That review process was deliberate. I used separate coding agents to challenge the Rust design, provenance, fuzz grammar, captures, flags, safety, benchmarks, and presentation. I only accepted a concern when I could reproduce it as a failing case and run the fix through the full suite.

## What I would reuse on the next port

The process that held up was fairly simple:

1. Pin the exact source revision and fetch it during verification.
2. Keep the inherited tests unchanged.
3. Run the source and target implementations on the same generated inputs.
4. Turn every mismatch into a permanent directed regression.
5. Exhaustively test small domains that are defined by a specification.
6. Benchmark the unflattering paths too.
7. State the implementation boundary and the limits of the proof.

Picomatch Mortis now passes all 1,977 frozen upstream tests and 28 native tests. Its current bounded differential corpus reports zero mismatches across 100,535 executions. Rust owns scanning, compilation, and every non-short-circuited match, capture, and ignore search. JavaScript remains at the API boundary where JavaScript objects and callbacks require it.

## Receipts

- [Source and reproduction instructions](https://github.com/LubuSeb/picomatch-mortis)
- [Decision log](https://github.com/LubuSeb/picomatch-mortis/blob/main/DECISIONS.md)
- [Benchmark report](https://github.com/LubuSeb/picomatch-mortis/blob/main/BENCHMARK.md)
- [Judge demo](https://github.com/LubuSeb/picomatch-mortis/releases/tag/port-mortem-demo-v1)

Built for [Hackathon Raptors](https://www.raptors.dev/), [@raptors_hack](https://x.com/raptors_hack), and the Port Mortem 2026 JavaScript to Rust track.

*Disclosure: I used AI coding agents during the port and AI assistance to structure, edit, and fact-check this article. Every technical claim was checked against the repository's tests, scripts, decision log, and benchmark artifacts.*
