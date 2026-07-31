'use strict'

const { close } = require('./bridge')

exports.mochaHooks = {
  async afterAll() {
    await close()
  },
}
