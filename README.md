# RustPDF

Editor de PDF para desktop, escrito em Rust, com interface GTK 3 e CLI: páginas, anotações, formulários AcroForm, edição do texto e das imagens já presentes na página, OCR e criptografia preservada ao salvar. Licença AGPL-3.0-or-later.

## Compilar e executar

Requer Rust, GTK 3, qpdf com `libqpdf`, Poppler (`pdfinfo`, `pdftoppm`, `pdftotext`), Tesseract, zlib e libjpeg-turbo instalados no sistema.

```bash
cargo build --release --offline
./target/release/rustpdf /caminho/documento.pdf
```

O argumento PDF é opcional. A interface tem barra de menus com mnemônicos e atalhos (Arquivo, Editar, Exibir, Documento, Comentários, Formulários, Ferramentas, Ajuda — **F1** lista os atalhos), duas barras planas com ícones simbólicos próprios e dicas, ferramentas exclusivas espelhadas no menu Ferramentas, navegação de páginas (primeira/anterior/página/seguinte/última), zoom real com ajuste à página/largura e Ctrl + roda do mouse, painel lateral com miniaturas das páginas e lista de comentários, e barra de status com página, formato do papel e zoom. Permite criar um documento, abrir e salvar como, navegar e ampliar páginas, girar, mover, excluir, recortar ou inserir páginas em branco, adicionar páginas de outro PDF, inserir notas, destacar áreas por coordenadas ou **arrastando o mouse sobre a página**, adicionar texto visível e imagens PNG/JPEG com posição definida em pontos PDF, **substituir texto já desenhado** (clique duplo) ou **movê-lo arrastando o mouse** (também `move-text` e `edit-text-at` na CLI) — só o trecho clicado muda, mesmo dentro de Form XObjects compartilhados, **substituir, mover ou remover imagens já presentes**, criar, listar e preencher campos de texto (inclusive multilinha), caixas de seleção, grupos de rádio e campos de escolha (combo ou lista) AcroForm, extrair páginas, exportar PNG, copiar texto extraído e executar OCR da página. PDFs protegidos podem ser abertos com senha. O comando **OCR pesquisável** adiciona uma camada de texto invisível à página, preservando sua aparência. Desfazer/refazer preserva até 32 estados. As alterações ficam em uma cópia temporária até **Salvar como**.

As coordenadas X e Y partem do canto inferior esquerdo da área visível. A aplicação considera recorte e rotação da página ao colocar texto, notas, destaques e campos.

## Licença

Copyright (C) 2026 Welyqrson Bastos Amaral.

O RustPDF é software livre: você pode redistribuí-lo e/ou modificá-lo sob os termos da **GNU Affero General Public License**, conforme publicada pela Free Software Foundation, versão 3 da Licença ou (a seu critério) qualquer versão posterior (`AGPL-3.0-or-later`). Ele é distribuído na esperança de ser útil, mas **sem nenhuma garantia**; veja o arquivo [LICENSE](LICENSE). Código-fonte: <https://github.com/wbaamaral/rustpdf>.

Componentes de terceiros, licenças e a regra de integração com o Poppler estão em [TERCEIROS.md](TERCEIROS.md).

## Pacotes (Linux e Windows)

`packaging/package.sh linux` gera o `.tar.gz` portátil (com `install.sh`), o pacote Arch/Manjaro (`PKGBUILD` + `makepkg`), o `.rpm` e, onde houver `dpkg-deb`, o `.deb`. `packaging/package.sh windows`, no MSYS2 MINGW64, gera a pasta portátil `.zip` com GTK, Poppler, qpdf e Tesseract embutidos e o instalador NSIS. Detalhes, dependências e limitações em [packaging/README.md](packaging/README.md).

## Diagnóstico de lentidão

