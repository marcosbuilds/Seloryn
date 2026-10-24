# Seloryn — política de nomes

Status: política proposta para adoção incremental na linha `0.12`.

Esta política transforma a preferência por nomes curtos em uma regra de
engenharia verificável. O objetivo é reduzir atrito de leitura e integração
sem apagar a semântica dos contratos, quebrar referências históricas ou
confundir um alias com uma capacidade já implementada.

## Princípios

1. **Curto, mas inequívoco.** Um nome pode ser reduzido somente quando a
   leitura isolada ainda revela seu papel e seu domínio.
2. **Compatibilidade antes de remoção.** O nome existente continua válido
   durante a migração; o nome curto é adicionado, testado e documentado antes
   de qualquer depreciação.
3. **Contratos não são cosmética.** IDs de tarefas, fixtures, schemas,
   relatórios e probes independentes preservam o nome que explica sua
   identidade fora do diretório.
4. **O código real decide.** Um alias só pode ser adotado depois de apontar
   para a implementação existente e de possuir casos positivos e negativos.
5. **Uma ideia por segmento.** Evitar repetir o diretório no nome do arquivo,
   salvo quando o arquivo precisa ser autoidentificável fora dele.
6. **Histórico é evidência.** Não renomear o passado para fazê-lo parecer mais
   organizado; registrar mapeamentos quando uma migração for necessária.

## Forma canônica para nomes novos

| Superfície | Regra | Limite prático | Exemplos |
| --- | --- | ---: | --- |
| Branch | `<classe>/<assunto>` em minúsculas, kebab-case; 1–3 palavras no assunto | 32 caracteres no assunto | `proof/tokens`, `feature/context`, `fix/job-lease` |
| Diretório de produto | substantivo em minúsculas, kebab-case | 32 caracteres | `agent-runtime`, `token-lab` |
| Módulo Rust | snake_case, nome do domínio, sem prefixo redundante | 32 caracteres | `tensor`, `runtime`, `analysis` |
| Módulo Python | snake_case; um substantivo ou verbo principal | 32 caracteres | `transcript.py`, `token_count.py` |
| Arquivo de contrato/evidência | `<id>.<papel>.<extensão>` quando o ID for parte da identidade | sem limite artificial | `TOKEN-PAIR-01.python.transcript.json` |
| Projeto `.sely` | nome do projeto no diretório; entrada curta somente se não for artefato público isolado | 32 caracteres | `main.sely` ou `entry.sely` |
| Comando CLI | verbo curto; subcomando de uma palavra quando possível | 16 caracteres | `inspect`, `job status`, `job claim` |
| Builtin tensorial | prefixo `tensor_` + operação; preservar dtype quando o contrato o expõe; usar nome sem prefixo apenas quando o contexto já for inequívoco | 32 caracteres | `tensor_get_f32`, `tensor_zeros_f32`, `tensor_sum_to_shape` |

O limite é um gatilho de revisão, não uma ordem para truncar nomes. Um nome
maior é aceitável quando o comprimento carrega um ID, um papel de evidência,
uma plataforma ou uma distinção necessária para descoberta automática.

## Branches

`master` permanece exatamente com esse nome e continua sendo a branch oficial.
Branches de trabalho são curtas e temporárias:

- `proof/tokens` para a prova pareada de tokenização;
- `research/agent-runtime` para pesquisa do runtime de agentes;
- `feature/context` para uma capacidade de contexto;
- `fix/job-lease` para uma correção reproduzida;
- `release/0.12.0` somente durante uma preparação real de release.

O nome atual `proof/token-pair-and-governance` é compreensível, porém longo.
Para trabalho novo, o equivalente canônico é `proof/tokens`. A troca de uma
branch que já tenha PR deve preservar o PR, o SHA e as regras de proteção; se a
plataforma não permitir alterar o head com segurança, mantém-se a branch até o
merge e aplica-se a convenção no próximo ciclo. Nunca usar force-push para
encurtar uma branch.

## Arquivos e diretórios

Nomes novos devem passar por quatro perguntas:

1. O nome continua claro quando copiado para uma mensagem ou log?
2. O diretório já fornece o domínio que está sendo repetido?
3. Há referências em workflow, manifest, teste, exemplo ou documentação?
4. O nome é uma identidade congelada de experimento, ou apenas uma API local?

`transcript.py` é o exemplo aprovado: é o nome canônico curto do contador de
transcript. `transcript_token_count.py` permanece como shim de compatibilidade,
porque ainda aparece no workflow, no fixture histórico `TOKEN-AGENT-01.json` e
em documentação. O teste atual já usa `test_transcript.py`.

