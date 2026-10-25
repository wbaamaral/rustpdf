#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Auxiliar de empacotamento do RustPDF para Linux e Windows.
#
#   packaging/package.sh linux   [opções]   # .tar.gz portátil, Arch (.pkg.tar.zst), .rpm, .deb
#   packaging/package.sh windows [opções]   # .zip portátil e instalador NSIS (.exe)
#   packaging/package.sh all     [opções]
#
# Opções:
#   --formats LISTA   formatos separados por vírgula (linux: tar,arch,rpm,deb; windows: zip,nsis).
#                     Padrão: todos os disponíveis nesta máquina.
#   --skip-build      usa os binários já compilados em target/.
#   --skip-tests      não roda cargo test (nem o check() do makepkg).
#   --out PASTA       destino dos pacotes (padrão: dist/packages).
#   --no-ocr-data     Windows: não inclui dados de idioma do Tesseract.
#   -h, --help        esta ajuda.
#
# Windows: rode no shell MSYS2 MINGW64/UCRT64 (com os pacotes listados em
# packaging/README.md) ou, a partir do Linux, defina RUSTPDF_WIN_SYSROOT com
# uma árvore mingw64 do MSYS2 e tenha o alvo x86_64-pc-windows-gnu e o
# x86_64-w64-mingw32-gcc instalados.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PACKAGING="$ROOT/packaging"
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
URL=${RUSTPDF_URL:-"https://github.com/wbaamaral/rustpdf"}
LICENSE_ID=${RUSTPDF_LICENSE:-"AGPL-3.0-or-later"}
MAINTAINER=${RUSTPDF_MAINTAINER:-"$(git -C "$ROOT" config user.name 2>/dev/null || echo RustPDF) <${RUSTPDF_EMAIL:-rustpdf@localhost}>"}

TARGET_OS=""
FORMATS=""
SKIP_BUILD=0
SKIP_TESTS=0
OCR_DATA=1
OUT="$ROOT/dist/packages"

say()  { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33maviso:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merro:\033[0m %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
usage() { sed -n '2,22p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
    case "$1" in
        linux|windows|all) TARGET_OS=$1 ;;
        --formats) FORMATS=${2:?}; shift ;;
        --formats=*) FORMATS=${1#*=} ;;
        --skip-build) SKIP_BUILD=1 ;;
        --skip-tests) SKIP_TESTS=1 ;;
        --out) OUT=${2:?}; shift ;;
        --out=*) OUT=${1#*=} ;;
        --no-ocr-data) OCR_DATA=0 ;;
        -h|--help) usage; exit 0 ;;
        *) usage; die "opção desconhecida: $1" ;;
    esac
    shift
done
[ -n "$TARGET_OS" ] || { usage; exit 1; }
[ -n "$VERSION" ] || die "versão não encontrada em Cargo.toml"
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/rustpdf-pkg.XXXXXX")
trap 'rm -rf "$WORK"' EXIT
PRODUCED=()

wants() {
    # wants FORMATO PADRÃO_DISPONÍVEL → verdadeiro se o formato foi pedido
    # (ou, sem --formats, se está disponível nesta máquina).
    local format=$1 available=$2
    if [ -n "$FORMATS" ]; then
        case ",$FORMATS," in *",$format,"*) return 0 ;; *) return 1 ;; esac
    fi
    [ "$available" = 1 ]
}

# Substitui @CHAVE@ nos modelos.
fill() {
    sed -e "s|@VERSION@|$VERSION|g" -e "s|@URL@|$URL|g" -e "s|@LICENSE@|$LICENSE_ID|g" \
        -e "s|@MAINTAINER@|$MAINTAINER|g" "$@"
}

# ------------------------------------------------------------------ Linux

linux_build() {
    if [ "$SKIP_BUILD" = 0 ]; then
        say "Compilando (release)"
        (cd "$ROOT" && cargo build --release --offline)
        if [ "$SKIP_TESTS" = 0 ]; then
            say "Testando"
            (cd "$ROOT" && cargo test --release --offline)
        fi
    fi
    for binary in rustpdf rustpdf-cli; do
        [ -x "$ROOT/target/release/$binary" ] || die "binário ausente: target/release/$binary"
    done
}

