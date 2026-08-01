'use strict'

const assert = require('node:assert')
const path = require('node:path')
const reference = require('picomatch-reference')

process.env.PICOMATCH_MORTIS_BIN = path.join(
  __dirname,
  '..',
  'target',
  'release',
  process.platform === 'win32' ? 'picomatch-mortis.exe' : 'picomatch-mortis'
)

const port = require('../tests')
const { close } = require('../tests/bridge')

// `capture` disables Picomatch's exact-input shortcut, ensuring every batch
// actually executes both regular-expression engines.
const LEGACY_OPTIONS = { flags: 'i', windows: false, capture: true }
const LEGACY_CLASS_OPTIONS = { ...LEGACY_OPTIONS, literalBrackets: false }
const BATCH_SIZE = 128

const canonicalizeLegacyCodeUnit = codeUnit => {
  const upper = String.fromCharCode(codeUnit).toUpperCase()
  if (upper.length !== 1) return codeUnit

  const mapped = upper.charCodeAt(0)
  return codeUnit >= 0x80 && mapped < 0x80 ? codeUnit : mapped
}

const escapeRegex = value => value.replace(/[\\^$.*+?()[\]{}|]/g, '\\$&')
const escapeClass = value => value.replace(/([\\\]\^-])/g, '\\$1')
const label = codeUnit => `U+${codeUnit.toString(16).padStart(4, '0').toUpperCase()}`

const equivalenceClasses = () => {
  const classes = new Map()
  let nonidentityMappings = 0

  for (let codeUnit = 0; codeUnit <= 0xffff; codeUnit += 1) {
    const canonical = canonicalizeLegacyCodeUnit(codeUnit)
    if (canonical !== codeUnit) nonidentityMappings += 1
    if (!classes.has(canonical)) classes.set(canonical, [])
    classes.get(canonical).push(codeUnit)
  }

  return {
    classes: [...classes.values()].filter(group => group.length > 1),
    nonidentityMappings,
  }
}

const orderedEquivalences = classes => {
  const equivalences = []
  for (const group of classes) {
    for (const patternCodeUnit of group) {
      for (const inputCodeUnit of group) {
        if (inputCodeUnit !== patternCodeUnit) {
          equivalences.push([patternCodeUnit, inputCodeUnit])
        }
      }
    }
  }
  return equivalences
}

const batchContext = batch => {
  const [firstPattern, firstInput] = batch[0]
  const [lastPattern, lastInput] = batch[batch.length - 1]
  return `${label(firstPattern)} -> ${label(firstInput)} through ` +
    `${label(lastPattern)} -> ${label(lastInput)}`
}

const main = async () => {
  const started = process.hrtime.bigint()
  let checkCount = 0
  let nativeBatchCount = 0
  let equivalenceCount = 0
  let nonidentityMappings = 0

  try {
    const derived = equivalenceClasses()
    nonidentityMappings = derived.nonidentityMappings
    const equivalences = orderedEquivalences(derived.classes)
    equivalenceCount = equivalences.length

    for (let start = 0; start < equivalences.length; start += BATCH_SIZE) {
      const batch = equivalences.slice(start, start + BATCH_SIZE)
      const input = batch.map(([, codeUnit]) => String.fromCharCode(codeUnit)).join('')
      const literalPattern = batch.map(([codeUnit]) => String.fromCharCode(codeUnit)).join('')
      const literalSource = batch
        .map(([codeUnit]) => escapeRegex(String.fromCharCode(codeUnit)))
        .join('')
      const classPattern = batch
        .map(([codeUnit]) => `[${escapeClass(String.fromCharCode(codeUnit))}]`)
        .join('')
      const context = batchContext(batch)

      const nodeLiteral = new RegExp(`^(?:${literalSource})$`, 'i').test(input)
      const nodeClass = new RegExp(`^(?:${classPattern})$`, 'i').test(input)
      const referenceLiteral = reference.isMatch(input, literalPattern, LEGACY_OPTIONS)
      const referenceClass = reference.isMatch(input, classPattern, LEGACY_CLASS_OPTIONS)
      const nativeLiteral = port.isMatch(input, literalPattern, LEGACY_OPTIONS)
      const nativeClass = port.isMatch(input, classPattern, LEGACY_CLASS_OPTIONS)

      assert.equal(nodeLiteral, true, `Node literal /i rejected batch ${context}`)
      assert.equal(nodeClass, true, `Node class /i rejected batch ${context}`)
      assert.equal(referenceLiteral, nodeLiteral, `Pinned Picomatch literal /i rejected batch ${context}`)
      assert.equal(referenceClass, nodeClass, `Pinned Picomatch class /i rejected batch ${context}`)
      assert.equal(nativeLiteral, referenceLiteral, `Native literal /i diverged for batch ${context}`)
      assert.equal(nativeClass, referenceClass, `Native class /i diverged for batch ${context}`)
      checkCount += batch.length * 2
      nativeBatchCount += 2
    }
  } finally {
    await close()
  }

  const elapsedMs = Number(process.hrtime.bigint() - started) / 1e6
  const format = value => value.toLocaleString('en-US')
  console.log(
    `Verified ${format(nonidentityMappings)} nonidentity legacy BMP mappings ` +
    `(${format(equivalenceCount)} ordered equivalences, ` +
    `${format(checkCount)} literal/class mapping checks in ${format(nativeBatchCount)} native batches) ` +
    `in ${elapsedMs.toFixed(1)}ms ` +
    `on Node ${process.version}.`
  )
}

main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
