'use strict'

const assert = require('node:assert')
const picomatch = require('../tests')
const { close } = require('../tests/bridge')

const main = async () => {
  try {
    for (const pattern of ['a(b', 'a[b', '*]']) {
      assert.throws(
        () => picomatch(pattern, { strictBrackets: true }),
        SyntaxError,
        `${JSON.stringify(pattern)} should preserve Picomatch's SyntaxError class`
      )
    }
    console.log('Verified strict-bracket adapter error classes.')
  } finally {
    await close()
  }
}

main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
