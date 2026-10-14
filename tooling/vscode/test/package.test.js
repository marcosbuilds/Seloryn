'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const iconThemePath = path.join(root, manifest.contributes.iconThemes[0].path);
const iconTheme = JSON.parse(fs.readFileSync(iconThemePath, 'utf8'));
const pngPath = path.join(root, manifest.icon);
const png = fs.readFileSync(pngPath);

assert.equal(manifest.icon, './assets/seloryn-icon.png');
assert.equal(manifest.contributes.iconThemes.length, 1);
assert.equal(manifest.contributes.iconThemes[0].id, 'seloryn-file-icons');
assert.equal(iconTheme.fileExtensions.sely, '_seloryn');
assert.equal(iconTheme.languageIds.seloryn, '_seloryn');
assert.equal(iconTheme.iconDefinitions._seloryn.iconPath, '../assets/seloryn-file-icon.svg');
assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
assert.equal(png.readUInt32BE(16), 128);
assert.equal(png.readUInt32BE(20), 128);

const ignore = fs.readFileSync(path.join(root, '.vscodeignore'), 'utf8');
for (const entry of ['node_modules/**', 'test/**', 'examples/**', 'scripts/**', 'package-lock.json']) {
  assert.match(ignore, new RegExp(`^${entry.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&')}$`, 'mu'));
}

assert.equal(manifest.devDependencies['@vscode/vsce'], '3.7.1');
assert.equal(manifest.scripts.package, 'npm run check && vsce package --no-dependencies && node scripts/normalize-vsix.js');
console.log('vscode package metadata tests passed');
