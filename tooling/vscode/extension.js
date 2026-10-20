'use strict';

const childProcess = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const { StringDecoder } = require('node:string_decoder');
const { createSelorynLspClient } = require('./lsp-client');

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
  ['tensor_cross_entropy', 'tensor_cross_entropy(logits, targets, axis)', 'Compute fused mean softmax cross-entropy'],
  ['tensor_sum_to_shape', 'tensor_sum_to_shape(tensor, shape)', 'Reduce a broadcasted tensor'],
  ['train_linear', 'train_linear(train, test, seed, steps)', 'Narrow linear-training fixture'],
  ['gpu', 'gpu(train, test, seed, steps)', 'Canonical GPU linear-training profile alias'],
  ['train_mlp', 'train_mlp(train, test, checkpoint, seed, steps, batch_size, hidden_size, learning_rate)', 'Train a bounded CPU MLP with tamper-detecting resume checkpoint'],
  ['predict_mlp', 'predict_mlp(checkpoint, inputs)', 'Run bounded batch inference from a verified MLP checkpoint'],
  ['load_checkpoint', 'load_checkpoint(path)', 'Read a bounded checkpoint fixture'],
  ['print', 'print(text)', 'Write explicit program output']
]);

const MODULES = Object.freeze([
  ['io', 'Standard output import'],
  ['json', 'Bounded JSON project capability'],
  ['models', 'Narrow model contract fixture'],
  ['training', 'Bounded CPU linear and MLP training'],
  ['audit', 'Structured JSON2 audit events'],
  ['tensor', 'Bounded CPU tensor value export'],
  ['data', 'Bounded data iteration helpers'],
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
  ['as_i64_list', 'as_i64_list(value)', 'Convert a JSON number array to List[I64]'],
  ['as_f64_list', 'as_f64_list(value)', 'Convert a JSON number array to List[F64]'],
  ['as_text_list', 'as_text_list(value)', 'Convert a JSON string array to List[Text]'],
  ['from_text', 'from_text(value)', 'Create a JSON string'],
  ['from_i64', 'from_i64(value)', 'Create a JSON integer'],
  ['from_f64', 'from_f64(value)', 'Create a JSON number'],
  ['from_f64_list', 'from_f64_list(values)', 'Create a JSON number array'],
  ['sha256', 'sha256(value)', 'Hash canonical JSON text with SHA-256'],
  ['object', 'object(keys, values)', 'Create a JSON object from bounded key/value lists'],
  ['write_file', 'write_file(path, value)', 'Atomically persist JSON inside a granted project root'],
  ['values', 'values(tensor)', 'Export bounded F32 tensor values'],
  ['progress', 'progress(operation, step, total, metric)', 'Flush one source-owned training progress event'],
  ['emit', 'emit(kind, payload)', 'Append a structured source-owned audit event to the JSON2 receipt'],
  ['len', 'len(value)', 'Count a List or Text value'],
  ['to_f64', 'to_f64(value)', 'Convert a numeric value to F64'],
  ['ModelRequest', 'ModelRequest(model, prompt, max_output_tokens, seed)', 'Create the model request fixture'],
  ['ModelResponse', 'ModelResponse(status, text)', 'Create the model response fixture'],
  ['fixture_provider', 'fixture_provider(request)', 'Call the deterministic model fixture'],
  ['slow_fixture_provider', 'slow_fixture_provider(request, timeout)', 'Call the bounded slow model fixture'],
  ['backward', 'backward(function, point)', 'Use the scalar or single-tensor reverse-mode fixture'],
  ['gradcheck', 'gradcheck(function, point)', 'Run the scalar finite-difference fixture'],
  ['gradients', 'gradients(function, tensor_1, tensor_2, ...)', 'Return bounded gradients for multiple Tensor[F32] inputs'],
  ['range', 'range(stop)', 'Create a bounded I64 sequence for Seloryn loops']
]);

