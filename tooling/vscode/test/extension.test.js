'use strict';

const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const path = require('node:path');
const fs = require('node:fs');
const extension = require('../extension');
const lspClient = require('../lsp-client');

const lspFrames = Buffer.concat([
  lspClient.frame({ jsonrpc: '2.0', id: 1, result: { ok: true } }),
  lspClient.frame({ jsonrpc: '2.0', method: 'textDocument/publishDiagnostics', params: {} })
]);
const parsedLspFrames = lspClient.parseFrames(lspFrames);
assert.equal(parsedLspFrames.messages.length, 2);
assert.equal(parsedLspFrames.messages[0].id, 1);
assert.equal(parsedLspFrames.messages[1].method, 'textDocument/publishDiagnostics');
assert.equal(parsedLspFrames.remainder.length, 0);

for (const metadata of ['../package.json', '../language-configuration.json', '../syntaxes/seloryn.tmLanguage.json']) {
  const metadataPath = path.join(__dirname, metadata);
  assert.doesNotThrow(() => JSON.parse(fs.readFileSync(metadataPath, 'utf8')));
}

class CompletionItem {
  constructor(label, kind) {
    this.label = label;
    this.kind = kind;
  }
}

class SnippetString {
  constructor(value) {
    this.value = value;
  }
}

class Range {
  constructor(start, end) {
    this.start = start;
    this.end = end;
  }
}

class InlineCompletionItem {
  constructor(insertText, range) {
    this.insertText = insertText;
    this.range = range;
  }
}

class Position {
  constructor(line, character) {
    this.line = line;
    this.character = character;
  }
}

class Diagnostic {
  constructor(range, message, severity) {
    this.range = range;
    this.message = message;
    this.severity = severity;
  }
}

const vscode = {
  CompletionItem,
  CompletionItemKind: {
    Function: 1,
    Variable: 2,
    Constant: 3,
    Struct: 4,
    Enum: 5,
    Class: 6,
    Keyword: 7,
    Module: 8,
    Snippet: 9
  },
  CompletionItemTag: { Deprecated: 1 },
  InsertTextFormat: { Snippet: 2 },
  InlineCompletionItem,
  Diagnostic,
  DiagnosticSeverity: { Error: 0, Warning: 1, Information: 2, Hint: 3 },
  Position,
  Range,
  SnippetString,
  LanguageModelChatMessage: {
    User: (content) => ({ role: 'user', content })
  },
  workspace: {
    getConfiguration: () => ({
      get: (key, fallback) => key === 'aiGhostText.enabled' ? true : fallback
    })
  },
  lm: {
    selectChatModels: async () => [{
      sendRequest: async function* () {
        yield { text: '    return tensor_sum(values)' };
      }
    }]
  }
};

function documentFor(source) {
  const lines = source.split('\n');
  return {
    uri: {
      toString: () => 'file:///workspace/fixture.sely',
      fsPath: path.join('workspace', 'fixture.sely'),
      scheme: 'file'
    },
    languageId: 'seloryn',
    lineCount: lines.length,
    getText: () => source,
    lineAt: (line) => ({ text: lines[line] || '' }),
    offsetAt: (position) => lines.slice(0, position.line).reduce((total, value) => total + value.length + 1, 0) + position.character
  };
}

function positionAt(source, marker) {
  const offset = source.indexOf(marker);
  assert.notEqual(offset, -1, `marker ${marker} must exist`);
  const before = source.slice(0, offset);
  const lines = before.split('\n');
  return { line: lines.length - 1, character: lines.at(-1).length };
}

const fixturePath = path.join(__dirname, '..', 'examples', 'extension-evidence.sely');
const fixture = fs.readFileSync(fixturePath, 'utf8');

assert.match(fixture, /fn main/u);
assert.deepEqual(
  extension.extractLocalSymbols('fn classify(order: Order) -> Text:\n    let label: Text = "ok"'),
  [
    { name: 'classify', kind: 'function', detail: 'Local function declaration' },
    { name: 'label', kind: 'variable', detail: 'Local let binding' },
    { name: 'order', kind: 'variable', detail: 'Function parameter' }
  ]
);

