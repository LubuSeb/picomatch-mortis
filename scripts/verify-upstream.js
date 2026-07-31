'use strict'

const { execFileSync } = require('node:child_process')
const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')

const REPOSITORY = 'https://github.com/micromatch/picomatch.git'
const COMMIT = '4f41a8edade7a5ab19832f7b40ecce46b288767f'
const root = path.join(__dirname, '..')
const frozenRoot = path.join(root, 'tests', 'original')
const manifest = fs.readFileSync(path.join(root, 'tests', 'SHA256SUMS'), 'utf8')
const expected = manifest.trim().split(/\r?\n/).map(line => line.slice(66))
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'picomatch-upstream-'))

const filesRelativeTo = directory => {
  const visit = current => fs.readdirSync(current, { withFileTypes: true }).flatMap(entry => {
    const absolute = path.join(current, entry.name)
    return entry.isDirectory() ? visit(absolute) : [path.relative(directory, absolute)]
  })
  return visit(directory).map(value => value.replaceAll('\\', '/')).sort()
}

const canonical = filename => fs.readFileSync(filename).toString('utf8').replaceAll('\r\n', '\n')

try {
  execFileSync('git', ['init', '--quiet', temporary])
  execFileSync('git', ['-C', temporary, 'remote', 'add', 'origin', REPOSITORY])
  execFileSync('git', ['-C', temporary, 'fetch', '--quiet', '--depth=1', 'origin', COMMIT])
  execFileSync('git', ['-C', temporary, 'checkout', '--quiet', '--detach', 'FETCH_HEAD'])

  const upstreamRoot = path.join(temporary, 'test')
  const upstreamFiles = filesRelativeTo(upstreamRoot)
  const expectedFiles = [...expected].sort()
  if (JSON.stringify(upstreamFiles) !== JSON.stringify(expectedFiles)) {
    throw new Error('Frozen file list differs from the complete upstream test tree')
  }

  for (const relative of expectedFiles) {
    const frozen = canonical(path.join(frozenRoot, relative))
    const upstream = canonical(path.join(upstreamRoot, relative))
    if (frozen !== upstream) throw new Error(`${relative} differs from upstream ${COMMIT}`)
  }

  console.log(`Verified all ${expectedFiles.length} frozen files against upstream ${COMMIT}.`)
} finally {
  fs.rmSync(temporary, { recursive: true, force: true })
}
