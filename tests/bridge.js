'use strict'

const path = require('node:path')
const { Worker, isMainThread, parentPort, workerData } = require('node:worker_threads')

const CAPACITY = 256 * 1024
const TIMEOUT_MS = Number.parseInt(process.env.PICOMATCH_MORTIS_TIMEOUT_MS || '10000', 10)

if (!isMainThread) {
  const { spawn } = require('node:child_process')
  const { once } = require('node:events')
  const readline = require('node:readline')
  const control = new Int32Array(workerData.shared, 0, 2)
  const bytes = new Uint8Array(workerData.shared, 8)
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()
  const child = spawn(workerData.binary, workerData.binaryArgs, {
    stdio: ['pipe', 'pipe', 'inherit'],
    windowsHide: true,
  })
  const lines = readline.createInterface({ input: child.stdout })
  parentPort.on('close', () => child.kill())
  parentPort.on('message', message => {
    if (message === 'stop') {
      lines.close()
      child.kill()
      parentPort.close()
    }
  })

  const loop = async () => {
    Atomics.store(control, 0, 0)
    Atomics.notify(control, 0)
    while (true) {
      Atomics.wait(control, 0, 0)
      if (Atomics.load(control, 0) === -1) {
        lines.close()
        child.kill()
        parentPort.close()
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
  const binaryArgs = process.env.PICOMATCH_MORTIS_ARGS
    ? JSON.parse(process.env.PICOMATCH_MORTIS_ARGS)
    : ['serve']
  Atomics.store(control, 0, -2)
  const worker = new Worker(__filename, { workerData: { shared, binary, binaryArgs } })
  const workerExit = new Promise(resolve => worker.once('exit', resolve))
  if (Atomics.wait(control, 0, -2, TIMEOUT_MS) === 'timed-out') {
    void worker.terminate()
    throw new Error(`Rust proof bridge did not start within ${TIMEOUT_MS}ms`)
  }
  worker.unref()
  let closePromise
  let failed = false

  const hex = value => Buffer.from(String(value)).toString('hex')
  const call = args => {
    if (failed) throw new Error('Rust proof bridge is unavailable after a prior timeout')
    const request = encoder.encode(`${args.map(hex).join('\t')}\n`)
    if (request.length > CAPACITY) throw new RangeError('bridge request is too large')
    bytes.set(request)
    Atomics.store(control, 1, request.length)
    Atomics.store(control, 0, 1)
    Atomics.notify(control, 0)
    if (Atomics.wait(control, 0, 1, TIMEOUT_MS) === 'timed-out') {
      failed = true
      worker.ref()
      worker.postMessage('stop')
      void worker.terminate()
      throw new Error(`Rust proof bridge request exceeded ${TIMEOUT_MS}ms`)
    }
    const response = decoder.decode(bytes.subarray(0, Atomics.load(control, 1)))
    Atomics.store(control, 0, 0)
    Atomics.notify(control, 0)
    const [status, payload] = response.split('\t', 2)
    const value = Buffer.from(payload || '', 'hex').toString()
    if (status === 'error') throw new TypeError(value)
    return value
  }

  const close = () => {
    if (!closePromise) {
      worker.ref()
      Atomics.store(control, 0, -1)
      Atomics.notify(control, 0)
      worker.postMessage('stop')
      closePromise = (async () => {
        let timeout
        const boundedWait = new Promise(resolve => {
          timeout = setTimeout(() => resolve(false), 1_000)
          timeout.unref()
        })
        const exited = await Promise.race([workerExit.then(() => true), boundedWait])
        clearTimeout(timeout)
        if (!exited) await worker.terminate()
      })()
    }
    return closePromise
  }

  module.exports = { call, close }
}