const completionSource = [
  'fn classify(order: Order) -> Text:',
  '    let label: Text = "ok"',
  '    return lab',
  ''
].join('\n');
const completionDocument = documentFor(completionSource);
const labOffset = completionSource.indexOf('lab');
const labBefore = completionSource.slice(0, labOffset + 'lab'.length);
const labLines = labBefore.split('\n');
const completionPosition = { line: labLines.length - 1, character: labLines.at(-1).length };
const completionItems = extension
  .createCompletionProvider(vscode)
  .provideCompletionItems(completionDocument, completionPosition);
const labels = completionItems.map((item) => item.label);
assert.ok(labels.includes('label'), 'local binding must be suggested');
assert.ok(!labels.includes('tensor_f32'), 'prefix filtering must be applied');

const snippetSource = 'fn';
const snippetDocument = documentFor(snippetSource);
const snippetItems = extension
  .createCompletionProvider(vscode)
  .provideCompletionItems(snippetDocument, { line: 0, character: 2 });
const functionSnippet = snippetItems.find((item) => item.label === 'fn');
assert.ok(functionSnippet, 'implemented function snippet must be suggested');
assert.match(functionSnippet.insertText.value, /fn \$\{1:name\}/u);

const broadPosition = { line: 2, character: 0 };
const broadItems = extension.createCompletionProvider(vscode)
  .provideCompletionItems(completionDocument, broadPosition);
assert.ok(broadItems.some((item) => item.label === 'tensor_f32'));
assert.ok(broadItems.some((item) => item.label === 'read_stdin'));
assert.ok(broadItems.some((item) => item.label === 'json'));
assert.ok(broadItems.some((item) => item.label === 'match'));
assert.ok(broadItems.some((item) => item.label === 'Tensor'));
assert.ok(broadItems.some((item) => item.label === 'train_mlp'));
assert.ok(broadItems.some((item) => item.label === 'predict_mlp'));
assert.ok(broadItems.some((item) => item.label === 'gradients'));
assert.ok(broadItems.some((item) => item.label === 'range'));
assert.ok(broadItems.some((item) => item.label === 'as_f64_list'));
assert.ok(broadItems.some((item) => item.label === 'data'));

const trainingProject = path.resolve(__dirname, '..', '..', '..', 'examples', 'projects', 'mlp-training');
const trainingTerminalCalls = [];
const trainingVscode = {
  workspace: {
    workspaceFolders: [{ uri: { fsPath: trainingProject } }],
    getWorkspaceFolder: () => ({ uri: { fsPath: trainingProject } }),
    getConfiguration: () => ({ get: (key, fallback) => key === 'cliPath' ? 'seloryn-test' : fallback })
  },
  window: {
    activeTextEditor: {
      document: { uri: { fsPath: path.join(trainingProject, 'src', 'mlp-training-entry.sely') } }
    },
    createTerminal: (options) => {
      trainingTerminalCalls.push(options);
      return { show: () => {} };
    }
  }
};
assert.deepEqual(extension.createTrainingTerminal(trainingVscode), {
  status: 'started',
  projectRoot: trainingProject,
  cliPath: 'seloryn-test'
});
assert.deepEqual(trainingTerminalCalls[0].shellArgs, ['run', trainingProject, '--log=human']);
assert.equal(trainingTerminalCalls[0].shellPath, 'seloryn-test');

const hardwareTerminalCalls = [];
const hardwareVscode = {
  workspace: {
    workspaceFolders: [{ uri: { fsPath: trainingProject } }],
    getConfiguration: () => ({ get: (key, fallback) => key === 'cliPath' ? 'seloryn-test' : fallback })
  },
  window: {
    createTerminal: (options) => {
      hardwareTerminalCalls.push(options);
      return { show: () => {} };
    }
  }
};
assert.deepEqual(extension.createHardwareTerminal(hardwareVscode, 'devices'), {
  status: 'started',
  command: 'devices',
  cliPath: 'seloryn-test'
});
assert.deepEqual(hardwareTerminalCalls[0].shellArgs, ['devices', '--format=json']);
assert.equal(hardwareTerminalCalls[0].shellPath, 'seloryn-test');
assert.deepEqual(extension.createHardwareTerminal(hardwareVscode, 'gpu-probe'), {
  status: 'started',
  command: 'gpu-probe',
  cliPath: 'seloryn-test'
});
assert.deepEqual(hardwareTerminalCalls[1].shellArgs, ['gpu-probe', '--format=json']);

const prompt = extension.buildPrompt(completionDocument, completionPosition);
assert.match(prompt, /Seloryn/u);
assert.match(prompt, /<SELORYN_CURSOR>/u);
assert.ok(prompt.length < 13000, 'prompt context must be bounded');