Não transformar automaticamente nomes de evidência em `data.json`, `probe.py`
ou `entry.sely`. Fora do diretório, nomes como
`TOKEN-PAIR-01.python.transcript.json`,
`self-hosted-language-model-entry.sely` e
`auditable-local-inference-probe.py` explicam exatamente qual artefato está
sendo observado. A auditoria em `docs/project/filename-audit.md` confirma que
esses caminhos são referenciados por CI, manifests, matrizes e relatórios.

## Comandos

Comandos existentes e claros ficam estáveis: `seloryn inspect` e o grupo
`seloryn job` (`create`, `status`, `list`, `transition`, `claim`, `heartbeat`,
`release`, `recover`, `preflight`) já formam uma gramática curta. Não criar
sinônimos apenas para reduzir uma palavra; cada novo comando deve ter uma
responsabilidade única, saída estruturada quando usado por agentes e erro
determinístico.

Quando um comando precisar ser substituído, a sequência é:

1. adicionar o comando curto como forma canônica;
2. manter o comando antigo como alias por pelo menos uma linha de versão;
3. emitir aviso de depreciação somente fora de saídas estruturadas;
4. atualizar documentação e exemplos novos;
5. remover apenas com entrada no ledger de versão, teste de migração e revisão.

## Builtins da gramática

O código atual confirma, em `src/tensor_autograd.rs`, as operações
`tensor_sum`/`sum`, `tensor_mean`, `matmul`, `transpose`, `tensor_relu`,
`tensor_exp`, `tensor_log`, `tensor_softmax`, `tensor_logsumexp`,
`tensor_cross_entropy`, `tensor_sum_to_shape`, `reshape`, `tensor_f32` e
`tensor_zeros_f32`. Os testes também exercitam `tensor_get_f32`,
`tensor_get_i64`, `tensor_item_f32`, `tensor_to_f32`, `tensor_to_i64`,
`tensor_i64`, `tensor_shape`, `tensor_rank`, `tensor_dtype` e
`tensor_elements`.

O nome atual já é deliberadamente explícito: `tensor_get_f32` comunica domínio,
operação e dtype em uma única leitura. Ele não é um nome a ser encurtado por
estética. As APIs maduras usadas como referência também mantêm o contexto no
objeto/namespace (`torch.Tensor.item`, `torch.tensor`, `numpy.ndarray`), e as
diretrizes de Rust recomendam `snake_case`, palavras não ambíguas e evitar
abreviações que reduzam clareza. Como a gramática atual ainda não possui um
namespace `tensor.<op>`, o prefixo `tensor_` permanece a forma segura.

Portanto, a regra desta versão é conservadora: não criar aliases tensor específicos
quando eles removem `tensor`, o dtype ou uma parte da operação. Aliases já
existentes e inequívocos, como `sum` para `tensor_sum` e `matmul`/`reshape`,
permanecem por compatibilidade; aliases novos devem passar por revisão própria.
O contrato também é protegido por um teste negativo em
`tests/tensor_cpu.rs`: `ten_get_f32`, `getf32` e `tensor_getr_f32` devem
continuar sendo rejeitados como nomes indefinidos.

| Nome atual observado | Decisão nesta versão | Motivo | Status |
| --- | --- | --- | ---: |
| `tensor_get_f32` | manter o nome completo | `getf32` e `ten_get_f32` perdem leitura imediata do domínio; `tensor_getr_f32` não existe | Verified |
| `tensor_get_i64` | manter o nome completo | dtype faz parte do contrato de retorno | Verified |
| `tensor_item_f32` | manter o nome completo | distingue item escalar de tensor e preserva dtype | Verified |
| `tensor_to_f32` / `tensor_to_i64` | manter o nome completo | conversão tipada explícita | Verified |
| `tensor_zeros_f32` / `tensor_f32` | manter o nome completo | construtor e dtype são parte da segurança | Verified |
| `tensor_sum` | `sum` já existente; não adicionar outro alias | palavra curta é inequívoca no contexto da gramática atual | Verified |
| `tensor_mean`, `tensor_relu`, `tensor_exp`, `tensor_log` | avaliar somente aliases sem conflito | operações podem usar nomes idiomáticos, mas colisões com funções do usuário precisam ser medidas | Proposed |
| `tensor_softmax`, `tensor_logsumexp`, `tensor_cross_entropy` | manter o nome completo | `lse`/`xent` exigem dicionário mental e não são universais | Verified |
| `tensor_sum_to_shape` | manter o nome completo | `shape` é informação semântica necessária | Verified |
| `tensor_shape`, `tensor_rank`, `tensor_dtype`, `tensor_elements` | manter o nome completo | metadados não devem parecer operações escalares genéricas | Verified |

