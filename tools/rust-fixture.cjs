const { execFileSync } = require('node:child_process');
const path = require('node:path');

module.exports = (example, ...args) => execFileSync(
  process.env.CARGO || 'cargo',
  ['run','--quiet','--locked','--no-default-features','--example',example,'--',...args],
  { cwd: path.resolve(__dirname, '..'), encoding: 'utf8' }
);
