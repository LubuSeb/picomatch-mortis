'use strict'

const reference = require('picomatch-reference/posix')
const port = require('../tests')
const { close } = require('../tests/bridge')

const DEFAULT_CASES = 50_000
const DEFAULT_SEED = 0x504d3236

const caseCount = Number.parseInt(process.argv[2] || DEFAULT_CASES, 10)
const seed = Number.parseInt(process.argv[3] || DEFAULT_SEED, 10) >>> 0

if (!Number.isSafeInteger(caseCount) || caseCount < 1) {
  throw new TypeError('case count must be a positive integer')
}

let state = seed
const random = () => {
  state ^= state << 13
  state ^= state >>> 17
  state ^= state << 5
  return state >>> 0
}
const integer = maximum => random() % maximum
const chance = denominator => integer(denominator) === 0
const pick = values => values[integer(values.length)]

const literal = () => pick(['a', 'b', 'c', 'x', '0', '9', '.', '-', '_', '😀'])
const word = () => pick(['a', 'b', 'c', 'x', 'foo', 'bar', 'src', 'js', '😀'])

const pattern = () => {
  const left = word()
  let right = word()
  while (right === left) right = word()
  let output = pick([
    '*', '?', '**', `${left}*`, `${left}?${right}`, `*.${right}`,
    '[abc]', '[a-c]', '[!ab]', '[[:digit:]]',
    `{${left},${right}}`, '{1..3}',
    `@(${left}|${right})`, `?(${left}|${right})`, `+(${left}|${right})`,
    `*(${left}|${right})`, `!(${left}|${right})`,
    `@(${left}|${right})**@(${right}|${left})`,
    `${left}/?(${left}|${right})**`, `***(${left}|${right})`,
    `+(${left}|?)`,
    `${left}/!(${left}|${right})`, `@(${left}|${right})**`,
    `${left}/*/${right}`, `${left}/**/${right}`, `${left}/**`,
    `**/${left}`, `**/${left}?.${right}`, `**/*.${right}`,
  ])
  if (chance(11)) output = `./${output}`
  if (chance(13) && !output.startsWith('!(')) output = `!${output}`
  return output
}

const inputSegment = () => {
  const length = integer(9)
  let output = chance(8) ? '.' : ''
  for (let index = 0; index < length; index += 1) output += literal()
  return output
}

const input = windows => {
  const segments = []
  const count = 1 + integer(3)
  for (let index = 0; index < count; index += 1) segments.push(inputSegment())
  let output = segments.join('/')
  if (windows && chance(2)) output = output.replaceAll('/', '\\')
  return output
}

const options = () => {
  const value = {}
  for (const name of [
    'dot', 'nocase', 'contains', 'nonegate', 'noglobstar',
    'nobrace', 'nobracket', 'strictSlashes', 'bash', 'windows', 'posix',
  ]) {
    if (chance(13)) value[name] = true
  }
  if (chance(17)) value.literalBrackets = chance(2)
  if (chance(29)) value.flags = pick([
    'i', 'u', 'iu', 'd', 'g', 'y', 'm', 's', 'gm', 'my', 'v', 'iv', 'x', 'ii',
  ])
  return value
}

const outcome = operation => {
  try {
    return { kind: 'value', value: Boolean(operation()) }
  } catch (error) {
    return { kind: 'error', value: error && error.constructor && error.constructor.name }
  }
}

const equal = (left, right) => left.kind === right.kind && left.value === right.value

