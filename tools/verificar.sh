#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Cadeia de verificação completa; para no primeiro erro (código de saída ≠ 0).
#   tools/verificar.sh              confere
#   tools/verificar.sh --atualizar  confere e grava a catraca de cobertura
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --offline --all-targets -q -- -D warnings
python3 tools/cobertura.py "$@"
echo "Cadeia verde."
