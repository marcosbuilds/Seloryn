# Language models: roadmap contract

The long-range goal is **real foundation-model and LLM training/inference**, through a capable runtime and dedicated tensor/NN abstractions. Seloryn now has a separate, bounded next-token baseline in [`language-model.md`](language-model.md); it is a verified proof of the end-to-end path, not a claim of transformer or foundation-model capability.

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

### Reference model trajectory

1. Scalar autodiff and matrix tests.
2. CPU multilayer perceptron training on real data.
3. Bounded runtime-sized bigram next-token baseline on CPU — **verified preview**.
4. Minimal next-token transformer (small vocabulary, short context) on CPU.
5. Efficient accelerator kernels and correct mixed precision.
6. Save/resume checkpoints and interoperable model I/O.
7. Practical multi-device training and user-facing inference serving, proven by workloads and metrics.

### No false platform claims

CUDA, ROCm, Metal and other APIs require separately validated kernels/builds/driver/runtime combinations. ONNX, StableHLO, MLIR and SafeTensors are potential *interoperability targets or implementation dependencies*, not automatically working features. Safety and license implications must be reviewed per provider.

### Evaluation

Training loss curve, held-out perplexity, shape/dtype tests, throughput tokens/s, peak memory, compilation warm-up, CPU parity, deterministic seed limits, device count, and inference latency percentile. Publish environment details and measurement methodology. Claims of competing performance require public reproducible benchmarks.
