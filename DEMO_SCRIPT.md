# Picomatch Mortis: 2--3 minute demo

Record one continuous terminal take. Keep the repository and CI page visible
before starting; use a large font. Target length: 2:20.

## 0:00--0:20 -- The claim

> Picomatch Mortis is a from-scratch Rust port of Picomatch for Track F. The
> claim is deliberately narrow and reproducible: all 1,977 executable tests
> from the exact kickoff commit pass unchanged on Ubuntu and Windows, together
> with 16 native Rust regressions.

Show the CI badge and the "Judge it in 60 seconds" table in `README.md`.

## 0:20--0:45 -- Prove the fixtures

Run:

```sh
npm run verify:upstream
```

> This does not trust my own checksum. It fetches the pinned upstream commit,
> verifies the complete 38-file test tree, and byte-compares every normalized
> file with the frozen copy.

Then show the latest CI run with both operating-system jobs green. Do not wait
for the full test suite during the recording.

## 0:45--1:20 -- Show behavior

Run:

```sh
npm run demo
```

> One persistent bridge preserves Picomatch's synchronous JavaScript API, but
> scanning, compilation, and matching happen in Rust. Here are globstars,
> braces, negative extglobs, Windows paths, and JavaScript UTF-16 semantics.
> The scanner state and public regex source also come from Rust.

Pause briefly on the included and excluded negative-extglob rows, then on the
scanner tokens.

## 1:20--1:50 -- Show evidence beyond the suite

Show the completed output from:

```sh
npm run fuzz:ci
```

> This bounded harness runs identical generated inputs against upstream and
> the port. Early runs found edge cases; adversarial review then added Unicode
> and composed patterns plus three alternate seeds. It now reports 80,000
> comparisons and zero mismatches. This is not a formal proof, but it tests
> behavior beyond the frozen examples.

## 1:50--2:10 -- Engineering and safety

Show `src/lib.rs`, the `forbid(unsafe_code)` line, and the final two demo rows.

> Risky repeated extglobs are simplified structurally rather than handed to a
> regex engine unchecked. The crate forbids unsafe code, the regex dependency
> uses its safe implementation, and bridge requests have a tested bounded
> failure-and-teardown path instead of hanging forever.

## 2:10--2:25 -- Honest close

> The adapter still owns JavaScript-only callbacks, arrays, and RegExp object
> reconstruction; the Rust crate is not published yet; and this is behavioral
> evidence, not a theorem. The repository exposes every command and limitation
> needed to judge the claim. Thank you.

End on the README evidence table and repository URL.
