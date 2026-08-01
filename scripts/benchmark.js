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
const { close } = require('../tests/bridge')

const cases = [
  ['src/parser/glob.rs', 'src/**/*.rs'],
  ['packages/core/index.test.js', '**/!(*.test).js'],
  ['release-042.txt', 'release-{0..9}{0..9}{0..9}.txt'],
  ['foo/bar/baz.jsx', 'foo/bar/**/*.+(js|jsx)'],
]
const matchers = cases.map(([, pattern]) => pm(pattern))

for (let index = 0; index < 1_000; index++) {
  matchers[index % matchers.length](cases[index % cases.length][0])
}

const iterations = 25_000
let matches = 0
const started = performance.now()
for (let index = 0; index < iterations; index++) {
  if (matchers[index % matchers.length](cases[index % cases.length][0])) matches++
}
const elapsed = performance.now() - started

console.log(JSON.stringify({
  note: 'Persistent proof bridge included; non-comparative local measurement',
  iterations,
  matches,
  elapsedMs: Number(elapsed.toFixed(2)),
  operationsPerSecond: Math.round(iterations / (elapsed / 1_000)),
}, null, 2))

void close()
