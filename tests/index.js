'use strict'

const { call } = require('./bridge')
const scan = require('./lib/scan')

const optionArgs = options => {
  const args = []
  if (!options) return args
  const flags = typeof options.flags === 'string' ? options.flags : null
  if (flags ? flags.includes('i') : options.nocase === true) args.push('--nocase')
  if (flags && flags.includes('u')) args.push('--unicode')
  if (flags && flags.includes('v')) args.push('--unicode-sets')
  if (flags && flags.includes('m')) args.push('--multiline')
  if (flags && flags.includes('s')) args.push('--dot-all')
  for (const [name, flag] of [
    ['windows', '--windows'], ['posix', '--posix'], ['dot', '--dot'],
    ['contains', '--contains'], ['nonegate', '--nonegate'], ['noextglob', '--noextglob'],
    ['noext', '--noext'], ['noglobstar', '--noglobstar'], ['nobrace', '--nobrace'],
    ['nobracket', '--nobracket'], ['strictSlashes', '--strict-slashes'], ['bash', '--bash'],
    ['basename', '--basename'], ['matchBase', '--match-base'],
    ['keepQuotes', '--keep-quotes'], ['strictBrackets', '--strict-brackets'],
    ['regex', '--regex'],
    ['unescape', '--unescape'],
    ['capture', '--capture'],
  ]) {
    if (options[name] === true && !args.includes(flag)) args.push(flag)
  }
  if (typeof options.literalBrackets === 'boolean') {
    args.push('--literal-brackets', String(options.literalBrackets))
  }
  if (Number.isSafeInteger(options.maxLength) && options.maxLength >= 0) {
    args.push('--max-length', String(options.maxLength))
  }
  if (options.maxExtglobRecursion === false) {
    args.push('--unbounded-extglob-recursion')
  } else if (Number.isSafeInteger(options.maxExtglobRecursion) && options.maxExtglobRecursion >= 0) {
    args.push('--max-extglob-recursion', String(options.maxExtglobRecursion))
  }
  return args
}

const expandRanges = (glob, options) => {
  if (!options || typeof options.expandRange !== 'function') return glob
  return glob.replace(/\{([^{}]+?)\.\.([^{}]+?)(?:\.\.([^{}]+?))?\}/g, (_, start, end, step) => {
    return options.expandRange(start, end, ...(step === undefined ? [] : [step]), options)
  })
}

const nativeCall = (args, options) => {
  try {
    return call(args)
  } catch (error) {
    if (
      (options && options.strictBrackets && /Missing (?:opening|closing):/.test(error.message)) ||
      /Input length: \d+, exceeds maximum allowed length: \d+/.test(error.message)
    ) {
      throw new SyntaxError(error.message)
    }
    throw error
  }
}

const nativeMatch = (input, glob, options, originalInput = input) => {
  const payload = [expandRanges(glob, options), input]
  if (originalInput !== input) payload.push(originalInput)
  const matched = nativeCall(
    ['is-match', ...optionArgs(options), '--payload', ...payload],
    options
  ) === 'true'
  return matched
}

const nativeOutput = (command, glob, options) => nativeCall(
  [command, ...optionArgs(options), '--payload', expandRanges(glob, options)],
  options
)

const effectiveFlags = options => options.flags || (options.nocase ? 'i' : '')

const analyzeFlags = options => {
  try {
    const probe = new RegExp('', effectiveFlags(options))
    return { valid: true, flags: probe.flags }
  } catch (error) {
    return { valid: false, error }
  }
}

const namedCaptureIndexes = source => {
  const names = []
  let captureIndex = 0
  let inClass = false
  for (let index = 0; index < source.length; index++) {
    if (source[index] === '\\') {
      index++
      continue
    }
    if (source[index] === '[') {
      inClass = true
      continue
    }
    if (source[index] === ']' && inClass) {
      inClass = false
      continue
    }
    if (inClass || source[index] !== '(') continue
    if (source[index + 1] !== '?') {
      captureIndex++
      continue
    }
    if (source[index + 2] !== '<' || source[index + 3] === '=' || source[index + 3] === '!') {
      continue
    }
    const end = source.indexOf('>', index + 3)
    if (end === -1) continue
    captureIndex++
    names.push({ name: source.slice(index + 3, end), index: captureIndex })
  }
  return names
}

