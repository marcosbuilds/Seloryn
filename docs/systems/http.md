# HTTP and local servers

Seloryn is intended for localhost services, API clients, webhooks and long-running application servers.

```seloryn
from seloryn.http import Server, Request, Response

fn health(request: Request) -> Result[Response, HttpError]:
    return Ok(Response.json({"status": "ready"}))

fn main() -> Result[Unit, ServerError]:
    let server = Server.bind("127.0.0.1", 8080)?
    server.route("GET", "/health", health)?
    return server.serve()
```

This is a **future design**; no HTTP backend or CLI exists. Binding to loopback is the safe default; listening on all interfaces requires explicit configuration and a deployment/security review.

### Contract checklist

- Methods, status codes, content length and streaming parsers with strict limits.
- HTTPS with certificate validation and trusted cryptographic libraries.
- Timeout, retry and redirect policies; avoid unsafe retry of non-idempotent calls.
- SSRF mitigation on user-supplied remote URLs; DNS rebinding and redirect revalidation.
- Per-connection concurrency limits, cancellation and graceful shutdown.
- Structured typed errors without leaking tokens; redacted tracing and logs.
- Header/JSON size constraints, request body streaming and backpressure.
- Authentication and authorization as versioned library capabilities, not implicit framework magic.

Static routing syntax should not be overloaded to create a special syntax language. Functions, Result and typed request/response structures remain the dominant user contract.