assert.equal(extension.sanitizeGhostText('```seloryn\nreturn 1\n```', 100), 'return 1');
assert.equal(extension.sanitizeGhostText('return 1 <SELORYN_CURSOR>', 100), '');
assert.equal(extension.sanitizeGhostText('\u0000return 1', 100), 'return 1');
assert.equal(extension.sanitizeGhostText('abcdefgh', 4), 'abcd');

const diagnosticDocument = documentFor('🙂\nfn main() -> Unit:\n    return\n');
const diagnosticPayload = JSON.stringify({
  schema: 'seloryn.analysis.v1',
  language_version: '0.09',
  runtime_package: '0.9.4',
  diagnostics: [{
    code: 'SEL-P001',
    severity: 'error',
    category: 'parser',
    message: 'expected expression',
    file: diagnosticDocument.uri.fsPath,
    primary: {
      start: { line: 1, column: 2 },
      end: { line: 1, column: 2 }
    },
    help: 'check the surrounding syntax',
    stack: []
  }]
});
const parsedDiagnostics = extension.parseDiagnosticJson(vscode, diagnosticDocument, diagnosticPayload);
assert.equal(parsedDiagnostics.length, 1);
assert.equal(parsedDiagnostics[0].code, 'SEL-P001');
assert.equal(parsedDiagnostics[0].source, 'Seloryn');
assert.equal(parsedDiagnostics[0].range.start.line, 0);
assert.equal(parsedDiagnostics[0].range.start.character, 2, 'Unicode scalar column must become UTF-16 position');
assert.match(parsedDiagnostics[0].message, /check the surrounding syntax/u);

function mockSpawn({ stderr = '', stdout = '', code = 65, emitClose = true } = {}) {
  let invocation;
  const spawn = (command, args, options) => {
    invocation = { command, args, options };
    const child = new EventEmitter();
    child.stdout = new EventEmitter();
    child.stderr = new EventEmitter();
    child.killed = false;
    child.kill = () => {
      child.killed = true;
    };
    process.nextTick(() => {
      if (stdout) child.stdout.emit('data', stdout);
      if (stderr) child.stderr.emit('data', stderr);
      if (emitClose) child.emit('close', code, null);
    });
    spawn.child = child;
    return child;
  };
  spawn.invocation = () => invocation;
  return spawn;
}

const diagnosticConfiguration = {
  cliPath: 'seloryn-test',
  'diagnostics.timeoutMs': 500,
  'diagnostics.maxOutputBytes': 4096
};
const diagnosticVscode = {
  ...vscode,
  workspace: {
    getConfiguration: () => ({
      get: (key, fallback) => Object.prototype.hasOwnProperty.call(diagnosticConfiguration, key)
        ? diagnosticConfiguration[key]
        : fallback
    }),
    getWorkspaceFolder: () => ({ uri: { fsPath: path.resolve('workspace') } })
  }
};

