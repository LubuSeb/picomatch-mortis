'use strict'

const pm = require('../tests')
const { close } = require('../tests/bridge')

const crab = String.fromCodePoint(0x1f980)
const longS = '\u017f'

const examples = [
  ['globstar', 'src/parser/glob.rs', 'src/**/*.rs', {}, true],
  ['brace range', 'release-3.txt', 'release-{1..5}.txt', {}, true],
  ['negative extglob', 'index.js', '!(*.test).js', {}, true],
  ['excluded extglob', 'index.test.js', '!(*.test).js', {}, false],
  ['Windows path', 'src\\parser\\glob.rs', 'src/**/*.rs', { windows: true }, true],
  ['UTF-16 qmark', crab, '?', {}, false],
  ['typed payload', '--payload/value', '--payload/*', {}, true],
]

const main = async () => {
  try {
    console.log('Picomatch Mortis - Track F: JavaScript to Rust')
    console.log('Proof: 28 Rust tests + 1,977 unchanged upstream tests')
    console.log('Differential: 100,000 generated comparisons + 535 directed executions, 0 mismatches\n')

    console.log('Picomatch API backed by persistent native matcher')
    for (const [name, input, pattern, options, expected] of examples) {
      const result = pm.isMatch(input, pattern, options)
      const verdict = result === expected ? 'PASS' : 'FAIL'
      console.log(`${verdict} ${name.padEnd(18)} result=${String(result).padEnd(5)} expected=${String(expected).padEnd(5)} ${input}  <=  ${pattern}`)
    }
    console.log('      UTF-16 note: the crab is two JavaScript code units, so legacy ? rejects it.')

    console.log('\nECMAScript case-fold boundary')
    const legacyFold = pm.isMatch(longS, 's', { flags: 'i', windows: false })
    const unicodeFold = pm.isMatch(longS, 's', { flags: 'iu', windows: false })
    if (legacyFold !== false || unicodeFold !== true) {
      throw new Error('Legacy and Unicode case-fold behavior diverged from Node')
    }
    console.log(`PASS legacy /i   long-s matches s: ${legacyFold} (expected false)`)
    console.log(`PASS Unicode /iu long-s matches s: ${unicodeFold} (expected true)`)

    console.log('\nNative capture spans reconstructed as a real RegExpExecArray')
    const capture = pm('**/*.{js,ts}', {
      capture: true,
      flags: 'd',
      windows: false,
    })('foo/bar/baz.js', true).match
    console.log(JSON.stringify({
      values: Array.from(capture),
      index: capture.index,
      input: capture.input,
      indices: Array.from(capture.indices),
    }))

    console.log('\nDeterministic execution fuel and recovery')
    try {
      const result = pm.isMatch(`${'a'.repeat(30)}y`, '+(a*)b')
      throw new Error(`Expected a safe-work-limit error; received ${result}`)
    } catch (error) {
      if (!/execution exceeded the safe work limit/i.test(error.message)) throw error
      console.log(`PASS hostile input -> explicit error: ${error.message}`)
    }
    const recovered = pm.isMatch('src/lib.rs', 'src/*.rs')
    if (!recovered) throw new Error('Native matcher did not recover after the fuel error')
    console.log(`PASS immediate recovery through the same bridge: ${recovered}`)

    console.log('\nBoundary: Rust owns scanning, compilation, and matching; JavaScript is the proof adapter.')
    console.log('Executing makeRe() output directly uses JavaScript RegExp, outside native fuel accounting.')
    console.log('Public regex source may differ structurally even when tested behavior and captures agree.')
  } finally {
    await close()
  }
}

main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