const SNIPPETS = Object.freeze([
  ['fn', 'Function declaration', 'fn ${1:name}(${2:argument}: ${3:Type}) -> ${4:Unit}:\n    ${0}'],
  ['record', 'Record declaration', 'record ${1:Name}:\n    ${2:field}: ${3:Type}\n    ${0}'],
  ['enum', 'Enum declaration', 'enum ${1:Name}:\n    ${2:Variant}\n    ${0}'],
  ['if', 'Conditional block', 'if ${1:condition}:\n    ${0}'],
  ['for', 'List iteration', 'for ${1:item} in ${2:items}:\n    ${0}'],
  ['match', 'Exhaustive match block', 'match ${1:value}:\n    case ${2:pattern}:\n        ${0}'],
  ['train_mlp', 'Real bounded CPU MLP training', 'from training import train_mlp\n\nfn main() -> Json:\n    return train_mlp("data/train.json", "data/test.json", "checkpoints/model.json", seed=17, steps=640, batch_size=3, hidden_size=8, learning_rate=0.02)${0}'],
  ['predict_mlp', 'Bounded MLP batch inference', 'from training import predict_mlp\n\nfn main() -> Json:\n    return predict_mlp("checkpoints/model.json", "data/inputs.json")${0}'],
  ['self_hosted_mlp', 'Source-owned custom MLP workflow', 'from autograd import gradients\nfrom data import range\nfrom json import as_f64_list, from_f64_list, object, sha256, write_file\nfrom tensor import values\nfrom training import progress\n\nfor ${1:step} in range(${2:160}):\n    let ${3:derivatives}: List[Tensor[F32]] = gradients(${4:objective}, ${5:inputs}, ${6:first}, ${7:first_bias}, ${8:second}, ${9:second_bias}, ${10:targets})\n    ${6:first} = ${6:first} - ${11:rate} * ${3:derivatives}[1]\n    ${7:first_bias} = ${7:first_bias} - ${11:rate} * ${3:derivatives}[2]\n    ${8:second} = ${8:second} - ${11:rate} * ${3:derivatives}[3]\n    ${9:second_bias} = ${9:second_bias} - ${11:rate} * ${3:derivatives}[4]\n    progress("custom-mlp", ${1:step}, ${2:160}, tensor_item_f32(${12:loss}))${0}'],
  ['self_hosted_language_model', 'Source-owned language-model workflow', 'from app.data import load\nfrom app.model import objective\nfrom app.tokenizer import tokenize\nfrom autograd import gradients\nfrom json import as_text_list, get, read_file, write_file\nfrom training import progress\n\nlet ${1:tokens}: List[Text] = as_text_list(get(read_file("${2:data/train.json}"), ["tokens"]))?\nlet ${3:encoded}: List[I64] = tokenize(${1:tokens})?\nfor ${4:step} in range(${5:160}):\n    let ${6:derivatives}: List[Tensor[F32]] = gradients(${7:objective}, ${8:inputs}, ${9:weights}, ${10:targets})\n    ${9:weights} = ${9:weights} - ${11:rate} * ${6:derivatives}[1]\n    progress("bigram-language-model", ${4:step}, ${5:160}, tensor_item_f32(${12:loss}))${0}'],
  ['gpu', 'Canonical GPU linear-training alias', 'from training import gpu\n\nfn main() -> Json:\n    return gpu("data/train.json", "data/test.json", seed=7, steps=300)${0}'],
  ['self_hosted_train', 'Seloryn-authored gradient loop', 'from autograd import gradients\nfrom data import range\n\nfor ${1:step} in range(${2:120}):\n    let ${3:derivatives}: List[Tensor[F32]] = gradients(${4:objective}, ${5:inputs}, ${6:weight}, ${7:bias}, ${8:targets})\n    ${9:weight} = ${9:weight} - ${10:rate} * ${3:derivatives}[1]\n    ${11:bias} = ${11:bias} - ${10:rate} * ${3:derivatives}[2]${0}'],
  ['tensor_objective', 'Tensor objective returning a scalar', 'fn ${1:objective}(input: Tensor[F32]) -> Tensor[F32]:\n    let weights: Tensor[F32] = tensor_f32([1.0], [1, 1])\n    let prediction: Tensor[F32] = matmul(input, weights)\n    return tensor_mean(prediction * prediction)${0}']
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
      for (const [name, detail, insertText] of SNIPPETS) {
        const kind = vscode.CompletionItemKind.Snippet || vscode.CompletionItemKind.Keyword;
        const item = completionItem(vscode, name, kind, detail);
        if (vscode.SnippetString) {
          item.insertText = new vscode.SnippetString(insertText);
          item.insertTextFormat = vscode.InsertTextFormat.Snippet;
        }
        add(item);
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

const DEFAULT_DIAGNOSTIC_TIMEOUT_MS = 2500;
const DEFAULT_DIAGNOSTIC_MAX_OUTPUT_BYTES = 256 * 1024;

function documentFilePath(document) {
  return document?.uri?.fsPath || document?.uri?.path || '';
}

function findProjectRoot(filePath, workspaceRoot, fsModule = fs, pathModule = path) {
  if (!filePath) {
    return '';
  }
  const resolvedFile = pathModule.resolve(filePath);
  const resolvedWorkspace = workspaceRoot ? pathModule.resolve(workspaceRoot) : '';
  let current = pathModule.dirname(resolvedFile);
  while (true) {
    if (fsModule.existsSync(pathModule.join(current, 'seloryn.toml'))) {
      return current;
    }
    if (resolvedWorkspace && current === resolvedWorkspace) {
      break;
    }
    const parent = pathModule.dirname(current);
    const relativeToWorkspace = resolvedWorkspace
      ? pathModule.relative(resolvedWorkspace, parent)
      : '';
    if (parent === current || (resolvedWorkspace
      && (relativeToWorkspace === '..'
        || relativeToWorkspace.startsWith(`..${pathModule.sep}`)
        || pathModule.isAbsolute(relativeToWorkspace)))) {
      break;
    }
    current = parent;
  }
  return '';
}

function createTrainingTerminal(vscode, fsModule = fs, pathModule = path) {
  const editor = vscode.window?.activeTextEditor;
  const documentPath = documentFilePath(editor?.document);
  const workspaceFolder = documentPath
    ? vscode.workspace?.getWorkspaceFolder?.(editor.document.uri)
    : undefined;
  const workspaceRoot = workspaceFolder?.uri?.fsPath
    || vscode.workspace?.workspaceFolders?.[0]?.uri?.fsPath
    || '';
  const projectRoot = findProjectRoot(documentPath || workspaceRoot, workspaceRoot, fsModule, pathModule);
  if (!projectRoot) {
    vscode.window?.showErrorMessage?.('Seloryn: open a file inside a project with seloryn.toml before training.');
    return { status: 'unavailable' };
  }
  const configuration = vscode.workspace?.getConfiguration?.('seloryn');
  const cliPath = String(configuration?.get?.('cliPath', 'seloryn') || 'seloryn').trim();
  const terminal = vscode.window?.createTerminal?.({
    name: 'Seloryn Training',
    cwd: projectRoot,
    shellPath: cliPath,
    shellArgs: ['run', projectRoot, '--log=human']
  });
  terminal?.show?.();
  return { status: 'started', projectRoot, cliPath };
}

function createHardwareTerminal(vscode, command) {
  const configuration = vscode.workspace?.getConfiguration?.('seloryn');
  const cliPath = String(configuration?.get?.('cliPath', 'seloryn') || 'seloryn').trim();
  const workspaceRoot = vscode.workspace?.workspaceFolders?.[0]?.uri?.fsPath;
  const terminal = vscode.window?.createTerminal?.({
    name: `Seloryn ${command === 'gpu-probe' ? 'GPU Probe' : 'Devices'}`,
    ...(workspaceRoot ? { cwd: workspaceRoot } : {}),
    shellPath: cliPath,
    shellArgs: [command, '--format=json']
  });
  terminal?.show?.();
  return { status: 'started', command, cliPath };
}

function documentLineCount(document) {
  if (Number.isInteger(document?.lineCount) && document.lineCount > 0) {
    return document.lineCount;
  }
  const source = typeof document?.getText === 'function' ? document.getText() : '';
  return Math.max(1, source.split(/\r?\n/u).length);
}

function positionFromDiagnostic(vscode, document, point) {
  if (!point || !Number.isInteger(point.line) || !Number.isInteger(point.column)) {
    throw new Error('diagnostic position is not an integer');
  }
  const lineCount = documentLineCount(document);
  if (point.line < 1 || point.line > lineCount || point.column < 1) {
    throw new Error('diagnostic position is outside the document');
  }
  const lineIndex = point.line - 1;
  const lineText = typeof document.lineAt === 'function'
    ? document.lineAt(lineIndex).text
    : '';
  const codePoints = Array.from(lineText);
  if (point.column > codePoints.length + 1) {
    throw new Error('diagnostic column is outside the document');
  }
  // The CLI reports Unicode scalar columns; VS Code positions use UTF-16
  // offsets. Convert through code points so a diagnostic after an emoji lands
  // on the same source character instead of splitting its surrogate pair.
  const utf16Column = codePoints.slice(0, point.column - 1).join('').length;
  return new vscode.Position(lineIndex, utf16Column);
}

function diagnosticSeverity(vscode, severity) {
  const values = vscode.DiagnosticSeverity || {};
  switch (severity) {
    case 'warning':
      return values.Warning ?? 1;
    case 'information':
    case 'info':
      return values.Information ?? 2;
    case 'hint':
      return values.Hint ?? 3;
    case 'error':
      return values.Error ?? 0;
    default:
      throw new Error(`unsupported diagnostic severity: ${severity}`);
  }
}

function parseDiagnosticJson(vscode, document, output) {
  if (typeof output !== 'string' || !output.trim()) {
    throw new Error('Seloryn CLI returned empty diagnostic output');
  }
  let payload;
  try {
    payload = JSON.parse(output);
  } catch (error) {
    throw new Error(`Seloryn CLI returned invalid JSON: ${error.message}`);
  }
  if (!payload || !Array.isArray(payload.diagnostics)) {
    throw new Error('Seloryn diagnostic JSON has no diagnostics array');
  }
  if (payload.schema !== 'seloryn.analysis.v1') {
    throw new Error('Seloryn diagnostic JSON has an unsupported schema');
  }

  return payload.diagnostics.map((entry, index) => {
    if (!entry || typeof entry !== 'object'
      || typeof entry.code !== 'string'
      || typeof entry.message !== 'string'
      || typeof entry.severity !== 'string'
      || !entry.primary?.start
      || !entry.primary?.end) {
      throw new Error(`malformed Seloryn diagnostic at index ${index}`);
    }
    const start = positionFromDiagnostic(vscode, document, entry.primary.start);
    const end = positionFromDiagnostic(vscode, document, entry.primary.end);
    const range = new vscode.Range(start, end);
    const help = typeof entry.help === 'string' && entry.help ? `\n\n${entry.help}` : '';
    const diagnostic = new vscode.Diagnostic(
      range,
      `${entry.message}${help}`,
      diagnosticSeverity(vscode, entry.severity)
    );
    diagnostic.code = entry.code;
    diagnostic.source = 'Seloryn';
    return diagnostic;
  });
}

function integerSetting(config, key, fallback, minimum, maximum) {
  const value = config?.get?.(key, fallback);
  return Number.isInteger(value) && value >= minimum && value <= maximum ? value : fallback;
}

function createSelorynDiagnosticRunner(
  vscode,
  spawn = childProcess.spawn,
  pathModule = path,
  fsModule = fs
) {
  const active = new Map();

  function cancelAll() {
    for (const record of active.values()) {
      record.cancel();
    }
    active.clear();
  }

  function cancel(document) {
    const key = document?.uri?.toString?.() || documentFilePath(document);
    active.get(key)?.cancel?.();
  }

  function check(document, token = {}) {
    const filePath = documentFilePath(document);
    if (!filePath || document?.uri?.scheme && document.uri.scheme !== 'file') {
      return Promise.resolve({ status: 'ignored', diagnostics: [] });
    }
    if (token.isCancellationRequested) {
      return Promise.resolve({ status: 'cancelled', diagnostics: [] });
    }

    const configuration = vscode.workspace?.getConfiguration?.('seloryn');
    const cliPath = String(configuration?.get?.('cliPath', 'seloryn') || 'seloryn').trim();
    const timeoutMs = integerSetting(
      configuration,
      'diagnostics.timeoutMs',
      DEFAULT_DIAGNOSTIC_TIMEOUT_MS,
      100,
      10000
    );
    const maxOutputBytes = integerSetting(
      configuration,
      'diagnostics.maxOutputBytes',
      DEFAULT_DIAGNOSTIC_MAX_OUTPUT_BYTES,
      4096,
      4 * 1024 * 1024
    );
    const workspaceFolder = vscode.workspace?.getWorkspaceFolder?.(document.uri);
    const workspaceRoot = workspaceFolder?.uri?.fsPath || '';
    const cwd = workspaceRoot || pathModule.dirname(filePath);
    const projectRoot = findProjectRoot(filePath, workspaceRoot, fsModule, pathModule);
    const checkTarget = projectRoot || filePath;
    const key = document.uri?.toString?.() || filePath;
    const previous = active.get(key);
    if (previous) {
      previous.cancel();
    }

    return new Promise((resolve) => {
      let settled = false;
      let child;
      let timer;
      let cancellationSubscription;
      let stdout = '';
      let stderr = '';
      let outputBytes = 0;
      const stdoutDecoder = new StringDecoder('utf8');
      const stderrDecoder = new StringDecoder('utf8');

      const finish = (result) => {
        if (settled) {
          return;
        }
        settled = true;
        clearTimeout(timer);
        cancellationSubscription?.dispose?.();
        if (active.get(key)?.finish === finish) {
          active.delete(key);
        }
        resolve(result);
      };

      const cancel = () => {
        try {
          child?.kill?.();
        } finally {
          finish({ status: 'cancelled', diagnostics: [] });
        }
      };

      active.set(key, { cancel, finish });
      if (typeof token.onCancellationRequested === 'function') {
        cancellationSubscription = token.onCancellationRequested(cancel);
      }

      const append = (current, chunk, decoder) => {
        const raw = Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk), 'utf8');
        outputBytes += raw.length;
        const value = decoder ? decoder.write(raw) : raw.toString('utf8');
        if (outputBytes > maxOutputBytes) {
          try {
            child?.kill?.();
          } finally {
            finish({ status: 'output-limit', diagnostics: [] });
          }
          return current;
        }
        return current + value;
      };

      if (settled) {
        return;
      }
      try {
        // Keep the executable and each argument separate. The CLI path is one
        // configured executable; shell syntax is never parsed or evaluated.
        child = spawn(cliPath, [
          'check',
          checkTarget,
          '--format=json',
          '--color=never'
        ], {
          cwd,
          shell: false,
          windowsHide: true,
          stdio: ['ignore', 'pipe', 'pipe']
        });
      } catch (error) {
        finish({ status: 'unavailable', diagnostics: [], error });
        return;
      }

      child.stdout?.on?.('data', (chunk) => {
        stdout = append(stdout, chunk, stdoutDecoder);
      });
      child.stderr?.on?.('data', (chunk) => {
        stderr = append(stderr, chunk, stderrDecoder);
      });
      child.on?.('error', (error) => {
        finish({ status: 'unavailable', diagnostics: [], error });
      });
      child.on?.('close', (code, signal) => {
        if (settled) {
          return;
        }
        stdout += stdoutDecoder.end();
        stderr += stderrDecoder.end();
        if (code === 0) {
          finish({ status: 'ok', diagnostics: [], stdout });
          return;
        }
        const payload = stderr.trim() || stdout.trim();
        try {
          const diagnostics = parseDiagnosticJson(vscode, document, payload);
          finish({ status: 'diagnostics', diagnostics, exitCode: code, signal });
        } catch (error) {
          finish({
            status: 'failed',
            diagnostics: [],
            exitCode: code,
            signal,
            error
          });
        }
      });
      timer = setTimeout(() => {
        try {
          child.kill?.();
        } finally {
          finish({ status: 'timeout', diagnostics: [] });
        }
      }, timeoutMs);
    });
  }

  return { cancel, cancelAll, check };
}

function createSelorynDiagnosticController(vscode, context, dependencies = {}) {
  if (!vscode.languages?.createDiagnosticCollection) {
    return { dispose() {} };
  }
  const collection = vscode.languages.createDiagnosticCollection('seloryn');
  const runner = createSelorynDiagnosticRunner(
    vscode,
    dependencies.spawn,
    dependencies.path,
    dependencies.fs
  );
  const subscriptions = [];

  const checkDocument = async (document) => {
    if (document?.languageId && document.languageId !== 'seloryn') {
      return;
    }
    const configuration = vscode.workspace?.getConfiguration?.('seloryn');
    if (configuration?.get?.('diagnostics.enabled', true) === false) {
      collection.delete(document.uri);
      return;
    }
    const result = await runner.check(document);
    if (result.status === 'diagnostics') {
      collection.set(document.uri, result.diagnostics);
    } else if (result.status === 'ok' || result.status === 'failed'
      || result.status === 'unavailable' || result.status === 'timeout'
      || result.status === 'output-limit') {
      // Do not leave stale diagnostics after a saved file has been checked or
      // after the configured CLI becomes unavailable.
      collection.delete(document.uri);
    }
  };

  if (typeof vscode.workspace?.onDidSaveTextDocument === 'function') {
    subscriptions.push(vscode.workspace.onDidSaveTextDocument((document) => {
      void checkDocument(document);
    }));
  }
  if (typeof vscode.workspace?.onDidCloseTextDocument === 'function') {
    subscriptions.push(vscode.workspace.onDidCloseTextDocument((document) => {
      runner.cancel(document);
      collection.delete(document.uri);
    }));
  }

  return {
    checkDocument,
    dispose() {
      runner.cancelAll();
      for (const subscription of subscriptions) {
        subscription?.dispose?.();
      }
      collection.dispose?.();
    }
  };
}

function lspRange(vscode, value) {
  return new vscode.Range(
    new vscode.Position(value.start.line, value.start.character),
    new vscode.Position(value.end.line, value.end.character)
  );
}

function createSelorynLspController(vscode, context, dependencies = {}) {
  const configuration = vscode.workspace?.getConfiguration?.('seloryn');
  const workspaceFolder = vscode.workspace?.workspaceFolders?.[0];
  const workspaceRoot = workspaceFolder?.uri?.fsPath || '';
  if (!workspaceRoot || !vscode.languages?.createDiagnosticCollection) {
    return { status: 'unavailable', dispose() {} };
  }
  const collection = vscode.languages.createDiagnosticCollection('seloryn-lsp');
  const client = createSelorynLspClient({
    command: String(configuration?.get?.('cliPath', 'seloryn') || 'seloryn'),
    cwd: workspaceRoot,
    spawn: dependencies.spawn,
    onNotification: (message) => {
      if (message.method !== 'textDocument/publishDiagnostics') return;
      const params = message.params || {};
      const uri = vscode.Uri?.parse ? vscode.Uri.parse(params.uri) : params.uri;
      const diagnostics = (params.diagnostics || []).map((entry) => {
        const diagnostic = new vscode.Diagnostic(
          lspRange(vscode, entry.range),
          entry.message,
          entry.severity === 2
            ? vscode.DiagnosticSeverity.Warning
            : vscode.DiagnosticSeverity.Error
        );
        diagnostic.code = entry.code;
        diagnostic.source = entry.source || 'Seloryn';
        return diagnostic;
      });
      collection.set(uri, diagnostics);
    }
  });
  const subscriptions = [];
  const sendWhenReady = (action) => { void client.start().then(() => action()).catch(() => {}); };
  subscriptions.push(
    vscode.workspace.onDidOpenTextDocument?.((document) => {
      if (document.languageId === 'seloryn') sendWhenReady(() => client.didOpen(document));
    }),
    vscode.workspace.onDidChangeTextDocument?.((event) => {
      if (event.document.languageId === 'seloryn') sendWhenReady(() => client.didChange(event.document));
    }),
    vscode.workspace.onDidSaveTextDocument?.((document) => {
      if (document.languageId === 'seloryn') sendWhenReady(() => client.didSave(document));
    }),
    vscode.workspace.onDidCloseTextDocument?.((document) => {
      if (document.languageId === 'seloryn') client.didClose(document);
      collection.delete(document.uri);
    })
  );
  const selector = { language: 'seloryn', scheme: 'file' };
  subscriptions.push(
    vscode.languages.registerCompletionItemProvider(selector, {
      provideCompletionItems(document, position) {
        return client.start()
          .then(() => client.request('textDocument/completion', {
            textDocument: { uri: document.uri.toString() },
            position
          }))
          .then((result) => (result?.items || []).map((item) => {
            const completion = new vscode.CompletionItem(item.label, item.kind);
            completion.detail = item.detail;
            return completion;
          }))
          .catch(() => []);
      }
    }),
    vscode.languages.registerDefinitionProvider(selector, {
      provideDefinition(document, position) {
        return client.start()
          .then(() => client.request('textDocument/definition', {
            textDocument: { uri: document.uri.toString() },
            position
          }))
          .then((result) => (result || []).map((location) => ({
            uri: vscode.Uri?.parse ? vscode.Uri.parse(location.uri) : location.uri,
            range: lspRange(vscode, location.range)
          })))
          .catch(() => []);
      }
    })
  );
  client.start().catch(() => {});
  const active = vscode.workspace.textDocuments || [];
  for (const document of active) {
    if (document.languageId === 'seloryn') sendWhenReady(() => client.didOpen(document));
  }
  return {
    status: 'started',
    dispose() {
      for (const subscription of subscriptions) subscription?.dispose?.();
      collection.dispose?.();
      return client.dispose();
    }
  };
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
  const configuration = vscode.workspace?.getConfiguration?.('seloryn');
  const lspEnabled = configuration?.get?.('lsp.enabled', true) !== false;
  context.subscriptions.push(
    lspEnabled
      ? createSelorynLspController(vscode, context)
      : createSelorynDiagnosticController(vscode, context)
  );
  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(selector, createCompletionProvider(vscode), '.', ':', '_'),
    vscode.languages.registerInlineCompletionItemProvider(selector, createInlineCompletionProvider(vscode)),
    vscode.commands.registerCommand('seloryn.toggleAiGhostText', async () => {
      const configuration = vscode.workspace.getConfiguration('seloryn');
      const current = configuration.get('aiGhostText.enabled', false);
      await configuration.update('aiGhostText.enabled', !current, vscode.ConfigurationTarget.Global);
      const state = !current ? 'enabled' : 'disabled';
      vscode.window.showInformationMessage(`Seloryn AI ghost text ${state}.`);
    }),
    vscode.commands.registerCommand('seloryn.runTraining', () => {
      createTrainingTerminal(vscode);
    }),
    vscode.commands.registerCommand('seloryn.inspectDevices', () => {
      createHardwareTerminal(vscode, 'devices');
    }),
    vscode.commands.registerCommand('seloryn.runGpuProbe', () => {
      createHardwareTerminal(vscode, 'gpu-probe');
    })
  );
}

function deactivate() {}

module.exports = {
  BUILTINS,
  KEYWORDS,
  MODULES,
  RESERVED,
  SNIPPETS,
  STANDARD_FUNCTIONS,
  TYPES,
  activate,
  buildPrompt,
  createCompletionProvider,
  createHardwareTerminal,
  createTrainingTerminal,
  createInlineCompletionProvider,
  createSelorynDiagnosticController,
  createSelorynDiagnosticRunner,
  createSelorynLspController,
  deactivate,
  extractLocalSymbols,
  findProjectRoot,
  parseDiagnosticJson,
  sanitizeGhostText,
  wordPrefix
};