`RUSTPDF_TRACE=1 rustpdf documento.pdf` imprime no terminal quanto dura cada ação, render, processo externo (`pdftoppm`, `pdfinfo`, `qpdf`) e leitura/gravação do PDF. As miniaturas são geradas em segundo plano e só para as páginas alteradas.

## Binários para teste

Os executáveis compilados para Linux x86-64 estão em `dist/`. Na máquina de compilação, execute:

```bash
./dist/rustpdf-linux-x86_64 /caminho/documento.pdf
./dist/rustpdf-cli-linux-x86_64 info /caminho/documento.pdf
sha256sum -c dist/SHA256SUMS
```

O binário da interface usa bibliotecas compartilhadas GTK 3, libqpdf, zlib e libjpeg-turbo. As funções de visualização, extração e OCR também chamam `pdfinfo`, `pdftoppm`, `pdftotext` e `tesseract` instalados no sistema. Esta compilação foi validada no ambiente local; outras distribuições podem precisar de versões compatíveis dessas bibliotecas.

Para uso sem interface gráfica:

```bash
./target/release/rustpdf-cli new vazio.pdf
./target/release/rustpdf-cli append vazio.pdf duas-paginas.pdf vazio.pdf
./target/release/rustpdf-cli note duas-paginas.pdf anotado.pdf 2 "Revisar esta página"
./target/release/rustpdf-cli note-at anotado.pdf nota-local.pdf 1 "Revisar aqui" 120 320
./target/release/rustpdf-cli highlight-at anotado.pdf destacado.pdf 1 120 320 200 20
./target/release/rustpdf-cli insert-text anotado.pdf texto.pdf 1 "Texto adicionado"
./target/release/rustpdf-cli insert-text-at texto.pdf texto-local.pdf 1 "Outra posição" 120 500 16
./target/release/rustpdf-cli crop texto.pdf recortado.pdf 1 36
./target/release/rustpdf-cli insert-image recortado.pdf com-imagem.pdf 1 foto.jpg
./target/release/rustpdf-cli insert-image-at recortado.pdf imagem-local.pdf 1 foto.jpg 100 300 200 150
./target/release/rustpdf-cli ocr-searchable digitalizado.pdf pesquisavel.pdf 1 por+eng
./target/release/rustpdf-cli text-field pesquisavel.pdf formulario.pdf 1 cliente "Ana Maria" 72 700 220 30
./target/release/rustpdf-cli set-field formulario.pdf preenchido.pdf cliente Beatriz
./target/release/rustpdf-cli checkbox preenchido.pdf com-caixa.pdf 1 aceite nao 72 650 24
./target/release/rustpdf-cli set-checkbox com-caixa.pdf aceito.pdf aceite sim
./target/release/rustpdf-cli radio aceito.pdf com-radio.pdf 1 cor Azul "Azul,Verde,Vermelho" 72 600 24 30
./target/release/rustpdf-cli set-radio com-radio.pdf cor-escolhida.pdf cor Verde
./target/release/rustpdf-cli choice cor-escolhida.pdf com-lista.pdf 1 estado Aberto "Aberto,Fechado,Pendente" 72 520 220 30 lista
./target/release/rustpdf-cli set-choice com-lista.pdf final.pdf estado Pendente
./target/release/rustpdf-cli text-field final.pdf com-multi.pdf 1 obs "Linha 1\nLinha 2" 72 450 220 50 multiline
./target/release/rustpdf-cli fields final.pdf
./target/release/rustpdf-cli text-runs final.pdf 1
./target/release/rustpdf-cli replace-text final.pdf editado.pdf 1 "Linha 1" "Outra linha"
./target/release/rustpdf-cli images editado.pdf 1
./target/release/rustpdf-cli image-draws editado.pdf 1                          # NOME, ocorrência, caixa, ângulo
./target/release/rustpdf-cli move-image editado.pdf movida.pdf 1 Im1 10 -5      # única ocorrência
./target/release/rustpdf-cli move-image editado.pdf movida.pdf 1 Im1 10 -5 2    # só a 2ª (ou: todas)
./target/release/rustpdf-cli replace-image movida.pdf trocada.pdf 1 Im1 nova.png todas
./target/release/rustpdf-cli replace-image-at movida.pdf trocada.pdf 1 60 45 nova.png
./target/release/rustpdf-cli remove-image-at trocada.pdf sem-imagem.pdf 1 60 225
./target/release/rustpdf-cli remove-image movida.pdf sem-imagem.pdf 1 Im1
./target/release/rustpdf-cli info anotado.pdf
```

