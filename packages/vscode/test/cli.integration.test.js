'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const repositoryRoot = path.resolve(__dirname, '..', '..', '..');
const binaryName = process.platform === 'win32' ? 'seloryn.exe' : 'seloryn';
const binary = path.join(repositoryRoot, 'target', 'release', binaryName);
assert.ok(fs.existsSync(binary), `release CLI is required at ${binary}`);

const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'seloryn-vscode-cli-'));
try {
  const sourcePath = path.join(temporaryRoot, 'fixture with space 😀.sely');
  const validSource = 'fn main() -> Unit:\r\n    return\r\n';
  fs.writeFileSync(sourcePath, validSource, 'utf8');

  const valid = spawnSync(binary, [
    'check',
    sourcePath,
    '--format=json',
    '--color=never'
  ], {
    cwd: repositoryRoot,
    encoding: 'utf8',
    windowsHide: true,
    shell: false
  });
  assert.equal(valid.error, undefined);
  assert.equal(valid.status, 0);
  const validPayload = JSON.parse(valid.stdout);
  assert.equal(validPayload.schema, 'seloryn.check.v1');
  assert.equal(validPayload.checked, true);
  assert.equal(validPayload.status, 'ok');
  assert.equal(validPayload.path, sourcePath);
  assert.equal(valid.stderr, '');

  const invalidSource = 'fn main() -> Unit:\r\n    if 1:\r\n        return\r\n';
  fs.writeFileSync(sourcePath, invalidSource, 'utf8');
  const invalid = spawnSync(binary, [
    'check',
    sourcePath,
    '--format=json',
    '--color=never'
  ], {
    cwd: repositoryRoot,
    encoding: 'utf8',
    windowsHide: true,
    shell: false
  });
  assert.equal(invalid.error, undefined);
  assert.equal(invalid.status, 65);
  assert.equal(invalid.stdout, '');
  const payload = JSON.parse(invalid.stderr);
  assert.equal(payload.diagnostics.length, 1);
  assert.equal(payload.diagnostics[0].code, 'SEL-T004');
  assert.equal(payload.diagnostics[0].primary.start.line, 2);
  assert.equal(payload.diagnostics[0].file, sourcePath);

  const projectRoot = path.join(temporaryRoot, 'invalid project');
  fs.mkdirSync(path.join(projectRoot, 'src'), { recursive: true });
  fs.writeFileSync(path.join(projectRoot, 'seloryn.toml'), 'this is not valid TOML\n', 'utf8');
  fs.writeFileSync(path.join(projectRoot, 'src', 'main.sely'), 'fn main() -> Unit:\n    return\n', 'utf8');
  const invalidManifest = spawnSync(binary, [
    'check',
    projectRoot,
    '--format=json',
    '--color=never'
  ], {
    cwd: repositoryRoot,
    encoding: 'utf8',
    windowsHide: true,
    shell: false
  });
  assert.equal(invalidManifest.error, undefined);
  assert.equal(invalidManifest.status, 65);
  assert.equal(invalidManifest.stdout, '');
  const manifestPayload = JSON.parse(invalidManifest.stderr);
  assert.equal(manifestPayload.diagnostics.length, 1);
  assert.equal(manifestPayload.diagnostics[0].code, 'SEL-M001');
  assert.match(manifestPayload.diagnostics[0].message, /invalid project manifest/u);
  console.log('Seloryn VS Code CLI integration: 3 cases passed');
} finally {
  fs.rmSync(temporaryRoot, { recursive: true, force: true });
}
