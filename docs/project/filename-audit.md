# Auditoria cirúrgica de nomes de arquivos e diretórios

## Escopo e conclusão

Auditoria realizada em `D:/Project/Seloryn/seloryn`, no commit `621821689be96ed987e4ec299a91e952ca527d40`, com foco em caminhos rastreados pelo Git, `research/agent-token-economy`, testes, scripts e workflows.

Nenhum arquivo de código foi alterado. Este relatório é o único arquivo produzido por esta auditoria.

O único candidato que justifica uma simplificação imediata é `research/agent-token-economy/transcript_token_count.py`. A recomendação é introduzir `transcript.py` como nome canônico e manter `transcript_token_count.py` como wrapper de compatibilidade, incluindo o encaminhamento da CLI. Uma renomeação direta não é segura porque o nome está presente em workflow, documentação, teste, no prompt do experimento congelado e no carregamento dinâmico do teste.

Os demais nomes longos são, em sua maioria, contratos ou identificadores úteis: nomes de tarefas congeladas, fixtures, probes independentes, entradas declaradas em `seloryn.toml`, referências de benchmark, scripts por plataforma e relatórios canônicos. Encurtá-los agora reduziria rastreabilidade ou exigiria uma migração ampla sem benefício proporcional.

## Inventário executado

Resultados observados:

| Escopo | Resultado |
| --- | ---: |
| Arquivos rastreados por `git ls-files` | 353 |
| Maior caminho rastreado | 86 caracteres |
| Caminhos com 70 ou mais caracteres | 39 |
| Caminhos com 80 ou mais caracteres | 19 |
| Caminhos com 90 ou mais caracteres | 0 |
| `research/agent-token-economy` | 57 arquivos |
| `tests` | 42 arquivos |
| `benchmarks` | 20 arquivos |
| `.github/workflows` | 3 arquivos |
| `scripts` | 13 arquivos |
| `examples/projects` | 97 arquivos |

O primeiro estado observado do Git estava limpo (`## master...origin/master`). Na verificação final havia cinco caminhos não rastreados em `research/agent-token-economy/paired/`: `TOKEN-PAIR-01.manifest.json`, `python_task.py`, `python_task_test.py`, `seloryn_task.sely` e `seloryn_task_test.py`. Eles apareceram durante o trabalho paralelo, não foram criados, editados, renomeados ou removidos por esta auditoria e não fazem parte do inventário rastreado. Também foi observada uma cache Python ignorada (`__pycache__`), que não é um caminho versionado.

## Critério de seleção

Um caminho foi tratado como candidato quando atendia pelo menos um destes critérios:

1. nome-base com 30 ou mais caracteres;
2. caminho completo com 70 ou mais caracteres;
3. repetição aparente do identificador do diretório no arquivo;
4. nome citado em workflow, manifest, teste, documentação ou contrato, onde o risco de renomear precisava ser explicitamente avaliado.

Comprimento isolado não foi considerado defeito. A recomendação leva em conta semântica, estabilidade do contrato, descoberta por glob, referências textuais e possibilidade de compatibilidade.

## Candidatos e recomendações

### 1. Contador de transcript: ação recomendada

| Caminho | Comprimento (caminho / nome) | Papel | Referências encontradas | Risco | Recomendação |
| --- | ---: | --- | --- | --- | --- |
| `research/agent-token-economy/transcript_token_count.py` | 54 / 25 | CLI e biblioteca que mede texto visível de transcript usando o adaptador local | `.github/workflows/ci.yml`; `docs/benchmarks/agent-token-economy.md`; `research/agent-token-economy/README.md`; prompt em `fixtures/experiments/TOKEN-AGENT-01.json`; carregamento por caminho em `tests/test_transcript_token_count.py` | Alto: workflow compila o caminho, documentação o executa, o teste carrega o módulo e o fixture é uma evidência congelada | **Renomear com wrapper de compatibilidade**: adicionar `transcript.py` como nome canônico, manter este arquivo como shim com a mesma CLI e atualizar referências novas gradualmente. Não alterar o texto histórico do fixture congelado |
| `research/agent-token-economy/tests/test_transcript_token_count.py` | 65 / 30 | Teste de contrato do contador de transcript | `docs/benchmarks/agent-token-economy.md`; `research/agent-token-economy/README.md`; execução direta documentada; importa dinamicamente `token_count.py` e `transcript_token_count.py` | Médio: não é API de produção, mas documentação e execução manual dependem do nome | **Renomear diretamente**, junto com a migração do módulo, para `test_transcript.py`; atualizar documentação no mesmo commit |

