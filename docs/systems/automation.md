# Automation and systems

Future Seloryn scripts can orchestrate files, processes, periodic jobs, validators and outgoing APIs while preserving explicit effect boundaries.

```seloryn
from seloryn.fs import read_text
from seloryn.net import get_json

fn sync(url: Url, path: Path) -> Result[Unit, SyncError]:
    let existing = read_text(path)?
    require existing.length < 1_000_000 else SyncError("file too large")
    let remote = get_json(url, timeout=5.seconds)?
    save_snapshot(existing, remote)?
    return Ok(())
```

Proposed requirements: stable process argument vectors instead of shell string concatenation, working directories and environment isolation, recoverable timeout/cancel, durable scheduling identifiers, no background orphan tasks, file-atomic writes, path traversal protection, bounded log volume, and explicit network reachability permissions (without marketing the runtime as an OS sandbox).

A future scheduler library may support `every`, `cron` and job retries, but those are **libraries**, not new grammar keywords; time-zone and daylight-saving rules must be explicit.