const directedCases = [
  ['/', '[^a]*', { regex: true }],
  ['a/b/c', 'a/!(b)', { contains: true }],
  ['ab', '!(!(a))', { contains: true }],
  ['.a', '**/*a', { contains: true }],
  ['x/.a', '**/*a', { contains: true }],
  ['.a', '**/*', { contains: true }],
  ['a/', '**?(a)', {}],
  ['a/b', '**?(a)', {}],
  ['x/a', '**@(a)', {}],
  ['b', '***(a)', {}],
  ['a/a', '**{a,b}', {}],
  ['a/a/a', '!(b)**/*', {}],
  ['b/a', '!(b)**/*', {}],
  ['a/.x/y', '!(b)**/*', { dot: true }],
  ['a/.x/y', '!(b)**/*', {}],
  ['ab/.9.x', '!a*', { bash: true }],
  ['.😀aa.a_._//9aa', '**/*.a', { contains: true }],
  ['.-xcb-.😀/.c_xc0ca9/_9x', '**/*.😀', { contains: true }],
  ['a/b', '@(a|b)**@(a|b)', {}],
  ['ab/', '?b/?(a|b)**', {}],
  ['x', '***(a|b)', { bash: true }],
  ['x', '!!(a|b)', { noextglob: true }],
  ['b', './!a', {}],
  ['b', '!(a|?)', {}],
  ['x', '!(a|b)', { noextglob: true }],
  ['a/', 'a/?(b)', {}],
  ['a/', 'a/*(b)', {}],
  ['a', '@(?|a)', {}],
  ['a', '+(a|?)', {}],
  ['a', '+(a|a)', {}],
  ['@x', '@(?|a)', {}],
  ['a', '*(?)', {}],
  ['aa', '*(?a|b)', {}],
  ['b', '*(?a|b)', {}],
  ['x/{a,b}', '**{a,b}', { nobrace: true }],
  ['@x/x@a', '@(x)**@(a)', { noextglob: true }],
  ['ſ', 's', { nocase: true }],
  ['ı', 'i', { nocase: true }],
  ['K', '[a-z]', { nocase: true }],
  ['ſ', '[\uE000-\uF8FF]', { nocase: true }],
  ['ſ', '[^\uE000-\uF8FF]', { nocase: true }],
  ['Ȁ', '[\u0100-\u017F]', { nocase: true }],
  ['--dot', '--dot', {}],
  ['scan', '--tokens', {}],
  ['--payload', '--payload', {}],
  ['(', '[' + '('.repeat(65) + ']', {}],
  ['a', '{', { strictBrackets: true }],
  ['[', '[[]', { strictBrackets: true }],
  ['}', '}', { strictBrackets: true }],
  ['{', '{', { strictBrackets: true, nobrace: true }],
  ['[', '[', { strictBrackets: true, nobracket: true }],
  ['!', '[!]', { strictBrackets: true }],
  ['a', '[[:alpha:]', { strictBrackets: true }],
  ['a', '!a/**', {}],
  ['abxx_.c/', '!**/*.c', {}],
  ['?a', '(?(a))', {}],
  ['@?a', '@(?(a))', {}],
  ['a', '@(x|?(a))', {}],
  ['?', '!!(?)', { noextglob: true }],
  ['a', '!!!(?)', { noextglob: true }],
  ['a|', 'a||b', {}],
  ['b', 'x/a|b', {}],
  ['x/a', '**(a|b)', { noextglob: true }],
  ['a', '**/*(a|b)', { noglobstar: true }],
  ['x/a', '**/*(a|b)', { noglobstar: true }],
  ['!', '[^]]', {}],
  ['a', '[([:]a', { strictBrackets: true }],
  ['ab', '*(ab|abab)', {}],
  ['a|b', '*(a\\|b|@(a|b))', {}],
  ['A', 'a', { nocase: true, flags: 'u' }],
  ['\u017f', 's', { flags: 'iu' }],
  ['s', '\u017f', { flags: 'iu' }],
  ['s', '@(\u017f)', { flags: 'iu' }],
  ['s', '{\u017f,x}', { flags: 'iu' }],
  ['\ud83d\ude00\ud83d\ude00abc/b_9', '@(b|\ud83d\ude00)**', { flags: 'u' }],
  ['\ud83d\ude00\ud83d\ude00\ud83d\ude00-/9_b.x_9/.\ud83d\ude00', '@(bar|\ud83d\ude00)**@(\ud83d\ude00|bar)', { bash: true }],
  ['\u1e9e', '[\u00df]', { nocase: true }],
  ['\u00c5', '[\u212b]', { nocase: true }],
  ['\u019b', '\ua7dc', { nocase: true }],
  ['\u1c8a', '\u1c89', { nocase: true }],
  ['._\ud83d\ude00__', '{\ud83d\ude00,b}', { contains: true, literalBrackets: false, flags: 'iu' }],
  ['a', '*', { flags: 'x' }],
  ['a', '*', { flags: 'ii' }],
  ['a', 'a', { flags: 'x' }],
  ['a', '*', { flags: 'd' }],
  ['\na\n', 'a', { flags: 'm' }],
  ['\n', '*', { flags: 's' }],
  ['ba', 'a', { contains: true, flags: 'g' }],
  ['ba', 'a', { contains: true, flags: 'y' }],
  ['ab', 'a', { contains: true, flags: 'y' }],
  ['\ud83d\ude00', '?', { flags: 'v' }],
  ['a', '*', { flags: 'v' }],
  ['b', '[^a]', { flags: 'v' }],
  ['b', '[!a]', { flags: 'v', posix: true }],
  ['A', 'a', { flags: 'iv' }],
  ['c___-._/', '!c*', { nobrace: true, posix: true }],
  ['abc', '@(!(!(a))|b)', {}],
  ['abc', '@(!(!(a))|b)', { capture: true }],
  ['abc', '{!(!(a)),b}', {}],
  ['ab.js', '!(!(a)).js', {}],
  ['ab/x', '!(!(a))/x', {}],
  ['xaby', 'x!(!(a))y', {}],
  ['a', '?', { flags: 'u', windows: true }],
  ['a//b', 'a/**/*', {}],
  ['a//b', 'a/**/*', { capture: true }],
  ['a///b', 'a/**/*', {}],
  ['a///b', 'a/**/*', { capture: true, windows: true }],
]

const main = async () => {
  const mismatches = []
  let mismatchCount = 0
  try {
    for (const [candidate, glob, opts] of directedCases) {
      const expected = outcome(() => reference.isMatch(candidate, glob, opts))
      const actual = outcome(() => port.isMatch(candidate, glob, opts))
      if (!equal(expected, actual)) {
        mismatchCount += 1
        mismatches.push({ directed: true, glob, input: candidate, options: opts, expected, actual })
      }
    }
    for (let index = 0; index < caseCount; index += 1) {
      const opts = options()
      const glob = pattern()
      const candidate = input(opts.windows === true)
      const expected = outcome(() => reference.isMatch(candidate, glob, opts))
      const actual = outcome(() => port.isMatch(candidate, glob, opts))
      if (!equal(expected, actual)) {
        mismatchCount += 1
        if (mismatches.length < 20) {
          mismatches.push({ index, glob, input: candidate, options: opts, expected, actual })
        }
      }
    }
  } finally {
    await close()
  }

  console.log(`Differential seed: 0x${seed.toString(16).padStart(8, '0')}`)
  console.log(`Directed cases compared: ${directedCases.length.toLocaleString('en-US')}`)
  console.log(`Cases compared: ${caseCount.toLocaleString('en-US')}`)
  console.log(`Mismatches: ${mismatchCount}`)
  if (mismatchCount > 0) {
    console.error(JSON.stringify(mismatches, null, 2))
    process.exitCode = 1
  }
}

main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
