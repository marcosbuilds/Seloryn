# Errors and diagnostics

Failures have two distinct layers: **compiler diagnostics** for invalid source, and **typed runtime results** for valid programs encountering failures. The CLI promotes a top-level `Err(E)` returned by `main` to `SEL-R127`, preserving a nonzero process status for automation. No generic `except: pass`, no false-success return values.

### Compile-time diagnostics

```text
error[SEL-T102]: expected Bool, got Text
 --> src/service.sely:12:13
  |
12|     require username else ValidationError("missing")
  |             ^^^^^^^^ condition must have type Bool
  |
  = help: use a Bool predicate such as username.length > 0
```

A diagnostic has stable `code`, severity, category, source file, a primary
source span (byte offsets and Unicode-aware display columns), help,
machine-readable JSON form and reproducible ordering. The current checker can
emit several independent semantic diagnostics from one source. Runtime JSON
diagnostics also carry the deterministic function stack. Secondary labels,
related diagnostics, parser recovery across all malformed regions and module
identity fields remain future work; they are not claimed by the current CLI.

| Family | Codes | Example |
|---|---|---|
| Lexical | `SEL-L***` | mixed indentation, malformed UTF-8 |
| Parsing | `SEL-P***` | missing colon, invalid pattern |
| Typing | `SEL-T***` | incompatible units, non-Bool condition |
| Module | `SEL-M***` | missing module, cyclic initialization |
| Tensor runtime | `SEL-R201`–`SEL-R212` | invalid shape, budget, dtype, broadcast, domain, empty reduction, numeric failure or lossy conversion |
| Result/control flow | `SEL-R***` | invalid `require`, non-exhaustive `match`, bad `?` flow |
| Runtime | `SEL-R***` | overflow, division by zero, invalid call, collection limit or impossible match |
| Security | `SEL-S***` | denied network destination |
| Tooling | `SEL-C***` | unsupported CLI command or target |
| Job contract | `SEL-J001`–`SEL-J012` | invalid spec, job storage, event history or containment boundary |
| Job reservation | `SEL-J013`–`SEL-J020` | invalid reservation state, owner/bearer, expiry, identity or TTL |

Job reservation errors are intentionally explicit about the current boundary:

| Code | Meaning |
|---|---|
| `SEL-J013` | job state cannot be claimed |
| `SEL-J014` | another active bearer owns the reservation, or the claim retry is invalid |
| `SEL-J015` | job is terminal `lost`; create a new job |
| `SEL-J016` | local reservation lease expired, or is still active for recovery |
| `SEL-J017` | supplied bearer/owner is not the active one |
| `SEL-J018` | public transition attempted to write an executor-owned state |
| `SEL-J019` | persisted lease or attempt identity is inconsistent |
| `SEL-J020` | TTL is outside the bounded local range |
| `SEL-J021` | job preflight spec, source, path, hash, parser or semantic validation failed |

These errors do not imply that Seloryn has started a worker, proven process
liveness or fenced a stale process.

### Runtime failure semantics

```seloryn
fn load_config(path: Path) -> Result[Config, ConfigError]:
    let source = fs.read_text(path)?
    let config = Config.parse(source)?
    require config.port > 0 else ConfigError("port must be positive")
    return Ok(config)
```

`?` requires compatible E types (or an explicit, lossless `From` conversion chosen by the programmer). When error sets differ, convert explicitly using `map_error`. In the E4 pure interpreter, runtime failures include a stable code, source span and deterministic function stack. Resource cleanup must run through scoped ownership once resource effects exist. Panic-equivalent catastrophic faults terminate an unsafe/internal operation and must never masquerade as user-catchable validation errors.

### Acceptance rules

Tests must cover negative input, locations on CRLF/Unicode text, autocomplete fix suggestions, parser recovery across multiple declarations, error serialization and consistent exit codes in CMD and PowerShell. The example diagnostic is hypothetical, not output of an existing compiler.
