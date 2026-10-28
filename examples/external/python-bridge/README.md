# External Python bridge consumer

This is a small `.sely` project consumed by an application that lives outside
Seloryn's runtime implementation. It reads one JSON request and returns a
structured response, including non-ASCII text and its character count.

From the repository root after building a release binary:

```text
python -m pip install ./sdk/python
python examples/external/python-bridge/consumer.py --binary target/release/seloryn.exe --project examples/external/python-bridge
```

Use `seloryn` instead of `target/release/seloryn.exe` on POSIX systems. The
black-box proof in [`tests/python_bridge_probe.py`](../../../tests/python_bridge_probe.py)
also copies the project to a temporary directory before invoking it.
