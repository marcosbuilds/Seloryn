'use strict';

const KEYWORDS = Object.freeze([
  'let', 'var', 'const', 'fn', 'return', 'record', 'enum', 'match', 'case',
  'if', 'elif', 'else', 'for', 'while', 'loop', 'break', 'continue', 'in',
  'is', 'not', 'and', 'or', 'require', 'import', 'from', 'as', 'pub', 'module',
  'true', 'false', 'null'
]);

const RESERVED = Object.freeze([
  'trait', 'impl', 'class', 'async', 'await', 'using', 'defer', 'try', 'catch',
  'finally', 'yield'
]);

const TYPES = Object.freeze([
  ['Bool', 'Boolean value'],
  ['I64', 'Signed 64-bit integer'],
  ['F32', '32-bit floating-point value'],
  ['F64', '64-bit floating-point value'],
  ['Text', 'UTF-8 text'],
  ['Unit', 'Unit value'],
  ['Tensor', 'Bounded CPU tensor'],
  ['List', 'Typed list'],
  ['Result', 'Fallible result'],
  ['Option', 'Optional value']
]);

const BUILTINS = Object.freeze([
  ['tensor_f32', 'tensor_f32(values, shape)', 'Create an F32 CPU tensor'],
  ['tensor_i64', 'tensor_i64(values, shape)', 'Create an I64 CPU tensor'],
  ['tensor_zeros_f32', 'tensor_zeros_f32(shape)', 'Create a zero-filled F32 tensor'],
  ['tensor_zeros_i64', 'tensor_zeros_i64(shape)', 'Create a zero-filled I64 tensor'],
  ['tensor_shape', 'tensor_shape(tensor)', 'Read tensor dimensions'],
  ['tensor_rank', 'tensor_rank(tensor)', 'Read tensor rank'],
  ['tensor_elements', 'tensor_elements(tensor)', 'Read tensor element count'],
  ['tensor_dtype', 'tensor_dtype(tensor)', 'Read tensor dtype'],
  ['tensor_to_f32', 'tensor_to_f32(tensor)', 'Perform explicit lossless conversion to F32'],
  ['tensor_to_i64', 'tensor_to_i64(tensor)', 'Perform explicit lossless conversion to I64'],
  ['tensor_item_f32', 'tensor_item_f32(tensor)', 'Extract a rank-zero F32 value'],
  ['tensor_get_f32', 'tensor_get_f32(tensor, indices)', 'Read an F32 tensor element'],
  ['tensor_get_i64', 'tensor_get_i64(tensor, indices)', 'Read an I64 tensor element'],
  ['reshape', 'reshape(tensor, shape)', 'Reshape a tensor with the same element count'],
  ['tensor_sum', 'tensor_sum(tensor[, axis])', 'Reduce tensor values'],
  ['tensor_mean', 'tensor_mean(tensor[, axis])', 'Compute a tensor mean'],
  ['matmul', 'matmul(left, right)', 'Rank-two F32 matrix multiplication'],
  ['transpose', 'transpose(tensor)', 'Create a rank-two transpose view'],
  ['tensor_relu', 'tensor_relu(tensor)', 'Apply F32 ReLU'],
  ['tensor_exp', 'tensor_exp(tensor)', 'Apply elementwise F32 exp'],
  ['tensor_log', 'tensor_log(tensor)', 'Apply positive-domain F32 log'],
  ['tensor_softmax', 'tensor_softmax(tensor, axis)', 'Compute stable F32 softmax'],
  ['tensor_logsumexp', 'tensor_logsumexp(tensor, axis)', 'Compute stable log-sum-exp'],
  ['tensor_sum_to_shape', 'tensor_sum_to_shape(tensor, shape)', 'Reduce a broadcasted tensor'],
  ['train_linear', 'train_linear(train, test, seed, steps)', 'Narrow linear-training fixture'],
  ['load_checkpoint', 'load_checkpoint(path)', 'Read a bounded checkpoint fixture'],
  ['print', 'print(text)', 'Write explicit program output']
]);

const MODULES = Object.freeze([
  ['io', 'Standard output import'],
  ['json', 'Bounded JSON project capability'],
  ['models', 'Narrow model contract fixture'],
  ['training', 'Narrow linear-training fixture']
]);

