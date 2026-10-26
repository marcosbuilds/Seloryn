# Self-hosted language model

Este projeto é um protótipo executável de modelo de linguagem pequeno, treinado
inteiramente em `.sely`.

```text
data/*.json
    -> app.tokenizer        vocabulário e IDs próprios
    -> app.data             janelas de contexto e one-hot
    -> app.model            matmul contextual -> cross-entropy fundida
    -> app.parameters       parâmetros nomeados reutilizáveis
    -> app.optim            SGD reutilizável
    -> app.training         gradients + SGD + avaliação + persistência
    -> app.checkpoint       JSON versionado e hash SHA-256
    -> app.serving          recarga, texto livre e geração gulosa
    -> app.production       stdin: train | infer | infer_text
```

O limite é intencional e honesto: `data/vocab.json` define um vocabulário e o
treino recebe `context_size` e `steps`. As dimensões das matrizes, janelas e
one-hot são derivados dos dados, portanto o mesmo código aceita datasets e
vocabulários de tamanhos diferentes. O fixture usa contexto 4, calcula
perplexidade, valida hash dos pesos e do vocabulário e rejeita vocabulários
duplicados ou reordenados. `infer_text` separa whitespace Unicode, mas ainda
exige tokens presentes no vocabulário. Não há embeddings, subwords, batching
dinâmico, amostragem ou transformer.

## Executar

```powershell
seloryn check examples/projects/self-hosted-language-model
'{"mode":"train","context_size":4,"steps":160}' | seloryn run examples/projects/self-hosted-language-model --format=json --log=json2
'{"mode":"infer_text","text":"<bos> the cat sat","generation_steps":4}' | seloryn run examples/projects/self-hosted-language-model --format=json --log=json
```

O treinamento grava `checkpoints/language-model.json`, valida os hashes dos
pesos e do vocabulário ao recarregar e calcula a inferência usando o checkpoint,
não o objeto em memória.

## Comparação reproduzível

O probe `tests/self_hosted_language_model_probe.py` treina a mesma janela
contextual com uma implementação Python pura, sem framework, para comparar a matemática e o
resultado do runtime:

| Dimensão | Seloryn | Python de referência |
| --- | --- | --- |
| Código do treino | `.sely` (`gradients` + SGD) | Python puro (mesma regra) |
| Função | linear contextual + cross-entropy fundida | mesma |
| Dataset | corpus próprio neste projeto | mesmo corpus |
| Persistência | JSON + SHA-256 + reload | memória do probe |
| Inferência | stdin de produção + checkpoint | chamada local |

Os números exatos, curva de loss, hashes e códigos de saída são impressos pelo
probe e só são aceitos quando os invariantes passam. O log de treino é real e
encadeado; para evitar uma segunda avaliação cara em cada atualização, a perda
é emitida nos passos 1, múltiplos de 8 e 160 (21 eventos para 160 updates).

## Evidência medida

No binário release atual, com contexto `4`, 160 passos e taxa `0.5`, os dois
executores produziram os mesmos resultados dentro de `1e-5`:

| implementação | loss inicial | loss final | loss de teste | perplexidade | acurácia | geração |
|---|---:|---:|---:|---:|---:|---|
| Python stdlib | 2.639057330 | 0.181598135 | 1.088078311 | 2.96856393 | 0.666667 | `in in park <eos>` |
| Seloryn `.sely` | 2.639057398 | 0.181598127 | 1.088078260 | 2.96856380 | 0.666667 | `in in park <eos>` |

O treino saiu com código `0`, emitiu 21 eventos `training.progress` mais os
eventos source-owned de contrato e conclusão em uma cadeia JSON2 válida de 25
eventos, e o probe também reproduziu o treino em cópia isolada. Salvou e
recarregou o checkpoint, e a segunda invocação de inferência por texto livre
retornou uma previsão com `status=ok`. Casos negativos cobrem token desconhecido,
prompt menor que a janela, vocabulário duplicado e reordenação do vocabulário
com o mesmo tamanho; todos saem com código diferente de zero.
Os hashes e o quadro completo estão em
[`evidence.md`](evidence.md).

```text
cargo build --release
python tests/self_hosted_language_model_probe.py
python tests/compare_language_model.py
```

O limite permanece explícito: este é um baseline linear contextual com one-hot,
sem embeddings, transformer, subwords, batching dinâmico, GPU ou servidor HTTP. A
fronteira de integração comprovada é a API CLI JSON por stdin/stdout com
checkpoint persistido e validado por hash; a inferência não revalida os hashes
dos datasets porque não carrega datasets nessa operação.