### 2. Contratos da suíte de token economy: manter

| Caminho ou família | Comprimento observado | Papel e referências | Risco | Recomendação |
| --- | ---: | --- | --- | --- |
| `research/agent-token-economy/requirements-tokenizer.txt` | 55 / 26 | Lista opcional de dependência do tokenizer; citada pelo `README.md` | Baixo; identifica claramente a dependência | **Manter** |
| `research/agent-token-economy/schema/visible-transcript.schema.json` | 66 / 30 | Schema `seloryn.agent-visible-transcript.v1`; usado em `.github/workflows/ci.yml` e documentado no `README.md` | Alto: localização do contrato e comando de validação dependem dele | **Manter** |
| `research/agent-token-economy/fixtures/experiments/TOKEN-AGENT-01.json` | 69 / 17 | Transcript real congelado; o conteúdo cita os nomes dos contadores | Alto: evidência histórica imutável | **Manter** |
| `research/agent-token-economy/fixtures/experiments/TOKEN-AGENT-01.token-count.json` | 81 / 31 | Resultado derivado do experimento; citado no relatório final | Médio/alto: o sufixo identifica a saída de contagem | **Manter** |
| `research/agent-token-economy/manifest.json`, `runner.py`, `report-format.md` e `README.md` | 42, 38, 45 e 38 no caminho completo | Manifesto, executor, formato de relatório e documentação | Alto: nomes já são curtos dentro do domínio | **Manter** |

### 3. Tarefas, fixtures e replay: manter como contratos congelados

As famílias abaixo têm caminhos longos, mas o nome codifica o ID e o papel da evidência. A suíte usa os padrões `accept-*.py` e `contract-*.json`, e cada tarefa declara seus próprios arquivos. Uma renomeação exigiria alterar manifestos, comandos de aceitação, glob de descoberta e o formato público da suíte.

| Família exata | Inventário | Referências e risco | Recomendação |
| --- | --- | --- | --- |
| `research/agent-token-economy/tasks/` | 10 especificações: `a01-diagnostic-locality.json`, `a02-fallibility-boundary.json`, `a03-data-pipeline.json`, `a04-checkpoint-integrity.json`, `a05-source-owned-inference.json`, `b01-project-contract.json`, `b02-editor-lifecycle.json`, `b03-replay-receipt.json`, `b04-job-recovery.json`, `b05-maintenance-documentation.json` | `manifest.json` aponta cada spec; cada spec aponta fixture, adapter e observações. O maior nome-base é `b05-maintenance-documentation.json` (34; caminho 69). Risco alto por serem IDs semânticos | **Manter** |
| `research/agent-token-economy/fixtures/a01-diagnostic-locality` até `b05-maintenance-documentation` | 10 diretórios, 23 a 29 caracteres | Os specs apontam os diretórios; o `runner.py` descobre `accept-*.py` e `contract-*.json`. O diretório `b05-maintenance-documentation` produz caminhos de até 86 caracteres | **Manter** |
| `research/agent-token-economy/fixtures/*/accept-*.py` | 10 adapters `accept-a-01.py` … `accept-b-05.py`; caminhos de até 82 caracteres | Os comandos de aceitação nos specs apontam esses nomes e o runner valida o padrão `accept-*.py` | **Manter** |
| `research/agent-token-economy/fixtures/*/contract-*.json` | 10 contratos `contract-a-01.json` … `contract-b-05.json`; caminhos de até 86 caracteres | O runner descobre o padrão `contract-*.json`; são artefatos da tarefa, não redundâncias acidentais | **Manter** |
| `research/agent-token-economy/fixtures/replay/workspaces/{A-01..B-05}/submission-*.json` | 10 submissões; caminhos de até 81 caracteres | Cada adapter lê `submission-<id>.json`; os specs registram `adapter_file`. O ID repetido torna o arquivo autoidentificável fora do diretório | **Manter** |
| `research/agent-token-economy/fixtures/replay/provider-records.jsonl` | 67 | Entrada de replay referenciada pelos comandos da documentação | Médio; nome segue a entidade do schema | **Manter** |

### 4. Probes de testes e evidências: manter

