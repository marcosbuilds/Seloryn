'use strict';

const childProcess = require('node:child_process');
const { pathToFileURL } = require('node:url');
const extensionVersion = require('./package.json').version;

function frame(value) {
  const payload = Buffer.from(JSON.stringify(value), 'utf8');
  return Buffer.concat([
    Buffer.from(`Content-Length: ${payload.length}\r\n\r\n`, 'ascii'),
    payload
  ]);
}

function parseFrames(buffer) {
  let rest = Buffer.isBuffer(buffer) ? buffer : Buffer.from(buffer || '');
  const messages = [];
  while (true) {
    const separator = rest.indexOf(Buffer.from('\r\n\r\n'));
    if (separator < 0) break;
    const headers = rest.subarray(0, separator).toString('ascii').split(/\r\n/u);
    const lengthHeader = headers.find((header) => /^content-length:/iu.test(header));
    const length = lengthHeader ? Number(lengthHeader.split(':', 2)[1].trim()) : NaN;
    if (!Number.isSafeInteger(length) || length < 0) {
      throw new Error('Seloryn LSP returned an invalid Content-Length');
    }
    const start = separator + 4;
    if (rest.length < start + length) break;
    const payload = rest.subarray(start, start + length).toString('utf8');
    messages.push(JSON.parse(payload));
    rest = rest.subarray(start + length);
  }
  return { messages, remainder: rest };
}

class SelorynLspClient {
  constructor({ command, cwd, spawn = childProcess.spawn, onNotification = () => {} }) {
    this.command = command;
    this.cwd = cwd;
    this.spawn = spawn;
    this.onNotification = onNotification;
    this.child = null;
    this.buffer = Buffer.alloc(0);
    this.nextId = 1;
    this.pending = new Map();
    this.ready = null;
  }

  start() {
    if (this.ready) return this.ready;
    this.child = this.spawn(this.command, ['lsp', '--stdio'], {
      cwd: this.cwd,
      shell: false,
      windowsHide: true
    });
    this.child.stdout?.on('data', (chunk) => this.receive(chunk));
    this.ready = new Promise((resolve, reject) => {
      this._resolveReady = resolve;
      this._rejectReady = reject;
    });
    this.child.on?.('error', (error) => this.fail(error));
    this.child.on?.('close', (code, signal) => {
      if (code !== 0 && code !== null) {
        this.fail(new Error(`Seloryn LSP exited with code ${code}${signal ? ` (${signal})` : ''}`));
      } else {
        this.fail(new Error('Seloryn LSP exited before completing a request'));
      }
    });
    this.request('initialize', {
      processId: null,
      clientInfo: { name: 'seloryn-vscode', version: extensionVersion },
      rootUri: this.cwd ? pathToFileURL(this.cwd).href : null,
      capabilities: {}
    }).then((result) => {
      this.notify('initialized', {});
      this._resolveReady?.(result);
    }).catch((error) => {
      this._rejectReady?.(error);
      this.fail(error);
    });
    return this.ready;
  }

  receive(chunk) {
    this.buffer = Buffer.concat([this.buffer, Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)]);
    let parsed;
    try {
      parsed = parseFrames(this.buffer);
    } catch (error) {
      this.fail(error);
      return;
    }
    this.buffer = parsed.remainder;
    for (const message of parsed.messages) {
      if (Object.prototype.hasOwnProperty.call(message, 'id') && this.pending.has(message.id)) {
        const pending = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(message.error.message || 'Seloryn LSP request failed'));
        else pending.resolve(message.result);
      } else if (message.method) {
        this.onNotification(message);
      }
    }
  }

  request(method, params) {
    if (!this.child?.stdin) return Promise.reject(new Error('Seloryn LSP is unavailable'));
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(frame({ jsonrpc: '2.0', id, method, params }));
    });
  }

  notify(method, params) {
    if (this.child?.stdin) {
      this.child.stdin.write(frame({ jsonrpc: '2.0', method, params }));
    }
  }

  didOpen(document) {
    this.notify('textDocument/didOpen', {
      textDocument: { uri: document.uri.toString(), languageId: 'seloryn', version: 1, text: document.getText() }
    });
  }

  didChange(document) {
    this.notify('textDocument/didChange', {
      textDocument: { uri: document.uri.toString(), version: document.version || 1 },
      contentChanges: [{ text: document.getText() }]
    });
  }

  didSave(document) {
    this.notify('textDocument/didSave', { textDocument: { uri: document.uri.toString() } });
  }

  didClose(document) {
    this.notify('textDocument/didClose', { textDocument: { uri: document.uri.toString() } });
  }

  fail(error) {
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
  }

  async dispose() {
    try {
      if (this.child?.stdin) {
        await this.request('shutdown', null).catch(() => {});
        this.notify('exit', null);
      }
    } finally {
      this.child?.kill?.();
      this.fail(new Error('Seloryn LSP disposed'));
    }
  }
}

function createSelorynLspClient(options) {
  return new SelorynLspClient(options);
}

module.exports = { createSelorynLspClient, frame, parseFrames };