Execute `rustpdf-cli` sem argumentos para a lista completa de comandos.

Operações de imagem agem em **ocorrências** (cada desenho `/Nome Do`), nunca no nome inteiro por engano: por nome, se a imagem é desenhada mais de uma vez na página, informe `N` ou `todas` (sem isso o comando recusa e diz quantas há); por ponto, vale a imagem sob X Y. Substituir cria um XObject novo (ex.: `Im1_c1`) só para os desenhos escolhidos — as outras ocorrências e páginas mantêm a original; remover tira o recurso apenas quando nada mais o usa.

### Objetos na página: selecionar, mover, redimensionar e girar

Os mesmos recursos das alças da interface, com o objeto identificado por um ponto (X Y em pontos visíveis da página). A ordem de busca é a da interface: anotação/campo, imagem e texto.

```bash
./target/release/rustpdf-cli select doc.pdf 1 175 560                         # tipo, moldura, caixa, ângulo, capacidades
./target/release/rustpdf-cli move-object doc.pdf movido.pdf 1 175 560 30 -20  # desloca DX DY
./target/release/rustpdf-cli resize-object doc.pdf maior.pdf 1 175 560 2 1.5  # escala nos eixos do objeto, em torno do centro
./target/release/rustpdf-cli rotate-object doc.pdf girado.pdf 1 175 560 30    # graus, anti-horário, em torno do centro
./target/release/rustpdf-cli transform-object doc.pdf t.pdf 1 175 560 1 0 0 1 10 0  # matriz afim A B C D E F (espaço da página)
```

`select` imprime uma propriedade por linha (`chave<TAB>valor`): `tipo` (`imagem:Im1`, `texto`, `anotacao:nota#0`…), `rotulo`, `origem`, `eixo_u`, `eixo_v`, `caixa`, `largura`, `altura`, `angulo`, `redimensionavel`, `giravel` e `proporcional`. Cada comando de transformação imprime a nova `caixa`. Só a ocorrência apontada muda: Form XObjects compartilhados são copiados (copy-on-write). As capacidades valem também na CLI: nota só move; destaque e campo não giram; texto só escala proporcionalmente; cisalhamento e espelhamento são recusados.

Para um PDF protegido, a interface pede a senha ao abrir. A CLI pode recebê-la pela entrada padrão, sem incluí-la nos argumentos do processo:

```bash
read -rsp 'Senha: ' RUSTPDF_INPUT_PASSWORD
printf '%s\n' "$RUSTPDF_INPUT_PASSWORD" | ./target/release/rustpdf-cli --password-stdin info protegido.pdf
printf '%s\n' "$RUSTPDF_INPUT_PASSWORD" | ./target/release/rustpdf-cli --password-stdin --allow-unencrypted-output insert-text protegido.pdf copia-sem-senha.pdf 1 "Novo texto"
unset RUSTPDF_INPUT_PASSWORD
```

A cópia de trabalho é descriptografada em uma pasta temporária privada. Salvar e extrair páginas mantêm a criptografia do original — mesmas senhas de usuário e proprietário, permissões e algoritmo (`qpdf --copy-encryption`; a senha é passada por arquivo de argumentos privado, não pela linha de comando). Gerar uma cópia sem senha exige confirmação explícita na interface ou `--allow-unencrypted-output` na CLI, e um PDF criptografado existente nunca é substituído por uma cópia sem senha.

## Verificação

