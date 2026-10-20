# Componentes de terceiros

O RustPDF é distribuído sob a **GNU Affero General Public License v3.0 ou
posterior** (`AGPL-3.0-or-later`, arquivo [LICENSE](LICENSE)). O código do
crate não tem dependências de terceiros além da biblioteca padrão do Rust.
Em execução, ele usa os componentes abaixo, todos compatíveis com essa
distribuição.

## Bibliotecas ligadas dinamicamente

| Componente | Licença | Uso |
| --- | --- | --- |
| GTK 3, GLib/GObject, GDK, gdk-pixbuf, Pango | LGPL-2.0/2.1 ou posterior | interface |
| Cairo | LGPL-2.1 ou MPL-1.1 | desenho da seleção |
| qpdf (`libqpdf`) | Apache-2.0 (ou Artistic-2.0) | leitura e escrita de PDF |
| zlib | Zlib | compressão de imagens |
| libjpeg-turbo (`libturbojpeg`) | BSD-3-Clause e IJG | leitura de JPEG — *este software é baseado em parte no trabalho do Independent JPEG Group* |
| Biblioteca padrão do Rust | MIT ou Apache-2.0 | compilada no executável |

## Programas executados como processos separados

Não são ligados ao RustPDF: são chamados pela linha de comando, com troca de
arquivos e saída de texto (agregação).

| Programa | Licença | Uso |
| --- | --- | --- |
| Poppler (`pdftoppm`, `pdftotext`, `pdfinfo`) | GPL-2.0-only / GPL-3.0-or-later (por arquivo) | prévia, miniaturas, texto, geometria |
| qpdf (`qpdf`) | Apache-2.0 | rotação, junção, criptografia |
| Tesseract + Leptonica | Apache-2.0 / BSD-2-Clause | OCR (opcional) |
| OpenSSL (`openssl`) | Apache-2.0 | assinatura digital (em desenvolvimento) |

> Partes do Poppler são **GPL-2.0-only**, incompatível com a AGPL-3.0 numa
> obra combinada. Por isso o RustPDF **não deve ligar** com `libpoppler`:
> a integração continua por processos separados.

## Pacotes

- **Linux** (`.tar.gz`, Arch, `.rpm`, `.deb`): nenhum componente de terceiros
  é embutido; as dependências vêm da distribuição.
- **Windows** (`.zip`, instalador): embute GTK, Poppler, qpdf, Tesseract e as
  DLLs de que dependem, todos dos pacotes do MSYS2. Os textos das licenças
  ficam em `share/licenses/` e o código-fonte correspondente está nas receitas
  do MSYS2 (<https://github.com/msys2/MINGW-packages>) e nos projetos de
  origem de cada componente.
