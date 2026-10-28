'use strict';

const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { createSelorynLspClient, frame, parseFrames } = require('../lsp-client');
const extension = require('../extension');

const sent = [];
const spawn = (command, args, options) => {
  assert.equal(command, 'seloryn-test');
  assert.deepEqual(args, ['lsp', '--stdio']);
  assert.equal(options.shell, false);
  const child = new EventEmitter();
  child.stdout = new EventEmitter();
  child.stdin = {
    write(payload) {
      const messages = parseFrames(payload).messages;
      sent.push(...messages);
      for (const message of messages) {
        if (message.method === 'initialize') {
          process.nextTick(() => child.stdout.emit('data', frame({
            jsonrpc: '2.0',
            id: message.id,
            result: { capabilities: { completionProvider: {} } }
          })));
        }
        if (message.method === 'shutdown') {
          process.nextTick(() => child.stdout.emit('data', frame({
            jsonrpc: '2.0',
            id: message.id,
            result: null
          })));
        }
      }
    }
  };
  child.kill = () => {};
  return child;
};

(async () => {
  const client = createSelorynLspClient({ command: 'seloryn-test', cwd: 'workspace', spawn });
  const initialized = await client.start();
  assert.deepEqual(initialized, { capabilities: { completionProvider: {} } });
  assert.equal(sent[0].method, 'initialize');
  assert.equal(sent[1].method, 'initialized');
  client.didOpen({
    uri: { toString: () => 'file:///workspace/main.sely' },
    getText: () => 'fn main() -> Unit:\n    return\n'
  });
  assert.equal(sent[2].method, 'textDocument/didOpen');
  assert.equal(sent[2].params.textDocument.text.includes('fn main'), true);
  await client.dispose();
  assert.equal(sent.at(-2).method, 'shutdown');
  assert.equal(sent.at(-1).method, 'exit');

  const fakeVscode = {
    workspace: {
      workspaceFolders: [{ uri: { fsPath: 'D:\\workspace' } }],
      textDocuments: [],
      getConfiguration: () => ({ get: (key, fallback) => key === 'cliPath' ? 'seloryn-test' : fallback }),
      onDidOpenTextDocument: () => ({ dispose() {} }),
      onDidChangeTextDocument: () => ({ dispose() {} }),
      onDidSaveTextDocument: () => ({ dispose() {} }),
      onDidCloseTextDocument: () => ({ dispose() {} })
    },
    languages: {
      createDiagnosticCollection: () => ({ set() {}, delete() {}, dispose() {} }),
      registerCompletionItemProvider: () => ({ dispose() {} }),
      registerDefinitionProvider: () => ({ dispose() {} })
    }
  };
  const controller = extension.createSelorynLspController(fakeVscode, {}, { spawn });
  assert.equal(controller.status, 'started');
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(sent.filter((message) => message.method === 'initialize').length, 2);
  await controller.dispose();
  console.log('vscode LSP transport tests passed');
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