`cargo test --offline` roda os testes da biblioteca e `tests/cli.rs`, que executa o binário `rustpdf-cli` (selecionar, mover, redimensionar, girar, recusas por capacidade e `qpdf --check`). Os testes usam qpdf, Poppler e Tesseract em PDFs reais para verificar criação, inserção, movimentação, rotação, exclusão, recorte, anotação, destaque, texto visível, imagem, posicionamento, OCR pesquisável, campos de formulário, senhas, histórico, salvamento e renderização. Há também um teste de edição de conteúdo já existente: listagem e substituição de texto mostrado (inclusive dentro de Form XObjects), listagem/substituição/movimentação/remoção de imagens, `qpdf --check` e desfazer. O teste de destaque confirma a anotação `/Highlight` com `/QuadPoints`, mudança nos pixels renderizados e desfazer. O teste de OCR confirma que o texto se torna extraível e que os pixels renderizados permanecem idênticos. Os testes de formulário confirmam AcroForm, valores de texto atualizados, caixa marcada e desmarcada com aparências distintas, listagem e desfazer/refazer. A posição da nota é conferida na estrutura JSON de qpdf; a posição do texto é conferida por `pdftotext -bbox`, e imagens em posições diferentes produzem renderizações diferentes. Há ainda testes de coordenadas em páginas giradas e recortadas e de abertura com senha, incluindo bloqueio de salvamento sem consentimento explícito. `cargo clippy --offline --all-targets -- -D warnings` passa. Também foram executados fluxos pela CLI, seguidos de `qpdf --check`.

A interface GTK foi aberta na sessão gráfica `:0` e exercitada com um PDF de 3 páginas: menu, barras de ferramentas, navegação pela barra lateral (Página 1/2/3 com destaque da página atual), campo de página e barra de status. O fluxo de destaque com o mouse também foi exercitado: ativar **Destaque com mouse**, arrastar sobre a página, conferir a anotação renderizada e usar **Desfazer**. Os demais diálogos e o comportamento em outras resoluções ainda merecem prova manual.

## Cobertura atual

A aplicação ainda não reproduz edição tipográfica completa de caixas de texto existentes (reposicionar/estilizar glifos), assinaturas digitais, carimbos nem PDF/A. A edição de conteúdo cobre **substituição de texto mostrado** (operadores `Tj`/`TJ`) em conteúdo da página e em Form XObjects de overlay, **mover, redimensionar e girar** imagens e trechos de texto pela interface ou pela CLI (só a ocorrência selecionada) e **substituição/remoção de imagens** raster (XObject `/Image`), também por ocorrência. Texto convertido em curvas, strings em CID/fontes embutidas complexas e texto dividido em muitos `TJ` com kerning ainda não são editáveis. O OCR comum retorna texto em uma janela; o OCR pesquisável processa somente a página atual, exige os idiomas solicitados instalados no Tesseract e usa Helvetica com substituição por `?` para caracteres não representáveis em WinAnsi. Campos de formulário cobrem texto (inclusive multilinha), caixa de seleção, grupo de rádio e escolha combo/lista; todos são widgets AcroForm com nome e posição em pontos PDF. A listagem e o preenchimento programático atendem campos diretos na lista principal do AcroForm; campos hierárquicos ou com widget separado ainda não são atendidos. A aparência do texto usa Helvetica e pode substituir caracteres fora de WinAnsi por `?`. As notas e os destaques são anotações PDF reais, posicionadas por coordenadas. O destaque pode ser criado por coordenadas ou por arraste do mouse sobre a prévia; a seleção é retangular e ainda não acompanha glifos de texto. O texto visível usa Helvetica e aceita somente caracteres Latin-1 nesta versão. Imagens PNG/JPEG podem ser posicionadas e dimensionadas; a inserção simples ainda as centraliza em até 70% da página. PNGs precisam ter 8 bits por canal e não podem ser entrelaçados.
