'use strict'

const { execFileSync } = require('node:child_process')
const path = require('node:path')

const binary = process.env.PICOMATCH_MORTIS_BIN || path.join(
  __dirname,
  '..',
  '..',
  'target',
  'debug',
  process.platform === 'win32' ? 'picomatch-mortis.exe' : 'picomatch-mortis'
)

const decode = value => Buffer.from(value, 'hex').toString()

module.exports = (input, options = {}) => {
  const args = ['scan', input]
  if (options.scanToEnd) args.push('--scan-to-end')
  if (options.parts) args.push('--parts')
  if (options.tokens) args.push('--tokens')
  if (options.noext) args.push('--noext')
  if (options.nonegate) args.push('--nonegate')
  if (options.noparen) args.push('--noparen')
  if (options.unescape) args.push('--unescape')

  const fields = execFileSync(binary, args, { encoding: 'utf8' }).trimEnd().split('\t')
  const state = {
    prefix: decode(fields[0]),
    input: decode(fields[1]),
    start: Number(fields[2]),
    base: decode(fields[3]),
    glob: decode(fields[4]),
    isBrace: fields[5] === 'true',
    isBracket: fields[6] === 'true',
    isGlob: fields[7] === 'true',
    isExtglob: fields[8] === 'true',
    isGlobstar: fields[9] === 'true',
    negated: fields[10] === 'true',
    negatedExtglob: fields[11] === 'true',
  }

  if (options.parts || options.tokens) {
    state.slashes = fields[12] ? fields[12].split(',').map(Number) : []
    state.parts = fields[13] ? fields[13].split(',').map(decode) : []
  }
  if (options.tokens) {
    state.maxDepth = fields[14] === 'inf' ? Infinity : Number(fields[14])
    state.tokens = fields[15] ? fields[15].split(',').map(entry => {
      const [value, depth, isGlob, isPrefix, isGlobstar, isBrace, isBracket, isExtglob, negated, backslashes] = entry.split(':')
      const token = {
        value: decode(value),
        depth: depth === 'inf' ? Infinity : Number(depth),
        isGlob: isGlob === 'true',
      }
      for (const [name, flag] of [
        ['isPrefix', isPrefix], ['isGlobstar', isGlobstar], ['isBrace', isBrace],
        ['isBracket', isBracket], ['isExtglob', isExtglob], ['negated', negated],
        ['backslashes', backslashes],
      ]) {
        if (flag === 'true') token[name] = true
      }
      return token
    }) : []
  }

  return state
}
