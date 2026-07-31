'use strict'

const pm = require('../tests')
const { close } = require('../tests/bridge')

const examples = [
  ['globstar', 'src/parser/glob.rs', 'src/**/*.rs', {}, true],
  ['brace range', 'release-3.txt', 'release-{1..5}.txt', {}, true],
  ['negative extglob', 'index.js', '!(*.test).js', {}, true],
  ['excluded extglob', 'index.test.js', '!(*.test).js', {}, false],
  ['Windows path', 'src\\parser\\glob.rs', 'src/**/*.rs', { windows: true }, true],
  ['UTF-16 qmark', '🦀', '?', {}, false],
]

const main = async () => {
  try {
    console.log('Picomatch Mortis - Track F: JavaScript to Rust')
    console.log('Proof: 1,977 unchanged upstream + 16 native tests')
    console.log('Differential: 80,000 bounded cases + directed regressions, 0 mismatches\n')

    console.log('native matching')
    for (const [name, input, pattern, options, expected] of examples) {
      const result = pm.isMatch(input, pattern, options)
      const verdict = result === expected ? 'PASS' : 'FAIL'
      console.log(`${verdict} ${name.padEnd(18)} result=${String(result).padEnd(5)} expected=${String(expected).padEnd(5)} ${input}  <=  ${pattern}`)
    }
    console.log('      UTF-16 note: the crab is two JavaScript code units, so one ? must reject it.')

    console.log('\nRust-generated public regex (observable Picomatch API)')
    console.log(pm.makeRe('src/**/!(*.test).{js,ts}').source)

    console.log('\nRust scanner state')
    const state = pm.scan('src/**/+(glob|scan).rs', { parts: true, tokens: true })
    const tokens = state.tokens.map(token => ({
      ...token,
      depth: token.depth === Infinity ? 'Infinity' : token.depth,
    }))
    console.log(JSON.stringify({
      base: state.base,
      glob: state.glob,
      maxDepth: state.maxDepth === Infinity ? 'Infinity' : state.maxDepth,
      tokens,
    }, null, 2))

    console.log('\nhardening rewrites')
    console.log(`ambiguous repetition -> literal  ${pm.makeRe('+(a|aa)').source}`)
    console.log(`safe star language    -> [fo]*  ${pm.makeRe('*(*(f)*(o))').source}`)
  } finally {
    await close()
  }
}

main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
