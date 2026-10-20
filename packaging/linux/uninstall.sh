#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Remove o RustPDF instalado pelo install.sh (mesmo prefixo).
set -eu
if [ $# -ge 1 ]; then prefix=$1
elif [ "$(id -u)" -eq 0 ]; then prefix=/usr/local
else prefix=$HOME/.local
fi
rm -f "$prefix/bin/rustpdf" "$prefix/bin/rustpdf-cli" \
      "$prefix/share/applications/rustpdf.desktop" \
      "$prefix/share/icons/hicolor/scalable/apps/rustpdf.svg"
rm -rf "$prefix/share/doc/rustpdf" "$prefix/share/licenses/rustpdf"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$prefix/share/applications" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" 2>/dev/null || true
echo "RustPDF removido de $prefix"
