'use strict'

const { call } = require('./bridge')
const scan = require('./lib/scan')

const optionArgs = options => {
  const args = []
  if (!options) return args
  if (typeof options.flags === 'string' && options.flags.includes('i')) args.push('--nocase')
  for (const [name, flag] of [
    ['windows', '--windows'], ['dot', '--dot'], ['nocase', '--nocase'],
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

const nativeMatch = (input, glob, options) => {
  const matched = call([...optionArgs(options), 'is-match', glob, input]) === 'true'
  return matched
}

const nativeOutput = (command, glob, options) => call([
  ...optionArgs(options), command, glob,
])

const picomatch = (glob, options = {}, returnState = false) => {
  if (typeof glob !== 'string' || glob.length === 0) throw new TypeError('Expected pattern to be a non-empty string')
  const state = returnState ? {
    input: glob,
    negated: !options.nonegate && glob.startsWith('!') && !glob.startsWith('!('),
    negatedExtglob: glob.startsWith('!('),
  } : undefined
  const matcher = (input, returnObject = false) => {
    if (typeof input !== 'string') throw new TypeError('Expected input to be a string')
    let output = typeof options.format === 'function' ? options.format(input) : input
    if (options.windows) output = output.replace(/\\/g, '/')
    const matched = input === glob || nativeMatch(output, glob, options)
    const ignored = matched && options.ignore
      ? [].concat(options.ignore).some(pattern => nativeMatch(output, pattern, options))
      : false
    const result = {
      glob,
      state,
      input,
      output,
      match: matched && !ignored ? [output] : null,
      isMatch: matched && !ignored,
    }
    if (typeof options.onResult === 'function') options.onResult(result)
    if (ignored && typeof options.onIgnore === 'function') options.onIgnore(result)
    if (result.isMatch && typeof options.onMatch === 'function') options.onMatch(result)
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
picomatch.isMatch = (input, patterns, options = {}) => {
  return [].concat(patterns).some(pattern => picomatch(pattern, options)(input))
}
picomatch.makeRe = (glob, options = {}) => {
  if (typeof glob !== 'string') throw new TypeError('Expected a non-empty string')
  let source = nativeOutput('source', glob, options)
  if (options.windows) {
    source = source.replace(/\[\^\/\]|\\\/|\//g, value => value === '[^/]' ? '[^\\\\/]' : '[\\\\/]')
  }
  return new RegExp(source, options.nocase ? 'i' : '')
}
picomatch.parse = (glob, options = {}) => {
  const encoded = nativeOutput('tokens', glob, options)
  const tokens = encoded === '' ? [] : encoded.split('\x1e').map(entry => {
    const [type, value, hasOutput, output] = entry.split('\x1f')
    return { type, value, output: hasOutput === 'true' ? output : undefined }
  })
  return { input: glob, output: nativeOutput('parse', glob, options), tokens }
}
picomatch.scan = scan

module.exports = picomatch