# Tamanhos dos ícones PNG pré-renderizados em packaging/icons (rustpdf-N.png).
ICON_SIZES="16 22 24 32 48 64 128 256"

# Ícone do aplicativo sob `share`: SVG, PNGs do hicolor por tamanho e cópia
# em pixmaps (menus e docks que não leem SVG ou buscam um tamanho fixo).
install_icons() {
    local share=$1 size
    install -Dm644 "$PACKAGING/rustpdf.svg" "$share/icons/hicolor/scalable/apps/rustpdf.svg"
    for size in $ICON_SIZES; do
        install -Dm644 "$PACKAGING/icons/rustpdf-$size.png" \
            "$share/icons/hicolor/${size}x${size}/apps/rustpdf.png"
    done
    install -Dm644 "$PACKAGING/icons/rustpdf-128.png" "$share/pixmaps/rustpdf.png"
}

# Árvore /usr com os arquivos instalados (base de .rpm e .deb).
linux_stage_usr() {
    local root=$1
    install -Dm755 "$ROOT/target/release/rustpdf" "$root/usr/bin/rustpdf"
    install -Dm755 "$ROOT/target/release/rustpdf-cli" "$root/usr/bin/rustpdf-cli"
    install -Dm644 "$PACKAGING/linux/rustpdf.desktop" "$root/usr/share/applications/rustpdf.desktop"
    install_icons "$root/usr/share"
    install -Dm644 "$ROOT/README.md" "$root/usr/share/doc/rustpdf/README.md"
    install -Dm644 "$ROOT/LICENSE" "$root/usr/share/licenses/rustpdf/LICENSE"
    install -Dm644 "$ROOT/TERCEIROS.md" "$root/usr/share/doc/rustpdf/TERCEIROS.md"
}

linux_tar() {
    local name="rustpdf-$VERSION-linux-x86_64"
    local stage="$WORK/$name"
    say "Pacote portátil $name.tar.gz"
    install -Dm755 "$ROOT/target/release/rustpdf" "$stage/bin/rustpdf"
    install -Dm755 "$ROOT/target/release/rustpdf-cli" "$stage/bin/rustpdf-cli"
    install -Dm644 "$PACKAGING/linux/rustpdf.desktop" "$stage/share/applications/rustpdf.desktop"
    install_icons "$stage/share"
    install -Dm644 "$ROOT/README.md" "$stage/share/doc/rustpdf/README.md"
    install -Dm644 "$ROOT/LICENSE" "$stage/share/licenses/rustpdf/LICENSE"
    install -Dm644 "$ROOT/LICENSE" "$stage/LICENSE"
    install -Dm644 "$ROOT/TERCEIROS.md" "$stage/share/doc/rustpdf/TERCEIROS.md"
    install -Dm755 "$PACKAGING/linux/install.sh" "$stage/install.sh"
    install -Dm755 "$PACKAGING/linux/uninstall.sh" "$stage/uninstall.sh"
    tar -C "$WORK" -czf "$OUT/$name.tar.gz" "$name"
    PRODUCED+=("$OUT/$name.tar.gz")
}

