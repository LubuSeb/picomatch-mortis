'use strict'

const { call } = require('../bridge')

exports.basename = (input, options = {}) => call([
  ...(options.windows ? ['--windows'] : []),
  'basename',
  input,
])
