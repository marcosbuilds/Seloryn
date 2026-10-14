# JSON project example

This project proves three boundaries together: a local `app.format` module,
the native `json` module and a root-confined read of `data/config.json`.

```text
seloryn check examples/projects/json-config
seloryn run examples/projects/json-config
```

The observed result is `configured model: gpt-local`. The example does not
read arbitrary paths or contact a model provider.
