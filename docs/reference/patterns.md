# Patterns, guards and composition

The fundamental design forms are **records** (named data), **enums** (closed
variants), **Result** (explicit outcomes), **match** (exhaustive branching),
**require** (validation) and **pipeline** (transformation). The E4 interpreter
executes their validated scalar subset; structural record patterns, effects and
future library APIs remain design concepts. `if` is available, but it is not the
entire language model.

```seloryn
enum JobState:
    Pending
    Running(progress: F32)
    Finished(output: Text)
    Failed(reason: Text)

fn render(job: JobState) -> Text:
    match job:
        case Pending:
            return "Queued"
        case Running(progress):
            return "Running"
        case Finished(output):
            return output
        case Failed(reason):
            return "Failed: " + reason
```

### Guards (distinct from validation)

A `case Pattern if condition` guards an already matched pattern with a Bool expression. A guarded case cannot prove total coverage alone because its predicate could be false.

```seloryn
match value:
    case Ok(item) if item.size > 0:
        print("non-empty")
    case Ok(_):
        print("empty")
    case Err(problem):
        print(problem.message)
```

### Validators

`require` never throws and does not pretend arbitrary strings are errors; it constructs `Err(E)` in the current `Result` function. Multiple validators can be ordinary composition of result-returning functions. A separate schema-validator library can later expose declarative `rule` definitions **without modifying core grammar**.

### Avoid syntax sprawl

New statements must prove an ergonomic, correctness or performance advantage over functions, enums and existing operations. Do not add a new keyword just because a use case exists. Pattern exhaustiveness and Result typing are required acceptance gates.
