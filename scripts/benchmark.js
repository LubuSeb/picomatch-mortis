'use strict'

const { performance } = require('node:perf_hooks')
const path = require('node:path')

process.env.PICOMATCH_MORTIS_BIN = path.join(
  __dirname,
  '..',
  'target',
  'release',
  process.platform === 'win32' ? 'picomatch-mortis.exe' : 'picomatch-mortis'
)
const pm = require('../tests')

const cases = [
  ['src/parser/glob.rs', 'src/**/*.rs'],
  ['packages/core/index.test.js', '**/!(*.test).js'],
  ['release-042.txt', 'release-{0..9}{0..9}{0..9}.txt'],
  ['foo/bar/baz.jsx', 'foo/bar/**/*.+(js|jsx)'],
]

for (let index = 0; index < 1_000; index++) {
  const [input, pattern] = cases[index % cases.length]
  pm.isMatch(input, pattern)
}

const iterations = 25_000
let matches = 0
const started = performance.now()
for (let index = 0; index < iterations; index++) {
  const [input, pattern] = cases[index % cases.length]
  if (pm.isMatch(input, pattern)) matches++
}
const elapsed = performance.now() - started

console.log(JSON.stringify({
  note: 'Persistent proof bridge included; non-comparative local measurement',
  iterations,
  matches,
  elapsedMs: Number(elapsed.toFixed(2)),
  operationsPerSecond: Math.round(iterations / (elapsed / 1_000)),
}, null, 2))