Os probes usam a convenção deliberada `<capacidade>_probe.py`. Os nomes longos são referências públicas em CI, `tests/conformance_matrix.json`, `VERSION.toml`, documentação e/ou releases. Encurtar para nomes genéricos como `probe.py`, `model.py` ou `reliability.py` pioraria a identificação quando o artefato fosse visto isoladamente.

| Caminho | Comprimento (caminho / nome) | Referências encontradas | Risco | Recomendação |
| --- | ---: | --- | --- | --- |
| `tests/self_hosted_language_model_probe.py` | 41 / 35 | 11 arquivos: CI, release, `VERSION.toml`, README, docs de linguagem/status/releases, matriz de conformidade, README/evidence do projeto | Alto: executado em CI e no release; referência de capacidade verificada | **Manter** |
| `tests/auditable_local_inference_probe.py` | 40 / 34 | 7 arquivos: CI, README, docs de benchmark/status, examples README, evidence e matriz | Alto: probe independente e caminho público de evidência | **Manter** |
| `tests/operational_reliability_probe.py` | 38 / 32 | 4 arquivos: CI, benchmark, status e matriz | Alto: caminho de conformance e gate operacional | **Manter** |
| `tests/structured_inspection_probe.py` | 36 / 30 | 6 arquivos: README, CLI guide, status, relatório, tests README e matriz | Alto: evidência de `seloryn inspect` | **Manter** |
| `tests/source_contract_reuse_probe.py` | 36 / 30 | 3 arquivos: status, matriz e README do projeto | Médio/alto: prova nomeada da capacidade de reuso | **Manter** |
| `tests/project_manifest_probe.py`, `tests/release_staging_probe.py` e demais `*_probe.py` | 39 / 27 e 38 / 25; família com 32 probes | CI/release, `VERSION.toml`, matriz, status e documentação conforme a capacidade | Médio/alto: a convenção e os IDs são mais importantes que alguns caracteres poupados | **Manter** |

### 5. Entradas de projetos `.sely`: manter agora; possível migração em lote

Há 14 arquivos `*-entry.sely` em projetos/fixtures. Os três maiores exemplos são:

| Caminho | Comprimento (caminho / nome) | Referências encontradas | Risco | Recomendação |
| --- | ---: | --- | --- | --- |
| `examples/projects/self-hosted-language-model/src/self-hosted-language-model-entry.sely` | 86 / 37 | `seloryn.toml` e documentação de language model; o manifest declara `entry` | Alto: altera o ponto de entrada de projeto executável e aparece em documentação | **Manter** |
| `examples/projects/auditable-local-inference/src/auditable-local-inference-entry.sely` | 84 / 36 | `seloryn.toml` e evidence/README por referência indireta ao projeto | Alto: projeto de evidência verificado | **Manter** |
| `examples/projects/checkpoint-validation/src/checkpoint-validation-entry.sely` | 76 / 32 | `seloryn.toml` | Médio/alto: entrypoint declarado no manifest | **Manter** |
| Demais `*-entry.sely` (`evaluation-pipeline`, `gpu-linear-training`, `json-array-pipeline`, `json-config`, `linear-training`, `mlp-inference`, `mlp-training`, `self-hosted-linear`, `self-hosted-mlp`, `source-contract-reuse` e fixture `inspection-entry.sely`) | 62–72 no caminho; nomes de 22–32 | Cada um é apontado por seu `seloryn.toml`; alguns aparecem em docs/probes | Médio/alto | **Manter** |

Existe uma simplificação conceitual possível para uma etapa futura: padronizar todos os entrypoints como `main.sely` ou `entry.sely`. Isso seria uma renomeação direta em lote, com atualização dos manifests e referências, e não deve ser misturado com a migração do contador de transcript.

### 6. Benchmarks, documentação e scripts operacionais: manter

