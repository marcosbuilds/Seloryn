# Local service — design example

**Not runnable.** This documents expected API ergonomics and loopback safety.

```seloryn
from seloryn.http import Server, Request, Response

record Health:
    status: Text
    revision: Text

fn status(request: Request) -> Result[Response, HttpError]:
    let report = Health(status="ready", revision="unreleased")
    return Ok(Response.json(report))

fn main() -> Result[Unit, ServerError]:
    let app = Server.bind("127.0.0.1", 8080)?
    app.route("GET", "/status", status)?
    return app.serve()
```

Expected future test: `GET /status` on loopback returns valid JSON and a proper content type; refused public binding without explicit opt-in; server shuts down cleanly on Ctrl+C.