const preparePattern = (glob, options) => {
  const analysis = analyzeFlags(options)
  const sourceFlags = analysis.valid ? analysis.flags : ''
  const sourceOptions = { ...options, flags: sourceFlags, nocase: false }
  const pattern = expandRanges(glob, options)
  const source = nativeCall(
    ['source', ...optionArgs(sourceOptions), '--payload', pattern],
    sourceOptions
  )
  if (analysis.valid && source === '$^') {
    if (options.debug === true) throw new SyntaxError('Invalid generated regular expression')
    return { glob, pattern, options: sourceOptions, valid: false, global: false, sticky: false, lastIndex: 0, regex: /$^/ }
  }
  if (!analysis.valid) {
    if (options.debug === true) throw analysis.error
    return { glob, pattern, options: sourceOptions, valid: false, global: false, sticky: false, lastIndex: 0, regex: /$^/ }
  }
  try {
    const regex = new RegExp(source, analysis.flags)
    return {
      glob,
      pattern,
      options: { ...options, flags: regex.flags, nocase: false },
      valid: true,
      global: regex.global,
      sticky: regex.sticky,
      lastIndex: 0,
      regex,
      namedCaptures: namedCaptureIndexes(regex.source),
    }
  } catch (error) {
    if (options.debug === true) throw error
    return { glob, pattern, options: sourceOptions, valid: false, global: false, sticky: false, lastIndex: 0, regex: /$^/ }
  }
}

// `match-span` returns UTF-16 offsets as
// `M:<start>:<end>|<capture>,...`; `-` is an unmatched capture and `N`
// is the distinct no-match marker.
const decodeMatchSpan = output => {
  if (output === 'N') return null
  const match = /^M:(\d+):(\d+)\|(.*)$/.exec(output)
  if (!match) throw new Error('invalid native match-span response')
  const captures = match[3] === '' ? [] : match[3].split(',').map(value => {
    if (value === '-') return null
    const capture = /^(\d+):(\d+)$/.exec(value)
    if (!capture) throw new Error('invalid native capture span')
    return { start: Number(capture[1]), end: Number(capture[2]) }
  })
  return {
    start: Number(match[1]),
    end: Number(match[2]),
    captures,
  }
}

const createExecArray = (input, span, prepared) => {
  const match = [
    input.slice(span.start, span.end),
    ...span.captures.map(capture => capture === null
      ? undefined
      : input.slice(capture.start, capture.end)),
  ]
  match.index = span.start
  match.input = input
  if (prepared.namedCaptures.length > 0) {
    match.groups = Object.create(null)
    for (const capture of prepared.namedCaptures) {
      if (match[capture.index] !== undefined || !(capture.name in match.groups)) {
        match.groups[capture.name] = match[capture.index]
      }
    }
  } else {
    match.groups = undefined
  }
  if (prepared.regex.hasIndices) {
    match.indices = [
      [span.start, span.end],
      ...span.captures.map(capture => capture === null
        ? undefined
        : [capture.start, capture.end]),
    ]
    if (prepared.namedCaptures.length > 0) {
      match.indices.groups = Object.create(null)
      for (const capture of prepared.namedCaptures) {
        if (match.indices[capture.index] !== undefined || !(capture.name in match.indices.groups)) {
          match.indices.groups[capture.name] = match.indices[capture.index]
        }
      }
    } else {
      match.indices.groups = undefined
    }
  }
  return match
}

const regexStartIndex = regex => {
  if (!regex.global && !regex.sticky) return 0
  if (typeof regex.lastIndex === 'bigint' || typeof regex.lastIndex === 'symbol') {
    throw new TypeError('Cannot convert lastIndex to a number')
  }
  const value = Number(regex.lastIndex)
  if (Number.isNaN(value) || value <= 0) return 0
  if (value === Infinity) return Number.MAX_SAFE_INTEGER
  return Math.min(Math.trunc(value), Number.MAX_SAFE_INTEGER)
}