| Caminho ou família | Comprimento observado | Papel, referências e risco | Recomendação |
| --- | ---: | --- | --- |
| `benchmarks/references/bench_02_data_transformation.py`, `bench_03_numerical_computation.py`, `bench_04_small_neural_network.py` | 51, 55 e 54 no caminho; nomes de 31, 33 e 32 | Referenciados por `benchmarks/manifest.json`; `bench_04` também pelo teste de contratos e `bench_03` pelo runner. O nome mantém correspondência com `BENCH-02`…`BENCH-04` | **Manter** |
| `benchmarks/fixtures/bench-03-numerical-computation.sely` e `bench-06-error-handling.sely` | 57 e 50; nome de 35 e 28 | Runner e README usam os nomes como fixtures do caso de benchmark | **Manter** |
| `docs/project/engineering-evolution-report.md` | 44 / 31 | Relatório canônico, referenciado por `docs/index.md` e `docs/project/engineering-evolution.md` | **Manter**; `report.md` seria curto, mas ambíguo |
| `docs/benchmarks/agent-token-economy.md` | 38 / 22 | Documentação do domínio e referência aos contadores e comandos | **Manter** |
| `scripts/install/install-seloryn-windows.ps1` | 43 / 27 | Instalador Windows; citado em `VERSION.toml`, guia de instalação, matriz e probe | **Manter**; o sufixo de plataforma é necessário |
| `scripts/install/install-seloryn-posix.sh` | 40 / 24 | Instalador POSIX; o próprio script documenta seu uso | **Manter**; o sufixo de plataforma é necessário |

## O que não deve ser renomeado

- `agent-token-economy`, `self-hosted-language-model`, `auditable-local-inference` e `source-contract-reuse` são nomes de domínio ou projetos, não redundâncias acidentais.
- `accept-*`, `contract-*`, `submission-*` e `*-entry.sely` são convenções de descoberta ou campos de manifest. Encurtar nomes individualmente quebraria uniformidade e aumentaria mapeamentos.
- `visible-transcript.schema.json` e `TOKEN-AGENT-01.token-count.json` são artefatos de evidência/contrato; o comprimento acrescenta identificação útil.
- Nomes de probes e referências de benchmark fazem parte da trilha de evidência. Não devem ser trocados por nomes genéricos só para reduzir o comprimento.

## Plano de migração recomendado

1. Em uma alteração isolada, adicionar `research/agent-token-economy/transcript.py` como implementação canônica, preservando a mesma função pública e a mesma CLI.
2. Transformar `transcript_token_count.py` em wrapper de compatibilidade que encaminha importações e execução `__main__` para `transcript.py`.
3. Renomear diretamente o teste para `test_transcript.py` e atualizar apenas documentação, comandos novos e referências não históricas.
4. Manter o texto de `TOKEN-AGENT-01.json` e o resultado associado como evidência congelada; eles podem continuar registrando o nome antigo para preservar reprodução histórica.
5. Executar a suíte de token economy, `py_compile`, validação do schema e o workflow de CI antes de qualquer remoção futura do wrapper.
6. Tratar a padronização dos entrypoints para `main.sely` como uma mudança separada, com atualização dos 14 manifests e revisão do impacto nos exemplos.

## Comandos executados

Os comandos abaixo foram executados a partir da raiz do repositório:

```text
git status --short --branch
git ls-files
git ls-files --others --exclude-standard
git ls-files --deleted
git rev-parse HEAD
rg --files -g "!target" -g "!.git"
rg -n --hidden -g "!target" -g "!.git" "transcript_token_count|token_count|agent-token-economy|filename-audit" .
rg -n --hidden -g "!target" -g "!.git" "self_hosted_language_model_probe|auditable_local_inference_probe|operational_reliability_probe|source_contract_reuse_probe|structured_inspection_probe|project_manifest_probe|release_staging_probe|entry.sely|visible-transcript.schema|engineering-evolution\\.md" .
```

Também foram usadas consultas PowerShell sobre a saída de `git ls-files` para calcular comprimento de caminho e nome-base, separar os maiores caminhos e contar arquivos por escopo. Para referências de candidatos, foi usada busca literal com `rg -l --fixed-strings`.

## Limitações da auditoria

- Referências construídas dinamicamente, aliases de shell, caches, artefatos gerados e nomes mantidos fora do repositório não podem ser provados apenas por busca textual.
- `rg -l` mede referências textuais; não prova que uma referência é executada nem detecta todos os usos indiretos por glob, importação dinâmica ou convenção.
- O inventário rastreado contém 353 arquivos; os cinco caminhos não rastreados em `research/agent-token-economy/paired/` foram somente observados e ficaram fora da recomendação de renomeação.
- Nenhuma renomeação, atualização de workflow, alteração de manifest ou teste de migração foi realizada, pois o escopo desta tarefa era produzir somente a auditoria.
