# Model inference

The target inference contract supports typed model loading, explicit tokenizer, request batching and bounded memory, while keeping tensor model APIs distinct from HTTP serving.

```seloryn
from seloryn.models import Model
from seloryn.text import Tokenizer

fn predict(prompt: Text) -> Result[Text, ModelError]:
    let model = Model.load("model.safetensors")?
    let tokens = Tokenizer.load("tokenizer.json")?.encode(prompt)
    let output = model.generate(tokens, max_tokens=64, seed=27)?
    return Ok(Tokenizer.decode(output))
```

*Future illustrative API, not runnable.*

Required: deterministic input validation, bounded tokens/context and cache, cancellation, acceptable numerical parity, streaming decoder state, correct EOS behavior and resource cleanup. Loading a file must never execute arbitrary embedded code. Distinguish model metadata, operators, weights and tokenizer artifacts and validate compatibility and sizes before allocation.
