'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const extension = require('../extension');

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
    Module: 8
  },
  CompletionItemTag: { Deprecated: 1 },
  InsertTextFormat: { Snippet: 2 },
  InlineCompletionItem,
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
    uri: { toString: () => 'file:///fixture.sely' },
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

const broadPosition = { line: 2, character: 0 };
const broadItems = extension.createCompletionProvider(vscode)
  .provideCompletionItems(completionDocument, broadPosition);
assert.ok(broadItems.some((item) => item.label === 'tensor_f32'));
assert.ok(broadItems.some((item) => item.label === 'read_stdin'));
assert.ok(broadItems.some((item) => item.label === 'json'));
assert.ok(broadItems.some((item) => item.label === 'match'));
assert.ok(broadItems.some((item) => item.label === 'Tensor'));

const prompt = extension.buildPrompt(completionDocument, completionPosition);
assert.match(prompt, /Seloryn/u);
assert.match(prompt, /<SELORYN_CURSOR>/u);
assert.ok(prompt.length < 13000, 'prompt context must be bounded');

assert.equal(extension.sanitizeGhostText('```seloryn\nreturn 1\n```', 100), 'return 1');
assert.equal(extension.sanitizeGhostText('return 1 <SELORYN_CURSOR>', 100), '');
assert.equal(extension.sanitizeGhostText('\u0000return 1', 100), 'return 1');
assert.equal(extension.sanitizeGhostText('abcdefgh', 4), 'abcd');

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
