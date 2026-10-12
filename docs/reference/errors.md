# Errors and diagnostics

Failures have two distinct layers: **compiler diagnostics** for invalid source, and **typed runtime results** for valid programs encountering failures. No generic `except: pass`, no false-success return values.

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

A diagnostic has stable `code`, severity, module, source span (byte offsets and Unicode-aware display columns), primary and secondary labels, help, machine-readable JSON form and reproducible ordering. No stacktrace dump by default.

| Family | Codes | Example |
|---|---|---|
| Lexical | `SEL-L***` | mixed indentation, malformed UTF-8 |
| Parsing | `SEL-P***` | missing colon, invalid pattern |
| Typing | `SEL-T***` | incompatible units, non-Bool condition |
| Module | `SEL-M***` | missing module, cyclic initialization |
| Tensor runtime | `SEL-R201`–`SEL-R209` | invalid shape, per-tensor/global budget, dtype, broadcast or numeric failure |
| Result/control flow | `SEL-R***` | invalid `require`, non-exhaustive `match`, bad `?` flow |
| Runtime | `SEL-R***` | overflow, division by zero, invalid call or impossible match |
| Security | `SEL-S***` | denied network destination |
| Tooling | `SEL-C***` | unsupported CLI command or target |

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
