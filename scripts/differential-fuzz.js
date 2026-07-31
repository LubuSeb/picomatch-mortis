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

const main = async () => {
  const mismatches = []
  let mismatchCount = 0
  try {
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
