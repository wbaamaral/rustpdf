# Empacotamento do RustPDF

`packaging/package.sh` gera os pacotes de Linux e Windows a partir do código
deste diretório. Os pacotes vão para `dist/packages/` (fora do git), com
`SHA256SUMS` de tudo o que estiver lá.

```bash
packaging/package.sh linux                 # todos os formatos disponíveis nesta máquina
packaging/package.sh linux --formats tar   # só o .tar.gz portátil
packaging/package.sh windows               # no MSYS2 MINGW64/UCRT64
packaging/package.sh --help
```

| Opção | Efeito |
| --- | --- |
| `--formats LISTA` | `tar,arch,rpm,deb` (Linux) ou `zip,nsis` (Windows). Sem ela, gera os que a máquina suporta. |
| `--skip-build` | usa os binários já compilados em `target/` |
| `--skip-tests` | não roda `cargo test` (nem o `check()` do makepkg) |
| `--out PASTA` | outro destino |
| `--no-ocr-data` | Windows: não inclui os idiomas do Tesseract |

Variáveis: `RUSTPDF_LICENSE` (identificador SPDX; padrão `LicenseRef-RustPDF`
enquanto o projeto não tem licença definida), `RUSTPDF_URL`,
`RUSTPDF_MAINTAINER`, `RUSTPDF_EMAIL`.

## Linux

| Formato | Ferramenta | Conteúdo |
| --- | --- | --- |
| `rustpdf-<v>-linux-x86_64.tar.gz` | — | `bin/`, `share/` (atalho `.desktop`, ícone, README), `install.sh`, `uninstall.sh` |
| `rustpdf-<v>-1-x86_64.pkg.tar*` + `arch/PKGBUILD` | `makepkg` | pacote Arch/Manjaro compilado do código-fonte (extensão conforme o `PKGEXT` do `makepkg.conf`) |
| `rustpdf-<v>-1.x86_64.rpm` | `rpmbuild` | RPM binário (Fedora/openSUSE) |
| `rustpdf_<v>-1_amd64.deb` | `dpkg-deb` | DEB binário (Debian/Ubuntu) |

O pacote portátil instala para o usuário (`~/.local`) ou, como root, em
`/usr/local` (ou no prefixo dado: `./install.sh /opt/rustpdf`), atualiza o
menu e confere as dependências, sugerindo o comando de instalação da
distribuição.

Dependências de execução: GTK 3, qpdf (`libqpdf`), Poppler (`pdftoppm`,
`pdftotext`, `pdfinfo`), libjpeg-turbo, zlib; opcional Tesseract (OCR).

> Os binários são ligados às bibliotecas da máquina que compila (glibc,
> `libqpdf.so.N`). `.rpm` e `.deb` gerados no Manjaro servem a distribuições
> igualmente recentes; para Debian/Ubuntu/Fedora, rode o script nelas — os
> modelos (`linux/`) já declaram as dependências com os nomes de cada família.
> O `PKGBUILD` em `dist/packages/arch/` compila do código e serve a qualquer
> Arch/Manjaro.

## Windows

O pacote é uma pasta relocável no padrão do MSYS2 — o GTK localiza `share/` e
`lib/` relativos às DLLs:

```
rustpdf-<v>-windows-x86_64/
├── bin/        rustpdf.exe, rustpdf-cli.exe, pdftoppm/pdftotext/pdfinfo/qpdf/tesseract.exe e as DLLs
├── lib/        loaders do gdk-pixbuf (PNG, JPEG, SVG) e loaders.cache relativo
├── share/      esquemas do GLib, ícones Adwaita/hicolor, dados do Poppler e idiomas do Tesseract
├── etc/gtk-3.0/settings.ini
├── rustpdf.ico, LEIA-ME.md
```

Ao iniciar, o RustPDF põe a própria pasta `bin/` na frente do `PATH` (usa as
ferramentas do pacote) e aponta `TESSDATA_PREFIX` para `share/tessdata`.
As DLLs são achadas por `objdump -p` recursivo sobre os executáveis e os
loaders, copiando só as do sysroot (as do sistema — KERNEL32, USER32… — ficam
de fora).

Saídas: `rustpdf-<v>-windows-x86_64.zip` (portátil) e, com NSIS,
`rustpdf-<v>-windows-x86_64-setup.exe` (menu Iniciar, atalho opcional na área
de trabalho, associação opcional com `.pdf`, desinstalador e entrada em
"Aplicativos instalados").

### No Windows (MSYS2)

1. Instale o [MSYS2](https://www.msys2.org/) e abra o shell **MINGW64** (ou UCRT64, trocando o prefixo dos pacotes por `mingw-w64-ucrt-x86_64-`).
2. Instale as dependências:

   ```bash
   pacman -S --needed git zip \
     mingw-w64-x86_64-rust mingw-w64-x86_64-gtk3 mingw-w64-x86_64-librsvg \
     mingw-w64-x86_64-qpdf mingw-w64-x86_64-poppler mingw-w64-x86_64-libjpeg-turbo \
     mingw-w64-x86_64-tesseract-ocr mingw-w64-x86_64-tesseract-data-eng mingw-w64-x86_64-tesseract-data-por \
     mingw-w64-x86_64-nsis mingw-w64-x86_64-imagemagick
   ```

3. No diretório `rustpdf/`: `packaging/package.sh windows`.

### Compilação cruzada a partir do Linux

Requer o alvo Rust `x86_64-pc-windows-gnu`, o `x86_64-w64-mingw32-gcc` e uma
árvore `mingw64` do MSYS2 com os pacotes acima (extraídos dos `.pkg.tar.zst`
do MSYS2):

```bash
RUSTPDF_WIN_SYSROOT=/caminho/para/mingw64 packaging/package.sh windows
```

O `makensis` do Linux (pacote `nsis`) gera o instalador; sem ele, sai só o
`.zip`.

## Estado da verificação

- Linux: `.tar.gz`, Arch (makepkg + namcap) e `.rpm` gerados e inspecionados no
  Manjaro; o `.tar.gz` foi instalado num prefixo temporário, executado e
  desinstalado. `.deb` não foi gerado aqui (sem `dpkg-deb`).
- Windows: **não executado** — esta máquina não tem o alvo Windows nem o
  MinGW. O código passou a compilar sem APIs exclusivas do Unix (`cfg(unix)`),
  e a lógica de ferramentas do pacote foi testada no Linux; a primeira geração
  real deve ser feita no MSYS2 e conferida (abrir, renderizar, OCR, salvar).
