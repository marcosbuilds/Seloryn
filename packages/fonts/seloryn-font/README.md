# seloryn-font / v2.0

Experimental package candidate for the Seloryn brand family. The supplied
archive is preserved privately with its source hash; this directory is the
public package source and is not a stable GitHub release.

Fonte personalizada para a marca [Seloryn](https://github.com/marcosbuilds/Seloryn), derivada visualmente do `assets/brand/seloryn-symbol.svg` oficial. **Idioma: inglês + português do Brasil**. O símbolo local deste pacote acompanha a cópia canônica do repositório.

## Conceito revisado

A primeira versão tinha muitos anéis e hastes convencionais. A v2 utiliza um vocabulário formal recorrente: **dois movimentos de curva, sem conexões artificiais; saídas horizontais; terminais redondos; e aberturas intencionais nas letras de bojo**. Por exemplo: `a`, `b`, `d`, `e`, `g`, `o`, `p`, `q` usam bojos deliberadamente abertos; `n`, `h`, `m`, `r` usam pistas independentes; `O`, `Q` e `0` repetem a construção de dois fluxos. A fonte mantém peso consistente por variante, mas curvas e aberturas não são copiadas cegamente de um único glifo.

- **Light 300**: traço 55/1000 em; arejado para frases/títulos.
- **Bold 700**: traço 113/1000 em; assinatura, cartazes, headings.
- **138 caracteres**: ASCII legível para EN + PT-BR, mais `Á À Â Ã É Ê Í Ó Ô Õ Ú Ç` e equivalentes minúsculos, `ª º`, `€ £`, aspas tipográficas e pontuação de texto/código.
- **Formato proporcional, não monoespaçado**: ideal para identidade, títulos, páginas e materiais do projeto. Para código extenso ou alinhamento de colunas, recomendamos uma monoespaçada complementar, na aplicação de demonstração, o código propriamente dito usa uma fonte monoespaçada de apoio.

## Arquivos

```
seloryn-font/
├─ fonts/
│  ├─ seloryn-font-light.ttf
│  ├─ seloryn-font-bold.ttf
│  ├─ seloryn-font-light.woff
│  ├─ seloryn-font-bold.woff
│  ├─ seloryn-font-light.woff2
│  └─ seloryn-font-bold.woff2
├─ previews/
│  ├─ seloryn-font-specimen.png
│  ├─ seloryn-font-real-use.png
│  └─ seloryn-font-wordmark-{light,bold}.svg
├─ examples/
│  └─ tensor-matmul.sely           # cópia literal do exemplo publicado no GitHub
├─ source/
│  ├─ build_seloryn_font.py
│  └─ requirements.txt
├─ seloryn-font.css
├─ seloryn-symbol.svg            # símbolo oficial, referência (duas cores)
├─ index.html                    # laboratório de fonte + exemplo Seloryn real
├─ LICENSE
└─ README.md
```

## Usar em um website

Copie `fonts/` e `seloryn-font.css` mantendo os caminhos. Adicione no HTML:

```html
<link rel="stylesheet" href="/assets/seloryn-font/seloryn-font.css">
<h1 style="font-family:'Seloryn Font';font-weight:700">Seloryn</h1>
<p style="font-family:'Seloryn Font';font-weight:300">Código verificável.</p>
```

O `index.html` abre offline e demonstra uso real da fonte num conceito de interface com [código real `tensor-matmul.sely`](https://github.com/marcosbuilds/Seloryn/blob/master/examples/tensors/tensor-matmul.sely) — **não representa uma execução feita ou um screenshot do site oficial**. O botão de pesos e o slider deixam testar texto em duas variantes.

## Instalar no Windows

Abra os dois arquivos `.ttf`, clique em **Instalar**, selecione a família **Seloryn Font** nos programas, com estilo Light ou Bold.

## Arquivo gerador

Para regenerar em ambiente Python:

```bash
python -m pip install -r source/requirements.txt
python source/build_seloryn_font.py
```

Os traços estão descritos individualmente por letra, incluindo contornos Bézier; o script gera as curvas vetoriais e monta TrueType/WOFF/WOFF2. Kerning OT básico incluído.

## Licença

MIT, compatível com a licença pública consultada em 28/10/2026. Veja `LICENSE`.
