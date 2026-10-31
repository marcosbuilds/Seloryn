# Language-model capability boundary

Seloryn has bounded contextual next-token workflows in [`language-model.md`](language-model.md). The one-hot linear and embedding/ReLU MLP paths are verified proofs of an end-to-end CPU path, not claims of transformer, foundation-model or production-serving capability.

### Technical layers

| Layer | What must genuinely exist |
|---|---|
| Tokenization | Byte-level/BPE or unigram tokenizer, streaming decode and Unicode edge cases |
| Layers | Embeddings, norm, attention, residual connections, MLP, losses |
| Kernels | GEMM, fused normalization/softmax/attention where supported |
| Memory | Activation planning, checkpointing, stable allocator, transfer scheduling |
| Training | Autodiff, clipping, mixed precision, optimizer, scheduler, gradient accumulation |
| Inference | Decode/forward, KV cache, causal masks, sampling, batching |
| Model I/O | Versioned tensor serialization and safe metadata, checkpoint compatibility |
| Multi-device | Explicit devices, collectives, fault recovery, sharding and profiling |
| Deployment | Token-stream services, cancellation, timeouts, quotas, audit and isolation |

### Current verified boundary

- Scalar autodiff, matrix kernels and CPU MLP training are verified previews.
- The contextual language-model proof includes a bounded one-hot linear model
  and a separately selectable embedding/ReLU MLP model, both with a short
  context window, source-owned training and hash-bound JSON checkpoints.
- Subword tokenization, attention, transformer layers, GPU tensor training,
  distributed execution and HTTP serving are unsupported boundaries. The
  embedding path is CPU-only and bounded; it does not imply a transformer.
- A new capability is accepted only with an independent numerical reference,
  negative tests and published environment-specific evidence.

### No false platform claims

CUDA, ROCm, Metal and other APIs require separately validated kernels/builds/driver/runtime combinations. ONNX, StableHLO, MLIR and SafeTensors are potential *interoperability targets or implementation dependencies*, not automatically working features. Safety and license implications must be reviewed per provider.

### Evaluation

Training loss curve, held-out perplexity, shape/dtype tests, throughput tokens/s, peak memory, compilation warm-up, CPU parity, deterministic seed limits, device count, and inference latency percentile. Publish environment details and measurement methodology. Claims of competing performance require public reproducible benchmarks.