(async () => {
  const spawn = mockSpawn({ stderr: `${diagnosticPayload}\n` });
  const runner = extension.createSelorynDiagnosticRunner(diagnosticVscode, spawn, path);
  const result = await runner.check(diagnosticDocument, { isCancellationRequested: false });
  assert.equal(result.status, 'diagnostics');
  assert.equal(result.diagnostics[0].code, 'SEL-P001');
  assert.equal(spawn.invocation().command, 'seloryn-test');
  assert.deepEqual(spawn.invocation().args, [
    'check',
    diagnosticDocument.uri.fsPath,
    '--format=json',
    '--color=never'
  ]);
  assert.equal(spawn.invocation().options.cwd, path.resolve('workspace'));
  assert.equal(spawn.invocation().options.shell, false);

  const unicodePayload = JSON.stringify({
    schema: 'seloryn.analysis.v1',
  language_version: '0.09',
  runtime_package: '0.9.4',
    diagnostics: [{
      code: 'SEL-P001',
      severity: 'error',
      message: 'erro 😀',
      primary: {
        start: { line: 1, column: 1 },
        end: { line: 1, column: 1 }
      }
    }]
  });
  let splitChild;
  const splitSpawn = () => {
    splitChild = new EventEmitter();
    splitChild.stdout = new EventEmitter();
    splitChild.stderr = new EventEmitter();
    splitChild.kill = () => {};
    process.nextTick(() => {
      const bytes = Buffer.from(unicodePayload, 'utf8');
      const emojiOffset = bytes.indexOf(Buffer.from('😀', 'utf8')) + 1;
      splitChild.stderr.emit('data', bytes.subarray(0, emojiOffset));
      splitChild.stderr.emit('data', bytes.subarray(emojiOffset));
      splitChild.emit('close', 65, null);
    });
    return splitChild;
  };
  const splitResult = await extension
    .createSelorynDiagnosticRunner(diagnosticVscode, splitSpawn, path)
    .check(diagnosticDocument, { isCancellationRequested: false });
  assert.equal(splitResult.status, 'diagnostics');
  assert.match(splitResult.diagnostics[0].message, /😀/u);

  const projectRoot = path.resolve('workspace', 'project');
  const projectDocument = documentFor('fn main() -> Unit:\n    return\n');
  projectDocument.uri = {
    toString: () => 'file:///workspace/project/src/main.sely',
    fsPath: path.join(projectRoot, 'src', 'main.sely'),
    scheme: 'file'
  };
  const projectSpawn = mockSpawn({ stdout: 'checked project\n', code: 0 });
  const projectRunner = extension.createSelorynDiagnosticRunner(
    diagnosticVscode,
    projectSpawn,
    path,
    { existsSync: (candidate) => candidate === path.join(projectRoot, 'seloryn.toml') }
  );
  const projectResult = await projectRunner.check(projectDocument, { isCancellationRequested: false });
  assert.equal(projectResult.status, 'ok');
  assert.equal(projectSpawn.invocation().args[1], projectRoot, 'project files must check the manifest root');

  const successSpawn = mockSpawn({ stdout: 'checked fixture (syntax + semantics)\n', code: 0 });
  const success = await extension
    .createSelorynDiagnosticRunner(diagnosticVscode, successSpawn, path)
    .check(diagnosticDocument, { isCancellationRequested: false });
  assert.equal(success.status, 'ok');

  let cancelRequest;
  const hangingSpawn = mockSpawn({ emitClose: false });
  const cancellation = extension
    .createSelorynDiagnosticRunner(diagnosticVscode, hangingSpawn, path)
    .check(diagnosticDocument, {
      isCancellationRequested: false,
      onCancellationRequested: (callback) => {
        cancelRequest = callback;
        return { dispose() {} };
      }
    });
  cancelRequest();
  const cancelled = await cancellation;
  assert.equal(cancelled.status, 'cancelled');
  assert.equal(hangingSpawn.child.killed, true);

  let collectionItems;
  const saveListeners = [];
  const controllerVscode = {
    ...diagnosticVscode,
    languages: {
      createDiagnosticCollection: () => ({
        set: (_uri, items) => { collectionItems = items; },
        delete: () => {},
        dispose: () => {}
      })
    },
    workspace: {
      ...diagnosticVscode.workspace,
      onDidSaveTextDocument: (listener) => {
        saveListeners.push(listener);
        return { dispose() {} };
      },
      onDidCloseTextDocument: () => ({ dispose() {} })
    }
  };
  const controllerSpawn = mockSpawn({ stderr: diagnosticPayload });
  const controller = extension.createSelorynDiagnosticController(
    controllerVscode,
    { subscriptions: [] },
    { spawn: controllerSpawn, path }
  );
  await controller.checkDocument(diagnosticDocument);
  assert.equal(collectionItems[0].code, 'SEL-P001');
  assert.equal(saveListeners.length, 1);
  controller.dispose();
  console.log('vscode CLI diagnostic integration tests passed');
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

(async () => {
  const provider = extension.createInlineCompletionProvider(vscode);
  const result = await provider.provideInlineCompletionItems(
    completionDocument,
    completionPosition,
    {},
    { isCancellationRequested: false }
  );
  assert.equal(result.items.length, 1);
  assert.match(result.items[0].insertText, /tensor_sum/u);

  const unavailable = {
    ...vscode,
    lm: { selectChatModels: async () => [] }
  };
  const safeProvider = extension.createInlineCompletionProvider(unavailable);
  const safeResult = await safeProvider.provideInlineCompletionItems(
    completionDocument,
    completionPosition,
    {},
    { isCancellationRequested: false }
  );
  assert.deepEqual(safeResult, { items: [] });
  console.log('vscode extension tests passed');
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
