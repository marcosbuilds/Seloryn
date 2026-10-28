'use strict';

const fs = require('node:fs');
const path = require('node:path');
const yauzl = require('yauzl');
const yazl = require('yazl');

const packageJson = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'package.json'), 'utf8'));
const root = path.join(__dirname, '..');
const inputPath = path.join(root, `${packageJson.name}-${packageJson.version}.vsix`);
const temporaryPath = `${inputPath}.tmp`;
// Construct in local time so yazl's local-time DOS conversion stays valid on
// every supported timezone (a UTC midnight can be the previous calendar year).
const fixedMtime = new Date(2000, 0, 1, 0, 0, 0);

function readArchive() {
  return new Promise((resolve, reject) => {
    yauzl.open(inputPath, { lazyEntries: true }, (error, zipFile) => {
      if (error) {
        reject(error);
        return;
      }
      const entries = [];
      zipFile.readEntry();
      zipFile.on('entry', (entry) => {
        zipFile.openReadStream(entry, (streamError, stream) => {
          if (streamError) {
            zipFile.close();
            reject(streamError);
            return;
          }
          const chunks = [];
          stream.on('data', (chunk) => chunks.push(chunk));
          stream.on('error', reject);
          stream.on('end', () => {
            entries.push({ name: entry.fileName, data: Buffer.concat(chunks) });
            zipFile.readEntry();
          });
        });
      });
      zipFile.on('end', () => resolve(entries.sort((left, right) => left.name.localeCompare(right.name))));
      zipFile.on('error', reject);
    });
  });
}

function writeArchive(entries) {
  return new Promise((resolve, reject) => {
    const zipFile = new yazl.ZipFile();
    const output = fs.createWriteStream(temporaryPath);
    output.on('error', reject);
    output.on('finish', resolve);
    zipFile.outputStream.pipe(output);
    for (const entry of entries) {
      zipFile.addBuffer(entry.data, entry.name, {
        compress: true,
        mtime: fixedMtime,
        mode: 0o100644
      });
    }
    zipFile.end();
  });
}

readArchive()
  .then(writeArchive)
  .then(() => {
    fs.rmSync(inputPath, { force: true });
    fs.renameSync(temporaryPath, inputPath);
    console.log(`normalized ${path.basename(inputPath)} for reproducible ZIP metadata`);
  })
  .catch((error) => {
    try {
      fs.rmSync(temporaryPath, { force: true });
    } catch (_cleanupError) {
      // Preserve the original packaging error.
    }
    console.error(error);
    process.exitCode = 1;
  });
