'use strict'

const path = require('node:path')
const { Worker, isMainThread, parentPort, workerData } = require('node:worker_threads')

const CAPACITY = 256 * 1024

if (!isMainThread) {
  const { spawn } = require('node:child_process')
  const { once } = require('node:events')
  const readline = require('node:readline')
  const control = new Int32Array(workerData.shared, 0, 2)
  const bytes = new Uint8Array(workerData.shared, 8)
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()
  const child = spawn(workerData.binary, ['serve'], {
    stdio: ['pipe', 'pipe', 'inherit'],
    windowsHide: true,
  })
  const lines = readline.createInterface({ input: child.stdout })
  parentPort.on('close', () => child.kill())

  const loop = async () => {
    Atomics.store(control, 0, 0)
    Atomics.notify(control, 0)
    while (true) {
      Atomics.wait(control, 0, 0)
      if (Atomics.load(control, 0) === -1) {
        child.kill()
        return
      }
      const request = decoder.decode(bytes.subarray(0, Atomics.load(control, 1)))
      child.stdin.write(request)
      const [response] = await once(lines, 'line')
      const encoded = encoder.encode(response)
      bytes.set(encoded)
      Atomics.store(control, 1, encoded.length)
      Atomics.store(control, 0, 2)
      Atomics.notify(control, 0)
      Atomics.wait(control, 0, 2)
    }
  }
  loop().catch(error => {
    const encoded = encoder.encode(`error\t${Buffer.from(error.stack).toString('hex')}`)
    bytes.set(encoded)
    Atomics.store(control, 1, encoded.length)
    Atomics.store(control, 0, 2)
    Atomics.notify(control, 0)
  })
} else {
  const shared = new SharedArrayBuffer(CAPACITY + 8)
  const control = new Int32Array(shared, 0, 2)
  const bytes = new Uint8Array(shared, 8)
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()
  const binary = process.env.PICOMATCH_MORTIS_BIN || path.join(
    __dirname,
    '..',
    'target',
    'debug',
    process.platform === 'win32' ? 'picomatch-mortis.exe' : 'picomatch-mortis'
  )
  Atomics.store(control, 0, -2)
  const worker = new Worker(__filename, { workerData: { shared, binary } })
  Atomics.wait(control, 0, -2)
  worker.unref()

  const hex = value => Buffer.from(String(value)).toString('hex')
  const call = args => {
    const request = encoder.encode(`${args.map(hex).join('\t')}\n`)
    if (request.length > CAPACITY) throw new RangeError('bridge request is too large')
    bytes.set(request)
    Atomics.store(control, 1, request.length)
    Atomics.store(control, 0, 1)
    Atomics.notify(control, 0)
    Atomics.wait(control, 0, 1)
    const response = decoder.decode(bytes.subarray(0, Atomics.load(control, 1)))
    Atomics.store(control, 0, 0)
    Atomics.notify(control, 0)
    const [status, payload] = response.split('\t', 2)
    const value = Buffer.from(payload || '', 'hex').toString()
    if (status === 'error') throw new TypeError(value)
    return value
  }

  module.exports = { call }
}
