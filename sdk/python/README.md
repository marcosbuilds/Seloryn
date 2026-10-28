# Seloryn Python bridge

This directory is a dependency-free Python package for calling an installed
Seloryn binary through `run --format=json`.

```text
python -m pip install ./sdk/python
```

The package starts one process per request, uses no shell, and enforces input,
output-stream and timeout limits. It does not provide HTTP, retries, a daemon,
descendant fencing or a stable ABI. See
[`docs/systems/python-bridge.md`](../../docs/systems/python-bridge.md) for the
wire contract and limitations.
