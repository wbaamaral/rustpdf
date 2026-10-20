# RustPDF

> Editor de PDF nativo para desktop com interface GTK 3 e CLI, escrito em Rust.
> Permite manipulação de páginas, anotações, formulários AcroForm, edição de texto e imagens, OCR pesquisável, criptografia preservada e exportação rica para suítes de escritório (Word, Excel, PowerPoint, LibreOffice Writer/Draw, Markdown e CSV).

[![Licença: AGPL v3](https://img.shields.io/badge/Licen%C3%A7a-AGPL--3.0--or--later-blue.svg)](LICENSE)
[![Linguagem: Rust](https://img.shields.io/badge/Rust-1.80%2B-orange.svg)](https://www.rust-lang.org/)
[![Interface: GTK 3](https://img.shields.io/badge/GUI-GTK%203-green.svg)](https://www.gtk.org/)
[![Plataformas](https://img.shields.io/badge/Plataformas-Linux%20%7C%20Windows-lightgrey.svg)](#pacotes-linux-e-windows)

---

## 🚀 Recursos Principais

### 📄 Gestão Completa de Páginas
- Criar novos documentos em branco ou abrir PDFs existentes.
- Inserir, duplicar, reordenar, girar, recortar e excluir páginas.
- Mesclar e anexar páginas de múltiplos documentos.
- Painel lateral com miniaturas e navegação rápida entre páginas.

### ✍️ Edição Direta de Conteúdo
- **Substituição de Texto**: clique duplo para alterar textos existentes (operadores `Tj`/`TJ`), inclusive dentro de Form XObjects compartilhados.
- **Movimentação por Arraste**: selecione e reposicione blocos de texto ou imagens diretamente com o mouse.
- **Gestão de Imagens**: inserir imagens PNG/JPEG por coordenadas, substituir, redimensionar, girar ou remover ocorrências individuais.
- **Transformação de Objetos**: alças visuais para mover, rotacionar e redimensionar objetos com preservação geométrica.

### 📝 Formulários (AcroForm) e Anotações
- Criação e preenchimento de campos de texto (inclusive multilinha), caixas de seleção (*checkbox*), botões de rádio e listas/combos de escolha.
- Inserção de notas adesivas e destaques de texto por coordenadas ou por seleção com o mouse.

### 🔍 OCR Pesquisável Integrado
- Reconhecimento de texto via Tesseract em páginas digitalizadas.
- O modo **OCR Pesquisável** adiciona uma camada de texto invisível sobre o documento original, tornando-o indexável e selecionável sem alterar os pixels renderizados.

### 📤 Exportação Estruturada Multiformato
Exportação nativa a partir da interface gráfica ou via CLI:

| Formato | Tipo | Características Principais |
| --- | --- | --- |
| **Word (`.docx`)** | Documento | Títulos estilizados, listas numeradas, tabelas estruturadas e paginação configurável (`--quebrar-paginas`). |
| **Writer (`.odt`)** | Documento | Formato OpenDocument Text nativo para LibreOffice Writer. |
| **PowerPoint (`.pptx`)** | Apresentação | Um slide por página com caixas de texto editáveis ou modo fiel (`--fiel`) com imagem da página. |
| **Draw (`.odg`)** | Desenho Vetorial | **Vetorização nativa** para LibreOffice Draw com linhas, formas e caixas de texto editáveis. |
| **Excel (`.xlsx`)** | Planilha | Extração inteligente de tabelas com uma aba por tabela e preservação de valores numéricos/monetários. |
| **CSV (`.csv`)** | Dados | Exportação de tabelas tabulares com delimitador `;` e BOM UTF-8 compatível com planilhas. |
| **Markdown (`.md`)** | Texto Estruturado | Cabeçalhos hierárquicos, parágrafos, listas, tabelas GFM e modo `--para-ia` otimizado para consumo por LLMs. |

### 🔒 Segurança e Criptografia
- Abertura de documentos protegidos por senha de usuário ou proprietário.
- Preservação estrita das permissões e chaves criptográficas ao salvar (`qpdf --copy-encryption`).
- Fornecimento seguro de senha pela entrada padrão (`--password-stdin`) na CLI, sem expor credenciais em listas de processos.

---

## 📦 Instalação e Execução

### Pré-requisitos do Sistema
- **Compilador**: Rust (edição 2021)
- **Bibliotecas**: GTK 3 (`libgtk-3-dev`), qpdf (`libqpdf-dev`), zlib (`zlib1g-dev`), libjpeg-turbo (`libturbojpeg0-dev`)
- **Ferramentas de Suporte**: Poppler (`pdfinfo`, `pdftoppm`, `pdftotext`), Tesseract OCR e descompactador `unzip`

### Compilando a partir do Código-Fonte
```bash
# Compilar versão otimizada
cargo build --release --offline

# Executar a interface gráfica
./target/release/rustpdf [arquivo.pdf]
```

### Pacotes Prontos (Linux e Windows)
O script `packaging/package.sh` gera instaladores e pacotes nativos:
- **Linux**:
  - Pacote portátil `.tar.gz` (com script `install.sh`)
  - Arch Linux / Manjaro: `.pkg.tar.zst` (via `PKGBUILD`)
  - Debian / Ubuntu: `.deb`
  - Fedora / RHEL: `.rpm`
- **Windows**:
  - Pasta portátil `.zip` e instalador NSIS (`.exe`) via MSYS2 MINGW64.

Veja detalhes completos em [packaging/README.md](packaging/README.md).

### Binários Pré-compilados para Teste
Executáveis compilados para Linux x86-64 acompanham o repositório em `dist/`:
```bash
./dist/rustpdf-linux-x86_64 documento.pdf
./dist/rustpdf-cli-linux-x86_64 info documento.pdf
sha256sum -c dist/SHA256SUMS
```

---

## 💻 Interface de Linha de Comando (`rustpdf-cli`)

A CLI oferece controle total sobre todas as operações do editor:

### Gestão de Documentos e Páginas
```bash
# Criar documento vazio e anexar páginas
rustpdf-cli new vazio.pdf
rustpdf-cli append vazio.pdf duas-paginas.pdf resultado.pdf

# Recortar margem da página (em pontos)
rustpdf-cli crop documento.pdf recortado.pdf 1 36

# Informações estruturais do PDF
rustpdf-cli info documento.pdf
```

### Anotações e Formulários
```bash
# Inserir nota adesiva por página ou por coordenadas
rustpdf-cli note doc.pdf anotado.pdf 2 "Revisar este capítulo"
rustpdf-cli note-at doc.pdf nota-local.pdf 1 "Observação importante" 120 320

# Destacar área da página (X Y Largura Altura)
rustpdf-cli highlight-at doc.pdf destacado.pdf 1 120 320 200 20

# Inserir e preencher campos AcroForm
rustpdf-cli text-field doc.pdf form.pdf 1 cliente "Nome do Cliente" 72 700 220 30
rustpdf-cli set-field form.pdf preenchido.pdf cliente "Novo Nome"
rustpdf-cli checkbox form.pdf com-caixa.pdf 1 aceite nao 72 650 24
rustpdf-cli set-checkbox com-caixa.pdf aceito.pdf aceite sim
rustpdf-cli radio form.pdf com-radio.pdf 1 opcao A "A,B,C" 72 600 24 30
rustpdf-cli choice form.pdf com-lista.pdf 1 estado Aberto "Aberto,Pendente,Fechado" 72 520 220 30 lista
```

### Edição de Conteúdo e Imagens
```bash
# Adicionar novo texto ou imagem
rustpdf-cli insert-text-at doc.pdf texto.pdf 1 "Texto inserido" 120 500 16
rustpdf-cli insert-image-at doc.pdf imagem.pdf 1 logo.png 100 300 200 150

# Substituir texto já existente
rustpdf-cli replace-text doc.pdf alterado.pdf 1 "Texto Antigo" "Texto Novo"

# Mover, substituir ou remover imagens existentes por ocorrência
rustpdf-cli move-image doc.pdf movido.pdf 1 Im1 10 -5
rustpdf-cli replace-image doc.pdf trocado.pdf 1 Im1 nova.png todas
rustpdf-cli remove-image doc.pdf limpo.pdf 1 Im1
```

### Transformação Geométrica de Objetos
```bash
# Consultar propriedades do objeto sob um ponto (X Y)
rustpdf-cli select doc.pdf 1 175 560

# Mover, redimensionar e girar objetos
rustpdf-cli move-object doc.pdf movido.pdf 1 175 560 30 -20
rustpdf-cli resize-object doc.pdf ampliado.pdf 1 175 560 2.0 1.5
rustpdf-cli rotate-object doc.pdf girado.pdf 1 175 560 30
```

### Exportação Multiformato
```bash
# Exportar documento para suítes de escritório
rustpdf-cli export doc.pdf saida.docx                              # Microsoft Word
rustpdf-cli export doc.pdf saida.odt                               # LibreOffice Writer
rustpdf-cli export doc.pdf saida.pptx                              # PowerPoint (editável)
rustpdf-cli export doc.pdf saida.pptx --fiel                       # PowerPoint (página como imagem)
rustpdf-cli export doc.pdf saida.odg                               # LibreOffice Draw (vetorial)

# Exportar dados tabulares
rustpdf-cli export doc.pdf tabelas.xlsx                            # Planilha Excel
rustpdf-cli export doc.pdf tabelas.csv --separador ';'             # Tabelas em CSV

# Exportar para Markdown (geral e otimizado para IA)
rustpdf-cli export doc.pdf documento.md                            # Markdown semântico
rustpdf-cli export doc.pdf documento.md --para-ia                  # Otimizado para LLMs e agentes

# Opções de intervalo de páginas e OCR para documentos escaneados
rustpdf-cli export doc.pdf parcial.docx --paginas 1-3,7
rustpdf-cli export escaneado.pdf texto.md --ocr por
```

### Documentos Protegidos com Senha
```bash
# Ler e operar com senha via stdin com segurança
printf '%s\n' "$SENHA" | rustpdf-cli --password-stdin info protegido.pdf
printf '%s\n' "$SENHA" | rustpdf-cli --password-stdin insert-text protegido.pdf out.pdf 1 "Texto"
```

---

## ⌨️ Atalhos Principais da Interface

| Tecla | Ação |
| --- | --- |
| **Ctrl + O** | Abrir documento PDF |
| **Ctrl + S** | Salvar cópia do documento |
| **Ctrl + Z** / **Ctrl + Shift + Z** | Desfazer / Refazer (até 32 estados) |
| **Ctrl + +** / **Ctrl + -** | Ampliar / Reduzir zoom |
| **Ctrl + 0** | Ajustar largura à janela |
| **Ctrl + 1** | Ajustar página inteira |
| **Home** / **End** | Ir para a primeira / última página |
| **Page Up** / **Page Down** | Ir para a página anterior / seguinte |
| **F1** | Exibir lista completa de atalhos e ajuda |

---

## 🛠️ Diagnóstico de Desempenho

`RUSTPDF_TRACE=1 rustpdf documento.pdf` imprime no terminal a duração exata de cada ação, renderização, chamadas externas (`pdftoppm`, `pdfinfo`, `qpdf`) e operações de E/S. As miniaturas são geradas em segundo plano e exclusivamente para páginas modificadas.

---

## 🔬 Qualidade e Engenharia de Software

O desenvolvimento segue princípios de código limpo e testes contínuos:

- **Arquitetura em Camadas**: separação estrita entre `dominio` (regras puras sem efeitos colaterais), `app` (casos de uso e portas), `infra` (adaptadores para processos e bibliotecas do sistema) e `apresentacao` (lógica desacoplada da GUI).
- **Testes Automatizados**: testes de unidade puros, testes de contrato comparando dublês com ferramentas reais (Poppler, Tesseract, QPDF) e testes de integração de ponta a ponta na CLI.
- **Catraca de Cobertura**: aferição contínua via `tools/cobertura.py` com metas mínimas por camada (`dominio` ≥ 90%, `app`/`apresentacao` ≥ 85%) e cobertura global monitorada.
- **Integração Contínua**: workflows automatizados no GitHub Actions validando compilação, formatação, clippy e cobertura em Ubuntu, Arch Linux, Fedora e Windows.

---

## ⚖️ Licença

Copyright (C) 2026 Welyqrson Bastos Amaral.

Este programa é software livre: você pode redistribuí-lo e/ou modificá-lo sob os termos da **GNU Affero General Public License** (`AGPL-3.0-or-later`), conforme publicada pela Free Software Foundation, versão 3 da licença ou (a seu critério) qualquer versão posterior.

O RustPDF é distribuído na expectativa de que seja útil, mas **SEM NENHUMA GARANTIA**; sem mesmo a garantia implícita de COMERCIABILIDADE ou ADEQUAÇÃO A UM DETERMINADO PROPÓSITO. Consulte o arquivo [LICENSE](LICENSE) para obter mais detalhes.

Informações detalhadas sobre componentes de terceiros, bibliotecas de sistema utilizadas e declarações de licenças estão disponíveis em [TERCEIROS.md](TERCEIROS.md). Código-fonte público: <https://github.com/wbaamaral/rustpdf>.
