'use strict'

const assert = require('node:assert')
const picomatch = require('../tests')
const utils = require('../tests/lib/utils')
const { call, close } = require('../tests/bridge')

const main = async () => {
  try {
    for (const pattern of ['a(b', 'a[b', '*]']) {
      assert.throws(
        () => picomatch(pattern, { strictBrackets: true }),
        SyntaxError,
        `${JSON.stringify(pattern)} should preserve Picomatch's SyntaxError class`
      )
    }
    assert.equal(picomatch('[[]', { strictBrackets: true })('['), true)
    assert.equal(picomatch('}', { strictBrackets: true })('}'), true)
    assert.equal(picomatch('{', { strictBrackets: true, nobrace: true })('{'), true)
    assert.equal(picomatch('[', { strictBrackets: true, nobracket: true })('['), true)
    assert.throws(
      () => picomatch.matchBase('a', '{', { strictBrackets: true }),
      SyntaxError
    )

    assert.equal(picomatch.isMatch('--dot', '--dot'), true)
    assert.equal(picomatch.isMatch('scan', '--tokens'), false)
    assert.equal(picomatch.isMatch('--payload', '--payload'), true)
    assert.equal(picomatch.scan('--tokens', { tokens: true }).input, '--tokens')
    assert.equal(utils.basename('--windows', { windows: true }), '--windows')

    assert.equal(
      call(['match-span', '--capture', '--unicode', '--start', '0', '--payload', '@(a|(b))', 'a']),
      'M:0:1|0:1,-'
    )
    assert.equal(
      call(['match-span', '--start', '0', '--sticky', '--contains', '--payload', 'a', 'ba']),
      'N'
    )

    const astral = String.fromCodePoint(0x1f600)
    assert.equal(picomatch(astral, { maxLength: 2 })(astral), true)
    assert.throws(() => picomatch(astral, { maxLength: 1 }), SyntaxError)
    assert.throws(
      () => picomatch.isMatch(String.fromCharCode(0xd801), String.fromCharCode(0xd800)),
      /Lone UTF-16 surrogates/
    )

    for (const flags of ['x', 'ii']) {
      assert.equal(picomatch.isMatch('a', '*', { flags }), false)
    }
    assert.equal(picomatch.isMatch('a', 'a', { flags: 'x' }), true)
    assert.equal(picomatch.makeRe('*', { flags: 'x' }).source, '$^')
    assert.throws(() => picomatch('*', { flags: 'x', debug: true }), SyntaxError)
    assert.equal(picomatch.isMatch('a', '*', { flags: 'd' }), true)
    assert.equal(picomatch.isMatch('\na\n', 'a', { flags: 'm' }), true)
    assert.equal(picomatch.isMatch('\n', '*', { flags: 's' }), true)
    assert.equal(picomatch.isMatch('ba', 'a', { contains: true, flags: 'g' }), true)
    assert.equal(picomatch.isMatch('ba', 'a', { contains: true, flags: 'y' }), false)
    assert.equal(picomatch.isMatch('ab', 'a', { contains: true, flags: 'y' }), true)
    assert.equal(picomatch.isMatch(astral, '?', { flags: 'v' }), true)
    assert.equal(picomatch.isMatch('a', '*', { flags: 'v' }), false)
    assert.equal(picomatch.makeRe('*', { flags: 'v' }).flags, '')
    assert.equal(picomatch.isMatch('b', '[^a]', { flags: 'v' }), false)
    assert.equal(picomatch.isMatch('b', '[!a]', { flags: 'v', posix: true }), false)

    const braceMatch = picomatch('*.{js,ts}')('foo.js', true).match
    assert.deepEqual(Array.from(braceMatch), ['foo.js', 'js'])
    assert.equal(braceMatch.index, 0)
    assert.equal(braceMatch.input, 'foo.js')

    const generatedCaptures = picomatch('**/*.{js,ts}', { capture: true })(
      'foo/bar/baz.js',
      true
    ).match
    assert.deepEqual(
      Array.from(generatedCaptures),
      ['foo/bar/baz.js', 'foo/bar', 'baz', 'js']
    )
    assert.equal(picomatch.isMatch('x/a/x', '*/(*)/\\1', { capture: true }), true)
    assert.equal(picomatch.isMatch('x/a/a', '*/(*)/\\1', { capture: true }), false)
    assert.equal(picomatch.isMatch('a/x/a', '[ab]/(*)/\\1', { capture: true }), true)
    assert.equal(
      picomatch.isMatch('a/x/x', '[ab]/(*)/\\1', {
        capture: true,
        literalBrackets: false,
      }),
      true
    )

    assert.equal(picomatch.makeRe('!(a|b)', { capture: true }).source, '$^')
    assert.equal(picomatch.isMatch('c', '!(a|b)', { capture: true }), false)
    assert.throws(() => picomatch('!(a|b)', { capture: true, debug: true }), SyntaxError)
    assert.deepEqual(
      Array.from(picomatch('a!(b)c', { capture: true })('aac', true).match),
      ['aac', 'a']
    )

    const ignoredResult = picomatch('*', { ignore: 'b' })('b', true)
    assert.equal(ignoredResult.isMatch, false)
    assert.deepEqual(Array.from(ignoredResult.match), ['b'])
    assert.equal(picomatch('a')('a', true).match, true)
    assert.equal(picomatch('*')('', true).match, undefined)
    assert.equal(picomatch('*.js', { basename: true })('foo/bar.js', true).match, true)
    assert.equal(picomatch.makeRe('a/*.js').test('a/b.js/'), false)
    assert.equal(picomatch.makeRe('{!(a),b}', { windows: false }).test(''), true)

    const named = picomatch('(?<letter>a)', { capture: true, flags: 'd' })('a', true).match
    assert.equal(named.groups.letter, 'a')
    assert.deepEqual(named.indices.groups.letter, [0, 1])
    const duplicateNamed = picomatch('(?:(?<letter>a)|(?<letter>b))', {
      capture: true,
      flags: 'd',
      windows: false,
    })
    for (const input of ['a', 'b']) {
      const match = duplicateNamed(input, true).match
      assert.equal(match.groups.letter, input)
      assert.deepEqual(match.indices.groups.letter, [0, 1])
    }
    assert.deepEqual(
      Array.from(picomatch('a/**', { capture: true, windows: true })('a//b', true).match),
      ['a//b', '/b']
    )
    const repeatedSeparators = picomatch('a/**/*', { capture: true, windows: false })
    assert.deepEqual(Array.from(repeatedSeparators('a//b', true).match), ['a//b', '', 'b'])
    assert.deepEqual(Array.from(repeatedSeparators('a///b', true).match), ['a///b', '/', 'b'])

    const globalContains = picomatch('a', { contains: true, flags: 'g', windows: false })
    assert.deepEqual(
      ['ba', 'ba', 'ba'].map(input => globalContains(input)),
      [true, false, true]
    )
    const stickyContains = picomatch('a', { contains: true, flags: 'y', windows: false })
    assert.deepEqual(
      ['ba', 'ab', 'ab'].map(input => stickyContains(input)),
      [false, true, false]
    )
    const globalAstral = picomatch('*', { capture: true, flags: 'gu', windows: false })
    assert.deepEqual(
      [astral, 'a', 'a'].map(input => globalAstral(input)),
      [true, false, true]
    )
    assert.throws(() => picomatch('a', { maxLength: 3, ignore: 'abcd' }), SyntaxError)
    assert.throws(
      () => picomatch('a'.repeat(65_537), { maxLength: 100_000 }),
      SyntaxError
    )

    assert(picomatch.makeRe('?'.repeat(40_000)) instanceof RegExp)
    assert.equal(picomatch.isMatch('a'.repeat(110_000), '*'), true)
    assert.throws(
      () => picomatch.isMatch(`${'a'.repeat(30)}y`, '+(a*)b'),
      /execution exceeded the safe work limit/
    )
    assert.equal(picomatch.isMatch('a', 'a'), true)
    for (const delimiter of [String.fromCharCode(0x1e), String.fromCharCode(0x1f)]) {
      const parsed = picomatch.parse(delimiter)
      assert.equal(parsed.tokens.length, 2)
      assert.equal(parsed.tokens[1].value, delimiter)
    }
    assert.equal(picomatch.isMatch('a', 'a'), true)
    const maximumAstralPattern = astral.repeat(32_768)
    assert.equal(picomatch(maximumAstralPattern)(maximumAstralPattern), true)
    assert.throws(
      () => call(['source', '--payload', 'x'.repeat(2_100_000)]),
      RangeError
    )
    assert.equal(picomatch.isMatch('a', 'a'), true)
  } catch (error) {
    await close()
    throw error
  }

  const firstClose = close()
  assert.strictEqual(close(), firstClose)
  assert.throws(() => call(['source', '--payload', 'a']), /bridge is closed/)
  await firstClose
  assert.throws(() => call(['source', '--payload', 'a']), /bridge is closed/)
  console.log('Verified adapter errors, typed payloads, capacity, recovery, and teardown.')
}

main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
