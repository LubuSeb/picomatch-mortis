'use strict'

const path = require('node:path')

process.env.PICOMATCH_MORTIS_BIN = process.execPath
process.env.PICOMATCH_MORTIS_ARGS = JSON.stringify([path.join(__dirname, 'hanging-server.js')])
process.env.PICOMATCH_MORTIS_TIMEOUT_MS = '200'

const { call, close } = require('../tests/bridge')

const main = async () => {
  const started = Date.now()
  let timedOut = false
  try {
    call(['source', '*'])
  } catch (error) {
    timedOut = /exceeded/.test(error.message)
  } finally {
    await close()
  }
  const elapsed = Date.now() - started
  if (!timedOut) throw new Error('bridge call did not report its timeout')
  if (elapsed > 2_000) throw new Error(`bridge teardown took ${elapsed}ms`)
  console.log(`Bridge timeout and teardown completed in ${elapsed}ms.`)
}

main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
