'use strict'

const { performance } = require('node:perf_hooks')
const picomatch = require('picomatch-reference')

const cases = [
  ['src/parser/glob.rs', 'src/**/*.rs'],
  ['packages/core/index.test.js', '**/!(*.test).js'],
  ['release-042.txt', 'release-{0..9}{0..9}{0..9}.txt'],
  ['foo/bar/baz.jsx', 'foo/bar/**/*.+(js|jsx)'],
]
const matchers = cases.map(([, pattern]) => picomatch(pattern))

for (let index = 0; index < 10_000; index += 1) {
  matchers[index % matchers.length](cases[index % cases.length][0])
}

const iterations = 1_000_000
let matches = 0
const started = performance.now()
for (let index = 0; index < iterations; index += 1) {
  if (matchers[index % matchers.length](cases[index % cases.length][0])) matches += 1
}
const elapsed = performance.now() - started

console.log(JSON.stringify({
  referenceCommit: '4f41a8edade7a5ab19832f7b40ecce46b288767f',
  compiledPatterns: matchers.length,
  iterations,
  matches,
  elapsedMs: Number(elapsed.toFixed(2)),
  operationsPerSecond: Math.round(iterations / (elapsed / 1_000)),
}, null, 2))
