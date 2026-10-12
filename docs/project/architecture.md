# Architecture

![Language pipeline](../../assets/architecture-flow.png)

The architecture is defined by owned language semantics from UTF-8 source to deterministic results. **Today the front-end checker, bounded pure interpreter and N1 CPU tensor runtime are implemented experimentally.** The first implementation uses a native systems-language host, but `.sely` is not a wrapper or source-to-source alias for any host language. Verified, established system libraries may be called through constrained interfaces; security-sensitive protocols and cryptography must not be casually reimplemented. [Status](status.md) is the current evidence boundary.

```mermaid
flowchart LR
    subgraph A1[Implemented today]
        A[Source] --> B[Tokens]
        B --> C[AST]
        C --> D[Symbols]
        D --> E[Supported semantic checks]
        E --> F[Pure E4 interpreter]
    end
    subgraph A2[Later stages]
        G[Core IR]
        H[Tensor graph]
        I[Future optimized tensor backend]
        J[Accelerator backend]
        K[Services]
        L[Model training]
    end
    F -. future IR .-> G
    F --> H
    H -. future N2 .-> I
    H -. future A1 .-> J
    G -. future E6 .-> K
    I -. future N4 .-> L
    J -. future A2/A3 .-> L
```

Legend: **✓ implemented** means the checker/interpreter has source and tests; **⚠️ partial** means a bounded subset exists; **✗ not implemented** means the diagram is a design dependency only. Core IR, services and training are currently ✗; N1 is a bounded CPU reference, not a full tensor engine.

### Subsystems

| Component | Responsibility | Contract |
|---|---|---|
| Front end | UTF-8 scanning, block tokens, recovery parser, source spans | No successful execution of malformed input |
| Semantic core | **⚠️ Partial:** name binding, scalar type constraints, exhaustiveness and fallibility checks | Diagnosable static behavior; no runtime execution |
| Core IR | **✗ Not implemented** | Typed operations and optimization remain design work |
| Execution | **⚠️ Experimental E4/N1:** pure tree-walk interpreter plus CPU tensor builtins | No effects, core IR, modules or accelerators |
| Tensor engine | **⚠️ Experimental N1:** owned F32/I64 CPU buffers, checked shapes, broadcasting, total/axis reductions and rank-2 matmul with bounded resident memory | No symbolic shapes, views, autodiff or device movement |
| Device interface | **✗ Not implemented** | CPU and accelerator providers require later evidence |
| System library | **✗ Not implemented** | Streams, HTTP, files and subprocesses require later gates |
| Product tools | **⚠️ Partial:** development CLI only; packages, formatter, LSP and installer are future | Platform-specific measured support |

### Capability sequencing

The public syntactic API of a future feature is only *accepted* once the underlying behavior is implemented, tested and versioned. Language syntax cannot be considered an alternative for working matrix multiplication, reverse-mode autodiff or performant kernels. The next executable dependency after the bounded E4/N1 runtime is [autograd](../compute/autograd.md) → [training](../compute/training.md) → [LLMs](../compute/llms.md).
