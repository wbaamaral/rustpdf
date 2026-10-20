#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Instala o RustPDF a partir do pacote portátil.
#   ./install.sh            → ~/.local (usuário) ou /usr/local (root)
#   ./install.sh /opt/x     → prefixo escolhido
set -eu
here=$(cd "$(dirname "$0")" && pwd)
if [ $# -ge 1 ]; then prefix=$1
elif [ "$(id -u)" -eq 0 ]; then prefix=/usr/local
else prefix=$HOME/.local
fi

install -Dm755 "$here/bin/rustpdf" "$prefix/bin/rustpdf"
install -Dm755 "$here/bin/rustpdf-cli" "$prefix/bin/rustpdf-cli"
install -Dm644 "$here/share/applications/rustpdf.desktop" "$prefix/share/applications/rustpdf.desktop"
install -Dm644 "$here/share/icons/hicolor/scalable/apps/rustpdf.svg" "$prefix/share/icons/hicolor/scalable/apps/rustpdf.svg"
install -Dm644 "$here/share/licenses/rustpdf/LICENSE" "$prefix/share/licenses/rustpdf/LICENSE"
for doc in "$here"/share/doc/rustpdf/*; do
    install -Dm644 "$doc" "$prefix/share/doc/rustpdf/$(basename "$doc")"
done
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$prefix/share/applications" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" 2>/dev/null || true

echo "RustPDF instalado em $prefix"
case ":$PATH:" in *":$prefix/bin:"*) ;; *) echo "Aviso: $prefix/bin não está no PATH." ;; esac

# Dependências de execução (bibliotecas e ferramentas do sistema).
missing=""
for tool in pdftoppm pdftotext pdfinfo qpdf; do
    command -v "$tool" >/dev/null 2>&1 || missing="$missing $tool"
done
libs=$(ldd "$prefix/bin/rustpdf" 2>/dev/null | awk '/not found/ {print $1}' | tr '\n' ' ')
if [ -n "$missing" ] || [ -n "$libs" ]; then
    echo
    echo "Faltam dependências:${missing:+ ferramentas:$missing}${libs:+ bibliotecas: $libs}"
    echo "  Arch/Manjaro:  sudo pacman -S gtk3 qpdf poppler libjpeg-turbo tesseract tesseract-data-por"
    echo "  Debian/Ubuntu: sudo apt install libgtk-3-0 qpdf poppler-utils libturbojpeg0 tesseract-ocr tesseract-ocr-por"
    echo "  Fedora:        sudo dnf install gtk3 qpdf poppler-utils turbojpeg tesseract tesseract-langpack-por"
fi
command -v tesseract >/dev/null 2>&1 || echo "Opcional: instale o tesseract para OCR."