const matchPrepared = (prepared, input, originalInput = input) => {
  if (input === '') return { isMatch: false, match: undefined }
  if (!prepared.options.capture && (originalInput === prepared.glob || input === prepared.glob)) {
    return { isMatch: true, match: true }
  }
  if (!prepared.valid) return { isMatch: false, match: null }
  const start = regexStartIndex(prepared.regex)
  const output = nativeCall([
    'match-span',
    ...optionArgs(prepared.options),
    '--start', String(start),
    ...(prepared.sticky ? ['--sticky'] : []),
    '--payload', prepared.pattern, input,
  ])
  const span = decodeMatchSpan(output)
  if (span === null) {
    if (prepared.global || prepared.sticky) {
      prepared.lastIndex = 0
      prepared.regex.lastIndex = 0
    }
    return { isMatch: false, match: null }
  }
  if (prepared.global || prepared.sticky) {
    prepared.lastIndex = span.end
    prepared.regex.lastIndex = span.end
  }
  const match = prepared.options.basename || prepared.options.matchBase
    ? true
    : createExecArray(input, span, prepared)
  return { isMatch: true, match }
}

const picomatch = function (glob, options, returnState = false) {
  if (Array.isArray(glob)) {
    const matchers = glob.map(pattern => picomatch(pattern, options, returnState))
    return input => matchers.some(matcher => matcher(input))
  }
  if (typeof glob !== 'string' || glob.length === 0) throw new TypeError('Expected pattern to be a non-empty string')
  if (options && options.windows == null && process.platform === 'win32') {
    options = { ...options, windows: true }
  }
  options ||= {}
  const prepared = preparePattern(glob, options)
  const ignorePatterns = options.ignore ? [].concat(options.ignore) : []
  const ignoreOptions = { ...options, ignore: null, onMatch: null, onResult: null }
  const preparedIgnores = ignorePatterns.map(pattern => preparePattern(pattern, ignoreOptions))
  const state = returnState ? {
    input: glob,
    negated: !options.nonegate && glob.startsWith('!') && !glob.startsWith('!('),
    negatedExtglob: glob.startsWith('!('),
  } : undefined
  const matcher = (input, returnObject = false) => {
    if (typeof input !== 'string') throw new TypeError('Expected input to be a string')
    let output = typeof options.format === 'function' ? options.format(input) : input
    if (options.windows) output = output.replace(/\\/g, '/')
    const matched = matchPrepared(prepared, output, input)
    const result = {
      glob,
      state,
      regex: prepared.regex,
      posix: options.windows,
      input,
      output,
      match: matched.match,
      isMatch: matched.isMatch,
    }
    if (typeof options.onResult === 'function') options.onResult(result)
    if (!result.isMatch) return returnObject ? result : false

    const ignored = preparedIgnores.some(pattern => matchPrepared(pattern, output, input).isMatch)
    if (ignored) {
      if (typeof options.onIgnore === 'function') options.onIgnore(result)
      result.isMatch = false
      return returnObject ? result : false
    }
    if (typeof options.onMatch === 'function') options.onMatch(result)
    return returnObject ? result : result.isMatch
  }
  matcher.state = state
  return matcher
}

picomatch.test = (input, regex) => {
  const match = regex.exec(input)
  return { isMatch: Boolean(match), match, output: input }
}
picomatch.matchBase = (input, glob, options = {}) => nativeMatch(input, glob, { ...options, basename: true })
picomatch.isMatch = (input, patterns, options) => {
  const normalized = options && options.windows == null ? { ...options, windows: false } : options
  return [].concat(patterns).some(pattern => picomatch(pattern, normalized)(input))
}
picomatch.makeRe = (glob, options = {}) => {
  if (typeof glob !== 'string') throw new TypeError('Expected a non-empty string')
  return preparePattern(glob, options).regex
}
picomatch.parse = (glob, options = {}) => {
  const output = nativeOutput('parse', glob, options)
  const encoded = nativeOutput('tokens', glob, options)
  const tokens = encoded === '' ? [] : encoded.split('\x1e').map(entry => {
    const [type, value, hasOutput, output] = entry.split('\x1f')
    const decode = field => Buffer.from(field, 'hex').toString()
    return {
      type: decode(type),
      value: decode(value),
      output: hasOutput === 'true' ? decode(output) : undefined,
    }
  })
  return { input: glob, output, tokens }
}
picomatch.scan = scan

module.exports = picomatch
