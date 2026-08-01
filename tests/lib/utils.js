'use strict'

const { call } = require('../bridge')

exports.basename = (input, options = {}) => call([
  'basename',
  ...(options.windows ? ['--windows'] : []),
  '--payload',
  input,
])
