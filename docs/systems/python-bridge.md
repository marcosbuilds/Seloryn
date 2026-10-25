# Python bridge

Seloryn now ships a dependency-free Python bridge for the bounded local CLI
contract. It turns an installed `seloryn` binary into a process-per-request
callable without invoking a shell:

```python
from seloryn_bridge import SelorynBridge

bridge = SelorynBridge("seloryn", timeout=30)
result = bridge.run("./my-project", input={"mode": "infer", "values": [1, 2]})
print(result.value)
```

Install the SDK from a checkout or release artifact with:

```text
python -m pip install ./sdk/python
```

The bridge uses `seloryn run <project> --format=json`. A successful call must
return the existing `{stdout,value}` envelope; a non-zero process becomes a
`BridgeError` with the Seloryn diagnostic code and exit code. The bridge also
offers `check()` for validation without execution. Input is one UTF-8 JSON
document, and `json.read_stdin` remains the explicit capability boundary in
the `.sely` program.

The SDK enforces a 1 MiB input limit, a 4 MiB capture limit for each stdout or
stderr stream and a 30-second default timeout. These are transport limits, not a
process sandbox. The implementation starts one process per call, does not
provide a daemon, HTTP server, retry policy, descendant fencing or remote
execution. `log="json2"` exposes parsed receipt events through
`BridgeResult.events`; receipt verification remains the responsibility of the
existing JSON2 tools.

The runnable external-consumer proof is in
[`examples/external/python-bridge`](../../examples/external/python-bridge),
with success, Unicode, typed failure, input-bound and copied-project cases in
[`tests/python_bridge_probe.py`](../../tests/python_bridge_probe.py). The
probe invokes only the release binary and the public SDK path; it does not
import Seloryn internals.