const STANDARD_FUNCTIONS = Object.freeze([
  ['read_stdin', 'read_stdin()', 'Read one bounded JSON document from stdin'],
  ['read_file', 'read_file(path)', 'Read one bounded JSON document from a granted project root'],
  ['parse', 'parse(text)', 'Parse bounded JSON text'],
  ['encode', 'encode(value)', 'Encode a Json value'],
  ['get', 'get(value, path)', 'Read a Json object path'],
  ['as_text', 'as_text(value)', 'Convert a Json string'],
  ['as_i64', 'as_i64(value)', 'Convert a Json integer'],
  ['as_f32', 'as_f32(value)', 'Convert a Json number to F32'],
  ['as_f64', 'as_f64(value)', 'Convert a Json number to F64'],
  ['as_bool', 'as_bool(value)', 'Convert a Json boolean'],
  ['ModelRequest', 'ModelRequest(model, prompt, max_output_tokens, seed)', 'Create the model request fixture'],
  ['ModelResponse', 'ModelResponse(status, text)', 'Create the model response fixture'],
  ['fixture_provider', 'fixture_provider(request)', 'Call the deterministic model fixture'],
  ['slow_fixture_provider', 'slow_fixture_provider(request, timeout)', 'Call the bounded slow model fixture'],
  ['backward', 'backward(function, point)', 'Use the scalar reverse-mode fixture'],
  ['gradcheck', 'gradcheck(function, point)', 'Run the scalar finite-difference fixture']
]);

const IDENTIFIER = '[\\p{L}_][\\p{L}\\p{N}_]*';

function extractLocalSymbols(text) {
  const symbols = [];
  const seen = new Set();
  const add = (name, kind, detail) => {
    if (!name || KEYWORDS.includes(name) || RESERVED.includes(name) || seen.has(name)) {
      return;
    }
    seen.add(name);
    symbols.push({ name, kind, detail });
  };

  const declarations = new RegExp(`\\b(fn|record|enum)\\s+(${IDENTIFIER})`, 'gu');
  for (const match of text.matchAll(declarations)) {
    const kind = match[1] === 'fn' ? 'function' : match[1] === 'record' ? 'struct' : 'enum';
    add(match[2], kind, `Local ${kind} declaration`);
  }

  const bindings = new RegExp(`\\b(let|var|const)\\s+(${IDENTIFIER})`, 'gu');
  for (const match of text.matchAll(bindings)) {
    add(match[2], match[1] === 'const' ? 'constant' : 'variable', `Local ${match[1]} binding`);
  }

  const functions = new RegExp(`\\bfn\\s+${IDENTIFIER}\\s*\\(([^)]*)\\)`, 'gu');
  for (const match of text.matchAll(functions)) {
    const parameterPattern = new RegExp(`(${IDENTIFIER})(?:\\s*:)`, 'gu');
    for (const parameter of match[1].matchAll(parameterPattern)) {
      add(parameter[1], 'variable', 'Function parameter');
    }
  }

  return symbols;
}

function documentPrefix(document, position) {
  const line = document.lineAt(position.line).text;
  return line.slice(0, position.character);
}

function wordPrefix(document, position) {
  const prefix = documentPrefix(document, position);
  const match = prefix.match(new RegExp(`(${IDENTIFIER})$`, 'u'));
  return match ? match[1] : '';
}

function completionItem(vscode, label, kind, detail, insertText) {
  const item = new vscode.CompletionItem(label, kind);
  item.detail = detail;
  item.insertText = insertText || label;
  if (kind === vscode.CompletionItemKind.Function && vscode.SnippetString) {
    item.insertText = new vscode.SnippetString(insertText || `${label}($0)`);
    item.insertTextFormat = vscode.InsertTextFormat.Snippet;
  }
  return item;
}

function createCompletionProvider(vscode) {
  return {
    provideCompletionItems(document, position) {
      const prefix = wordPrefix(document, position).toLowerCase();
      const candidates = [];
      const seen = new Set();
      const add = (item) => {
        if (!seen.has(item.label) && item.label.toLowerCase().startsWith(prefix)) {
          seen.add(item.label);
          candidates.push(item);
        }
      };

      for (const symbol of extractLocalSymbols(document.getText())) {
        const kind = symbol.kind === 'function'
          ? vscode.CompletionItemKind.Function
          : symbol.kind === 'constant'
            ? vscode.CompletionItemKind.Constant
            : symbol.kind === 'struct'
              ? vscode.CompletionItemKind.Struct
              : symbol.kind === 'enum'
                ? vscode.CompletionItemKind.Enum
                : vscode.CompletionItemKind.Variable;
        add(completionItem(vscode, symbol.name, kind, symbol.detail,
          symbol.kind === 'function' ? `${symbol.name}($0)` : symbol.name));
      }
      for (const [name, detail] of TYPES) {
        add(completionItem(vscode, name, vscode.CompletionItemKind.Class, detail));
      }
      for (const [name, signature, detail] of BUILTINS) {
        const item = completionItem(vscode, name, vscode.CompletionItemKind.Function, detail, `${name}($0)`);
        item.documentation = signature;
        add(item);
      }
      for (const [name, signature, detail] of STANDARD_FUNCTIONS) {
        const item = completionItem(vscode, name, vscode.CompletionItemKind.Function, detail, `${name}($0)`);
        item.documentation = signature;
        add(item);
      }
      for (const [name, detail] of MODULES) {
        add(completionItem(vscode, name, vscode.CompletionItemKind.Module, detail));
      }
      for (const keyword of KEYWORDS) {
        add(completionItem(vscode, keyword, vscode.CompletionItemKind.Keyword, 'Seloryn keyword'));
      }
      for (const keyword of RESERVED) {
        const item = completionItem(vscode, keyword, vscode.CompletionItemKind.Keyword, 'Reserved; not implemented');
        item.tags = vscode.CompletionItemTag ? [vscode.CompletionItemTag.Deprecated] : undefined;
        add(item);
      }
      return candidates;
    }
  };
}

