'use strict'

const { call } = require('./bridge')
const scan = require('./lib/scan')

const optionArgs = options => {
  const args = []
  if (!options) return args
  for (const [name, flag] of [
    ['windows', '--windows'], ['dot', '--dot'], ['nocase', '--nocase'],
    ['contains', '--contains'], ['nonegate', '--nonegate'], ['noextglob', '--noextglob'],
    ['noext', '--noext'], ['noglobstar', '--noglobstar'], ['nobrace', '--nobrace'],
    ['nobracket', '--nobracket'], ['strictSlashes', '--strict-slashes'], ['bash', '--bash'],
    ['basename', '--basename'], ['matchBase', '--match-base'],
    ['keepQuotes', '--keep-quotes'], ['strictBrackets', '--strict-brackets'],
    ['regex', '--regex'],
    ['unescape', '--unescape'],
  ]) {
    if (options[name] === true) args.push(flag)
  }
  if (typeof options.literalBrackets === 'boolean') {
    args.push('--literal-brackets', String(options.literalBrackets))
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
  if (typeof glob !== 'string') throw new TypeError('Expected pattern to be a non-empty string')
  const state = returnState ? { input: glob, negated: !options.nonegate && glob.startsWith('!') } : undefined
  const matcher = (input, returnObject = false) => {
    if (typeof input !== 'string') throw new TypeError('Expected input to be a string')
    const output = typeof options.format === 'function' ? options.format(input) : input
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
  const output = typeof options.format === 'function' ? options.format(input) : input
  return [].concat(patterns).some(pattern => input === pattern || nativeMatch(output, pattern, options))
}
picomatch.makeRe = (glob, options = {}) => {
  if (typeof glob !== 'string') throw new TypeError('Expected a non-empty string')
  let source = nativeOutput('source', glob, options)
  if (options.windows) {
    source = source.replace(/\[\^\/\]|\\\/|\//g, value => value === '[^/]' ? '[^\\\\/]' : '[\\\\/]')
  }
  return new RegExp(source, options.nocase ? 'i' : '')
}
picomatch.parse = (glob, options = {}) => ({ input: glob, output: nativeOutput('parse', glob, options) })
picomatch.scan = scan

module.exports = picomatch
