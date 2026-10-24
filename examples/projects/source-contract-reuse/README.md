# Source-level optimizer contract reuse

This small example is the proposed consolidation point for the overlap between
`auditable-local-inference` and `self-hosted-language-model`.

`src/contract/parameters.sely` and `src/contract/optim.sely` contain one
source-owned contract for `Parameter`, `replace`, `Sgd`, `sgd`, `sgd_rate` and
`step`. Two independent consumers import those same modules:

- `consumer.mlp` updates a two-element weight vector with `sgd_rate`;
- `consumer.language_model` updates a two-by-two weight tensor with `sgd`.

The entrypoint reports both results. The probe therefore checks real module
reuse and numerical behavior, rather than only comparing text or introducing a
registry. The intended migration is incremental: a future example can import
`contract.*` inside its own project and keep its model-specific data,
checkpoint and audit contracts local.

## Scope and stability boundary

This is a source-level example contract, not a package, registry, resolver or
runtime API. It is not an ABI, does not promise cross-project imports, and does
not freeze tensor layout, optimizer state, checkpoint formats or error values.
The `contract.*` module names and signatures remain experimental until a later
release gate explicitly versions and tests them. Existing examples are left
unchanged in this slice; adopting this contract in them is a separate change
that must first prove equivalent training and checkpoint behavior.

## Falsifiable promotion criteria

This example is ready to guide a later migration only while all of these remain
true:

1. both consumers import `contract.parameters` and `contract.optim` and there
   is no consumer-local copy of either module;
2. `check` and `run` pass, and the probe observes `0.95` and `2.9` within the
   stated tolerance;
3. the migration preserves each existing project's model-specific contracts,
   checkpoint fields, receipt events and negative cases;
4. no claim is made that a module can be imported across project roots: the
   current loader deliberately confines local imports to one `source_root`.

If criterion 3 or 4 cannot be met without a resolver, package format or
versioned ABI, this example remains a proposal and the existing projects must
keep their local copies.

Run it with:

```powershell
seloryn check examples/projects/source-contract-reuse
seloryn run examples/projects/source-contract-reuse --format=json
python tests/source_contract_reuse_probe.py
```
