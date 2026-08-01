# Picomatch Mortis: 2--3 minute judge demo

Record one continuous terminal take with a large font. Build once before
recording so the live commands stay fast. Target length: about 2:30.

## 0:00--0:25 -- Start with the standalone Rust program

Run these first, before showing any JavaScript:

```sh
cargo run --release --quiet -- is-match --payload "src/**/*.rs" "src/parser/glob.rs"
cargo run --release --quiet -- source --payload "!(*.test).js"
```

> The first result is `true`; the second is the regex source produced by the
> Rust compiler. This is a standalone native package, not a JavaScript rewrite.
> The typed `--payload` boundary also prevents option-looking patterns and
> inputs from being interpreted as CLI flags.

Optionally make that last point visible with one short extra command:

```sh
cargo run --release --quiet -- is-match --payload "--payload/*" "--payload/value"
```

## 0:25--0:55 -- Run the frozen proof

Run:

```sh
npm test
```

> This verifies the 38 frozen fixture files, runs 28 Rust tests, then runs all
> 1,977 executable tests from the exact kickoff commit unchanged. The final
> adapter checks exercise captures, flags, typed payloads, limits, recovery,
> and teardown. The same workflow runs on Ubuntu and Windows.

Let the command finish live; it should be brief after the build is warm. Pause
on the four green stages rather than scrolling through individual test names.

## 0:55--1:15 -- Prove the hardest compatibility rule

Run:

```sh
npm run test:casefold
```

> JavaScript's legacy `/i` case folding is not Unicode `/iu` folding. This
> independent exhaustive check derives the legacy equivalence classes and
> compares literal and character-class behavior against Node.

## 1:15--1:40 -- Show evidence beyond the upstream suite

Show the already-completed output from `npm run fuzz:ci` and the command in
`package.json`.

> Five deterministic seeds generated 100,000 differential comparisons, plus
> 535 directed executions. Every case ran against both the pinned upstream
> package and this port, with zero mismatches. This is strong reproducible
> evidence, not a claim of formal equivalence.

Do not run the full fuzz command during the recording; the seed list and final
outputs make the result reproducible without spending the viewer's time.

## 1:40--2:15 -- Show behavior, captures, and safe recovery

Run:

```sh
npm run demo
```

> The proof adapter keeps Picomatch's synchronous API while a persistent native
> process performs scanning, compilation, and matching. Notice legacy `i`
> rejecting long-s while Unicode `iu` accepts it, the real capture array with
> UTF-16 indices, and an option-looking payload crossing the typed boundary.

Pause on the final safety section:

> A hostile repeated extglob does not hang, crash, or silently return `false`.
> It raises an explicit safe-work-limit error, and the very next ordinary match
> succeeds through the same process.

## 2:15--2:35 -- Honest close

> The core scanner, compiler, and matcher are native Rust; JavaScript is the
> proof adapter required to run the unchanged upstream API tests. If a caller
> takes `makeRe()` output and executes that JavaScript `RegExp` directly, it no
> longer has the native matcher's fuel limit. Public regex source can also
> differ structurally even where tested behavior and captures agree. The repo
> exposes the tests, seeds, bounds, and CI needed to audit every claim.

End on the repository page with both Ubuntu and Windows jobs green.