function buildPrompt(document, position) {
  const source = document.getText();
  const offset = typeof document.offsetAt === 'function'
    ? document.offsetAt(position)
    : source.split(/\r?\n/u).slice(0, position.line).join('\n').length + position.character;
  const bounded = source.slice(Math.max(0, offset - 12000), offset);
  return [
    'You are completing a Seloryn .sely source file.',
    'Return only a short source continuation at the cursor; do not use Markdown or explain it.',
    'Use only the implemented Seloryn subset: typed bindings, functions, records, enums, Result/Option, match, pipelines, JSON fixtures and bounded CPU tensor builtins.',
    'Do not invent imports, effects, HTTP, GPU, async, packages or nullable syntax.',
    'Source immediately before the cursor:',
    bounded,
    '<SELORYN_CURSOR>'
  ].join('\\n');
}

function sanitizeGhostText(value, maxCharacters) {
  if (typeof value !== 'string') {
    return '';
  }
  let text = value.replace(/^```(?:seloryn)?\s*/iu, '').replace(/```\s*$/u, '');
  text = text.replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/gu, '');
  if (text.includes('<SELORYN_CURSOR>') || text.includes('```')) {
    return '';
  }
  return text.slice(0, maxCharacters).replace(/\s+$/u, '');
}

function createInlineCompletionProvider(vscode) {
  const cache = new Map();
  const inFlight = new Set();
  return {
    async provideInlineCompletionItems(document, position, _context, token) {
      const config = vscode.workspace?.getConfiguration?.('seloryn');
      if (!config?.get?.('aiGhostText.enabled', false) || !vscode.lm?.selectChatModels) {
        return { items: [] };
      }
      const key = `${document.uri?.toString?.() || 'document'}:${position.line}:${position.character}:${documentPrefix(document, position)}`;
      const now = Date.now();
      const cached = cache.get(key);
      if (cached && now - cached.time < 1500) {
        return cached.result;
      }
      if (inFlight.has(key)) {
        return { items: [] };
      }
      inFlight.add(key);
      try {
        const models = await vscode.lm.selectChatModels({});
        if (token?.isCancellationRequested || !models?.length) {
          return { items: [] };
        }
        const message = vscode.LanguageModelChatMessage?.User
          ? vscode.LanguageModelChatMessage.User(buildPrompt(document, position))
          : { role: 'user', content: buildPrompt(document, position) };
        const response = await models[0].sendRequest(
          [message],
          { temperature: 0.2, maxTokens: 120 },
          token
        );
        let generated = '';
        for await (const part of response) {
          if (token?.isCancellationRequested) {
            return { items: [] };
          }
          if (typeof part?.text === 'string') {
            generated += part.text;
          } else if (typeof part?.value?.text === 'string') {
            generated += part.value.text;
          }
        }
        const maxCharacters = config.get('aiGhostText.maxCharacters', 240);
        const text = sanitizeGhostText(generated, maxCharacters);
        if (!text) {
          return { items: [] };
        }
        const item = new vscode.InlineCompletionItem(text, new vscode.Range(position, position));
        const result = { items: [item] };
        cache.set(key, { time: Date.now(), result });
        return result;
      } catch (_error) {
        // Providers can disappear or reject a request. Empty suggestions are the safe fallback.
        return { items: [] };
      } finally {
        inFlight.delete(key);
      }
    }
  };
}

function activate(context) {
  // Keep the host API import inside activation so the behavior can be tested without VS Code.
  // eslint-disable-next-line global-require
  const vscode = require('vscode');
  const selector = { language: 'seloryn', scheme: 'file' };
  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(selector, createCompletionProvider(vscode), '.', ':', '_'),
    vscode.languages.registerInlineCompletionItemProvider(selector, createInlineCompletionProvider(vscode)),
    vscode.commands.registerCommand('seloryn.toggleAiGhostText', async () => {
      const configuration = vscode.workspace.getConfiguration('seloryn');
      const current = configuration.get('aiGhostText.enabled', false);
      await configuration.update('aiGhostText.enabled', !current, vscode.ConfigurationTarget.Global);
      const state = !current ? 'enabled' : 'disabled';
      vscode.window.showInformationMessage(`Seloryn AI ghost text ${state}.`);
    })
  );
}

function deactivate() {}

module.exports = {
  BUILTINS,
  KEYWORDS,
  MODULES,
  RESERVED,
  STANDARD_FUNCTIONS,
  TYPES,
  activate,
  buildPrompt,
  createCompletionProvider,
  createInlineCompletionProvider,
  deactivate,
  extractLocalSymbols,
  sanitizeGhostText,
  wordPrefix
};
