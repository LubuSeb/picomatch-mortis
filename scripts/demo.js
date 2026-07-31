'use strict'

const pm = require('../tests')

const examples = [
  ['globstar', 'src/parser/glob.rs', 'src/**/*.rs', {}],
  ['brace range', 'release-3.txt', 'release-{1..5}.txt', {}],
  ['negative extglob', 'index.js', '!(*.test).js', {}],
  ['Windows path', 'src\\parser\\glob.rs', 'src/**/*.rs', { windows: true }],
]

console.log('Picomatch Mortis — native Rust demo')
for (const [name, input, pattern, options] of examples) {
  console.log(`${name.padEnd(17)} ${String(pm.isMatch(input, pattern, options)).padEnd(5)}  ${input}  <=  ${pattern}`)
}

console.log('\nscanner')
console.log(JSON.stringify(pm.scan('src/**/+(glob|scan).rs', { parts: true, tokens: true }), null, 2))

console.log('\nhardening rewrites')
for (const pattern of ['+(a|aa)', '*(*(f)*(o))']) {
  console.log(`${pattern.padEnd(14)} ${pm.makeRe(pattern).source}`)
}