linux_arch() {
    local dir="$WORK/arch"
    mkdir -p "$dir"
    say "Pacote Arch/Manjaro (PKGBUILD + makepkg)"
    # Fonte: árvore de trabalho atual do crate (sem target/ e dist/).
    # -h: segue links simbólicos, se houver (o pacote leva o conteúdo).
    tar -h -C "$(dirname "$ROOT")" \
        --exclude="$(basename "$ROOT")/target" --exclude="$(basename "$ROOT")/dist" \
        --transform "s|^$(basename "$ROOT")|rustpdf-$VERSION|" \
        -czf "$dir/rustpdf-$VERSION.tar.gz" "$(basename "$ROOT")"
    local sum
    sum=$(sha256sum "$dir/rustpdf-$VERSION.tar.gz" | cut -d' ' -f1)
    fill "$PACKAGING/linux/PKGBUILD.in" | sed "s|@SHA256@|$sum|" > "$dir/PKGBUILD"
    mkdir -p "$OUT/arch"
    cp "$dir/PKGBUILD" "$dir/rustpdf-$VERSION.tar.gz" "$OUT/arch/"
    PRODUCED+=("$OUT/arch/PKGBUILD" "$OUT/arch/rustpdf-$VERSION.tar.gz")
    if ! have makepkg; then
        warn "makepkg ausente: PKGBUILD e fonte gravados em $OUT/arch (rode makepkg numa máquina Arch)"
        return
    fi
    if [ "$(id -u)" -eq 0 ]; then
        warn "makepkg não roda como root: PKGBUILD gravado em $OUT/arch"
        return
    fi
    local flags=(-f --noconfirm)
    [ "$SKIP_TESTS" = 1 ] && flags+=(--nocheck)
    (cd "$dir" && PKGDEST="$OUT" makepkg "${flags[@]}")
    # A extensão segue o PKGEXT do makepkg.conf (.pkg.tar, .pkg.tar.zst…).
    local package
    for package in "$OUT"/rustpdf-"$VERSION"-*.pkg.tar*; do
        if [ -f "$package" ]; then
            PRODUCED+=("$package")
            if have namcap; then
                say "namcap $(basename "$package")"
                namcap "$package" || true
            fi
        fi
    done
    return 0
}

linux_rpm() {
    say "Pacote RPM"
    local stage="$WORK/rpm-root" top="$WORK/rpmbuild"
    linux_stage_usr "$stage"
    mkdir -p "$top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    fill "$PACKAGING/linux/rustpdf.spec.in" | sed "s|@STAGE@|$stage|g" > "$top/SPECS/rustpdf.spec"
    rpmbuild -bb --quiet \
        --define "_topdir $top" \
        --define "_rpmdir $OUT" \
        --define "_dbpath $WORK/rpmdb" \
        --define "_build_name_fmt %%{NAME}-%%{VERSION}-%%{RELEASE}.%%{ARCH}.rpm" \
        "$top/SPECS/rustpdf.spec"
    PRODUCED+=("$OUT/rustpdf-$VERSION-1.x86_64.rpm")
}

linux_deb() {
    say "Pacote DEB"
    local stage="$WORK/deb-root"
    linux_stage_usr "$stage"
    mkdir -p "$stage/DEBIAN"
    fill "$PACKAGING/linux/debian-control.in" > "$stage/DEBIAN/control"
    install -Dm644 "$ROOT/LICENSE" "$stage/usr/share/doc/rustpdf/copyright"
    local size
    size=$(du -sk "$stage/usr" | cut -f1)
    echo "Installed-Size: $size" >> "$stage/DEBIAN/control"
    dpkg-deb --build --root-owner-group "$stage" "$OUT/rustpdf_${VERSION}-1_amd64.deb" >/dev/null
    PRODUCED+=("$OUT/rustpdf_${VERSION}-1_amd64.deb")
}

package_linux() {
    linux_build
    wants tar 1 && linux_tar
    wants arch "$(have makepkg && echo 1 || echo 0)" && linux_arch
    if wants rpm "$(have rpmbuild && echo 1 || echo 0)"; then
        have rpmbuild || die "rpmbuild ausente (formato rpm pedido)"
        linux_rpm
    fi
    if wants deb "$(have dpkg-deb && echo 1 || echo 0)"; then
        have dpkg-deb || die "dpkg-deb ausente (formato deb pedido)"
        linux_deb
    fi
    local distro
    distro=$(. /etc/os-release 2>/dev/null && echo "${PRETTY_NAME:-desconhecida}")
    warn "Binários ligados às bibliotecas desta máquina ($distro, $(ldd --version 2>/dev/null | head -1 | awk '{print $NF}' | sed 's/^/glibc /')): .rpm/.deb servem para distribuições compatíveis. Para outra família, rode este script nela."
}