Regras adicionais para aliases:

- aliases curtos só podem ser habilitados quando o parser, a resolução
  semântica, o runtime e os testes negativos os tratarem como o mesmo builtin;
- não usar abreviações opacas (`g32`, `z`, `op1`, `getf32`, `zerosf32`, `lse`,
  `xent`) quando a forma completa carrega informação de domínio ou tipo;
- se um futuro namespace `tensor.<op>` for implementado, ele poderá reduzir a
  repetição sem apagar a semântica. Até lá, `tensor_get_f32` é preferível a
  `ten_get_f32`;
- nomes explícitos já publicados, como `tensor_get_f32`, permanecem canônicos;
  qualquer alias curto é apenas uma compatibilidade opt-in e sua criação ou
  depreciação exige aviso, teste e nota de migração;
- o nome curto não muda o contrato de tipo, device, layout, memória ou erro;
  nomes não podem esconder limites de GPU ou de execução automática.

## Tabela de migração priorizada

| Ordem | Escopo | Ação | Critério de aceite | Compatibilidade |
| ---: | --- | --- | --- | --- |
| P0 | Branch | Preferir `proof/tokens` em novas provas e preservar `master` | branch curta, PR/SHAs intactos, sem force-push | manter head antigo se a troca ameaçar o PR |
| P0 | Transcript | Usar `transcript.py` em integrações novas | CLI, testes e CI passam com o canonical | manter `transcript_token_count.py` como shim |
| P0 | Tensor index/zero | manter `tensor_get_f32`, `tensor_get_i64`, `tensor_zeros_f32` | contratos existentes e nomes autoexplicativos | sem alias opaco |
| P1 | Tensor construção/conversão | manter `tensor_f32`, `tensor_i64`, `tensor_item_f32`, `tensor_to_f32`, `tensor_to_i64` | dtype, shape, overflow e conversões preservados | sem alias opaco |
| P1 | Operações de modelo | avaliar aliases apenas quando forem palavras completas e sem colisão | referência numérica e testes de shape/axis | formas longas permanecem |
| P2 | Namespace futuro | estudar `tensor.<op>` na gramática antes de abreviar | reduzir repetição sem perder domínio/tipo | mudança de linguagem versionada |
| P2 | Projetos/evidências | Não renomear automaticamente | manifest, glob, CI e links continuam estáveis | usar mapa de migração, nunca apagar histórico |

Cada linha deve ser implementada em uma mudança isolada, com teste
independente e revisão por alguém que não escreveu o alias. O nome curto só
entra na documentação de uso depois que a execução real o comprovar.

## Quando não renomear

Manter o nome original quando qualquer uma destas condições for verdadeira:

- é um ID de tarefa, experimento, release, plataforma ou fixture congelado;
- o nome é parte de um schema, manifest, glob de descoberta ou contrato de CI;
- o arquivo é uma evidência que precisa ser identificável fora do diretório;
- a renomeação alteraria links históricos, comandos reproduzíveis ou hashes;
- o encurtamento exigiria um dicionário mental maior que o nome original;
- há mais de uma interpretação plausível para o alias curto;
- não existe teste de compatibilidade e rollback simples;
- o ganho é apenas estético e não reduz erro de integração ou descoberta.

Nestes casos, registrar a decisão em uma auditoria ou mapa de migração é a
ação correta. Comprimento, sozinho, não é defeito.

## Checklist executável para revisão

- [ ] O nome segue a forma canônica da superfície?
- [ ] O domínio já torna o prefixo redundante?
- [ ] O nome é claro em um log sem o diretório?
- [ ] `rg` confirmou todas as referências antes da troca?
- [ ] O nome não é uma identidade histórica ou de evidência?
- [ ] Existe alias/shim para consumidores antigos?
- [ ] Há teste positivo, negativo e de compatibilidade?
- [ ] A documentação declara se o comportamento é `Verified`, `Experimental`,
      `Partial`, `Proposed`, `Blocked` ou `Unsupported`?
- [ ] A mudança não altera versão, release ou branch protegida sem o fluxo
      correspondente?

Uma política de nomes está concluída somente quando o nome curto torna a
integração mais fácil e a prova continua reproduzível. Caso contrário, o nome
longo, semanticamente preciso, é a decisão mais segura.
