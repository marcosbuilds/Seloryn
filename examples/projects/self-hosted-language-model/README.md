# Self-hosted language model

Este projeto é um protótipo executável de modelo de linguagem pequeno, treinado
inteiramente em `.sely`.

```text
data/*.json
    -> app.tokenizer        vocabulário e IDs próprios
    -> app.data             pares bigram e one-hot
    -> app.model            matmul -> softmax -> log -> cross-entropy
    -> app.training         gradients + SGD + avaliação + persistência
    -> app.checkpoint       JSON versionado e hash SHA-256
    -> app.serving          recarga e geração gulosa
    -> app.production       stdin: train | infer
```

O limite é intencional e honesto: a entrada é JSON com uma lista de tokens
inteiros pertencentes ao vocabulário fixo de 14 palavras/símbolos. Não há
tokenização Unicode, subwords, batching dinâmico ou contexto maior que um
token. O modelo aprende a distribuição do próximo token (`bigram`) e gera
texto seguindo essa distribuição.

## Executar

```powershell
seloryn check examples/projects/self-hosted-language-model
'{"mode":"train"}' | seloryn run examples/projects/self-hosted-language-model --format=json --log=json2
'{"mode":"infer","tokens":["<bos>","the","cat"],"prompt_count":3,"generation_steps":4}' | seloryn run examples/projects/self-hosted-language-model --format=json --log=json
```

O treinamento grava `checkpoints/language-model.json`, valida o hash dos pesos
ao recarregar e calcula a inferência usando o checkpoint, não o objeto em
memória.

## Comparação reproduzível

O probe `tests/self_hosted_language_model_probe.py` treina o mesmo bigram com
uma implementação Python pura, sem framework, para comparar a matemática e o
resultado do runtime:

| Dimensão | Seloryn | Python de referência |
| --- | --- | --- |
| Código do treino | `.sely` (`gradients` + SGD) | Python puro (mesma regra) |
| Função | bigram softmax + log cross-entropy | mesma |
| Dataset | corpus próprio neste projeto | mesmo corpus |
| Persistência | JSON + SHA-256 + reload | memória do probe |
| Inferência | stdin de produção + checkpoint | chamada local |

Os números exatos, curva de loss, hashes e códigos de saída são impressos pelo
probe e só são aceitos quando os invariantes passam.

## Evidência medida

No binário release atual, com 160 passos e taxa `0.5`, os dois executores
produziram os mesmos resultados dentro de `3e-6`:

| implementação | loss inicial | loss final | loss de teste | acurácia | geração |
|---|---:|---:|---:|---:|---|
| Python stdlib | 2.639057330 | 0.829485686 | 1.532877856 | 0.533333 | `the park <eos>` |
| Seloryn `.sely` | 2.639058590 | 0.829485893 | 1.532878041 | 0.533333 | `the park <eos>` |

O treino saiu com código `0`, emitiu 160 eventos `training.progress` em uma
cadeia JSON2 válida, salvou e recarregou o checkpoint, e a segunda invocação
de inferência retornou `park <eos>` com `status=ok`. O caso negativo com um
token desconhecido saiu com código diferente de zero e a mensagem
`unknown token in dataset`. Os hashes e o quadro completo estão em
[`evidence.md`](evidence.md).

```text
cargo build --release
python tests/self_hosted_language_model_probe.py
python tests/compare_language_model.py
```

O limite permanece explícito: este é um baseline bigram com vocabulário fixo,
sem transformer, subwords, batching dinâmico, GPU ou servidor HTTP. A fronteira
de integração comprovada é a API CLI JSON por stdin/stdout com checkpoint
persistido e validado por hash.
