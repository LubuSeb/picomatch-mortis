'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const sumsPath = path.join(root, 'tests', 'SHA256SUMS');
const original = path.join(root, 'tests', 'original');
const entries = fs.readFileSync(sumsPath, 'utf8').trim().split(/\r?\n/);

for (const entry of entries) {
  const match = entry.match(/^([0-9a-f]{64})  (.+)$/);
  if (!match) throw new Error(`Malformed SHA256SUMS entry: ${entry}`);

  const [, expected, relative] = match;
  const file = path.join(original, ...relative.split('/'));
  const canonical = fs.readFileSync(file, 'utf8').replace(/\r\n/g, '\n');
  const actual = crypto.createHash('sha256').update(canonical).digest('hex');

  if (actual !== expected) {
    throw new Error(`Frozen upstream test changed: ${relative}`);
  }
}

console.log(`Verified ${entries.length} frozen upstream files.`);