# ---------------------------------------------------------------- Windows

# DLLs de que um .exe/.dll depende (nomes como aparecem no PE).
pe_imports() {
    "$OBJDUMP" -p "$1" 2>/dev/null | sed -n 's/^[[:space:]]*DLL Name: //p' | tr -d '\r'
}

# Copia para $1 (pasta bin do pacote) as DLLs do sysroot de que os
# executáveis de $1 dependem, recursivamente. DLLs do sistema (KERNEL32,
# USER32…) não estão no sysroot e ficam de fora.
collect_dlls() {
    local bin=$1 queue=() file dll found
    for file in "$bin"/*.exe "$bin"/*.dll "${EXTRA_PE[@]}"; do
        [ -f "$file" ] && queue+=("$file")
    done
    while [ ${#queue[@]} -gt 0 ]; do
        file=${queue[0]}
        queue=("${queue[@]:1}")
        while IFS= read -r dll; do
            [ -n "$dll" ] || continue
            if find "$bin" -maxdepth 1 -iname "$dll" | grep -q .; then
                continue
            fi
            found=$(find "$SYSROOT/bin" -maxdepth 1 -iname "$dll" | head -1)
            if [ -n "$found" ]; then
                cp "$found" "$bin/"
                queue+=("$bin/$(basename "$found")")
            fi
        done < <(pe_imports "$file")
    done
}

windows_toolchain() {
    if [ -n "${MSYSTEM:-}" ] && [ "${MSYSTEM}" != MSYS ]; then
        SYSROOT=${MINGW_PREFIX:-/mingw64}
        OBJDUMP=${OBJDUMP:-objdump}
        CARGO_TARGET=""
        BIN_DIR="$ROOT/target/release"
        say "MSYS2 $MSYSTEM (sysroot $SYSROOT)"
    elif [ -n "${RUSTPDF_WIN_SYSROOT:-}" ]; then
        SYSROOT=$RUSTPDF_WIN_SYSROOT
        OBJDUMP=${OBJDUMP:-x86_64-w64-mingw32-objdump}
        CARGO_TARGET=x86_64-pc-windows-gnu
        BIN_DIR="$ROOT/target/$CARGO_TARGET/release"
        have x86_64-w64-mingw32-gcc || die "x86_64-w64-mingw32-gcc ausente (compilação cruzada)"
        export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
        export RUSTFLAGS="${RUSTFLAGS:-} -L native=$SYSROOT/lib"
        say "Compilação cruzada para $CARGO_TARGET (sysroot $SYSROOT)"
    else
        die "para Windows, rode no MSYS2 MINGW64/UCRT64 ou defina RUSTPDF_WIN_SYSROOT (ver packaging/README.md)"
    fi
    [ -d "$SYSROOT/bin" ] || die "sysroot inválido: $SYSROOT/bin não existe"
    have "$OBJDUMP" || die "$OBJDUMP ausente (binutils)"
}

windows_build() {
    if [ "$SKIP_BUILD" = 0 ]; then
        say "Compilando (release)"
        if [ -n "$CARGO_TARGET" ]; then
            (cd "$ROOT" && cargo build --release --offline --target "$CARGO_TARGET")
        else
            (cd "$ROOT" && cargo build --release --offline)
            if [ "$SKIP_TESTS" = 0 ]; then
                say "Testando"
                (cd "$ROOT" && cargo test --release --offline)
            fi
        fi
    fi
    for binary in rustpdf.exe rustpdf-cli.exe; do
        [ -f "$BIN_DIR/$binary" ] || die "binário ausente: $BIN_DIR/$binary"
    done
}

# Pasta portátil: bin/ (exe + ferramentas + DLLs), lib/ (loaders do
# gdk-pixbuf), share/ (esquemas, ícones Adwaita, dados do Poppler/Tesseract).
# O GTK no Windows localiza share/ e lib/ relativos à pasta das DLLs.
windows_stage() {
    STAGE="$WORK/rustpdf-$VERSION-windows-x86_64"
    local bin="$STAGE/bin" tool
    mkdir -p "$bin"
    say "Montando $(basename "$STAGE")"
    cp "$BIN_DIR/rustpdf.exe" "$BIN_DIR/rustpdf-cli.exe" "$bin/"
    for tool in pdftoppm pdftotext pdfinfo qpdf; do
        [ -f "$SYSROOT/bin/$tool.exe" ] || die "$tool.exe ausente em $SYSROOT/bin (instale poppler e qpdf)"
        cp "$SYSROOT/bin/$tool.exe" "$bin/"
    done
    if [ -f "$SYSROOT/bin/tesseract.exe" ]; then
        cp "$SYSROOT/bin/tesseract.exe" "$bin/"
    else
        warn "tesseract.exe ausente: o pacote não terá OCR"
    fi
    [ -f "$SYSROOT/bin/gdbus.exe" ] && cp "$SYSROOT/bin/gdbus.exe" "$bin/"

    # Loaders de imagem (PNG/JPEG e SVG — os ícones do RustPDF são SVG).
    EXTRA_PE=()
    local loaders
    loaders=$(find "$SYSROOT/lib/gdk-pixbuf-2.0" -maxdepth 2 -type d -name loaders 2>/dev/null | head -1)
    if [ -n "$loaders" ]; then
        local relative=${loaders#"$SYSROOT/"}
        mkdir -p "$STAGE/$relative"
        cp "$loaders"/*.dll "$STAGE/$relative/"
        local cache
        cache="$(dirname "$loaders")/loaders.cache"
        if [ -f "$cache" ]; then
            # Caminhos absolutos do sysroot viram relativos (pacote relocável).
            local prefix_unix=$SYSROOT prefix_mixed=$SYSROOT
            have cygpath && prefix_mixed=$(cygpath -m "$SYSROOT")
            sed -e "s|$prefix_mixed/||g" -e "s|$prefix_unix/||g" "$cache" \
                > "$STAGE/$(dirname "$relative")/loaders.cache"
        fi
        local loader
        for loader in "$STAGE/$relative"/*.dll; do EXTRA_PE+=("$loader"); done
        ls "$STAGE/$relative" | grep -qi svg || warn "loader SVG (librsvg) ausente: ícones podem não aparecer"
    else
        warn "loaders do gdk-pixbuf não encontrados em $SYSROOT/lib"
    fi
    collect_dlls "$bin"

    mkdir -p "$STAGE/share/glib-2.0/schemas"
    if [ -f "$SYSROOT/share/glib-2.0/schemas/gschemas.compiled" ]; then
        cp "$SYSROOT/share/glib-2.0/schemas/gschemas.compiled" "$STAGE/share/glib-2.0/schemas/"
    fi
    # Ícones internos do GTK (setas, caixas de seleção, combos).
    mkdir -p "$STAGE/share/icons"
    local theme
    for theme in Adwaita hicolor; do
        if [ -d "$SYSROOT/share/icons/$theme" ]; then
            cp -r "$SYSROOT/share/icons/$theme" "$STAGE/share/icons/"
        fi
    done
    [ -d "$SYSROOT/share/poppler" ] && cp -r "$SYSROOT/share/poppler" "$STAGE/share/"
    if [ "$OCR_DATA" = 1 ] && [ -d "$SYSROOT/share/tessdata" ]; then
        mkdir -p "$STAGE/share/tessdata"
        local language
        for language in eng por osd; do
            [ -f "$SYSROOT/share/tessdata/$language.traineddata" ] && cp "$SYSROOT/share/tessdata/$language.traineddata" "$STAGE/share/tessdata/"
        done
        [ -d "$SYSROOT/share/tessdata/configs" ] && cp -r "$SYSROOT/share/tessdata/configs" "$STAGE/share/tessdata/"
    fi
    mkdir -p "$STAGE/etc/gtk-3.0"
    printf '[Settings]\ngtk-icon-theme-name=Adwaita\n' > "$STAGE/etc/gtk-3.0/settings.ini"

    cp "$ROOT/README.md" "$STAGE/LEIA-ME.md"
    cp "$ROOT/LICENSE" "$STAGE/LICENSE.txt"
    cp "$ROOT/TERCEIROS.md" "$STAGE/TERCEIROS.md"
    # Licenças dos componentes embutidos (GPL/LGPL exigem o texto junto).
    if [ -d "$SYSROOT/share/licenses" ]; then
        cp -r "$SYSROOT/share/licenses" "$STAGE/share/"
    else
        warn "$SYSROOT/share/licenses ausente: inclua as licenças dos componentes embutidos"
    fi

    ICON=""
    if have magick; then
        ICON="$WORK/rustpdf.ico"
        if magick -background none -density 384 "$PACKAGING/rustpdf.svg" \
            -define icon:auto-resize=256,64,48,32,16 "$ICON" 2>/dev/null; then
            cp "$ICON" "$STAGE/rustpdf.ico"
        else
            ICON=""
            warn "não foi possível gerar rustpdf.ico"
        fi
    else
        warn "magick ausente: instalador sem ícone"
    fi
    return 0
}

# Caminho no formato do Windows para programas nativos (makensis do MSYS2).
native_path() {
    if have cygpath; then cygpath -w "$1"; else printf '%s' "$1"; fi
}

windows_zip() {
    local name
    name=$(basename "$STAGE")
    say "Pacote portátil $name.zip"
    rm -f "$OUT/$name.zip"
    if have zip; then
        (cd "$WORK" && zip -qr "$OUT/$name.zip" "$name")
    elif have 7z; then
        (cd "$WORK" && 7z a -tzip -bd "$OUT/$name.zip" "$name" >/dev/null)
    else
        die "zip ou 7z necessário para o formato zip"
    fi
    PRODUCED+=("$OUT/$name.zip")
}

windows_nsis() {
    say "Instalador NSIS"
    local setup="$OUT/rustpdf-$VERSION-windows-x86_64-setup.exe"
    local args=("-V2" "-DVERSION=$VERSION" "-DSOURCE=$(native_path "$STAGE")" "-DOUTFILE=$(native_path "$setup")"
        "-DLICENSE_FILE=$(native_path "$STAGE/LICENSE.txt")")
    if [ -n "$ICON" ]; then
        args+=("-DICON=$(native_path "$ICON")")
    fi
    makensis "${args[@]}" "$(native_path "$PACKAGING/windows/rustpdf.nsi")"
    PRODUCED+=("$setup")
}

package_windows() {
    windows_toolchain
    windows_build
    windows_stage
    wants zip 1 && windows_zip
    if wants nsis "$(have makensis && echo 1 || echo 0)"; then
        have makensis || die "makensis ausente (formato nsis pedido)"
        windows_nsis
    fi
}

# ------------------------------------------------------------------ geral

case "$TARGET_OS" in
    linux) package_linux ;;
    windows) package_windows ;;
    all) package_linux; package_windows ;;
esac

[ ${#PRODUCED[@]} -gt 0 ] || die "nenhum pacote gerado"
# Somas de todos os pacotes presentes na pasta (de qualquer execução).
(cd "$OUT" && find . -maxdepth 2 -type f ! -name SHA256SUMS ! -name '*.new' -printf '%P\n' \
    | sort | xargs -r sha256sum) > "$OUT/SHA256SUMS.new"
mv "$OUT/SHA256SUMS.new" "$OUT/SHA256SUMS"
say "Pacotes em $OUT:"
for file in "${PRODUCED[@]}"; do
    if [ -f "$file" ]; then
        printf '   %s  (%s)\n' "${file#"$OUT"/}" "$(du -h "$file" | cut -f1)"
    fi
done
