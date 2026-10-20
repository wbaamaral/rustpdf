#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Cobertura de testes por camada, com mínimos e catraca.

    tools/cobertura.py              mede e confere (falha se abaixo)
    tools/cobertura.py --atualizar  mede, confere os mínimos e grava a catraca

Mede com `cargo llvm-cov` (linhas), sem a ligação GTK (main.rs, shell.rs,
icons.rs), que não tem regras. Mínimos por camada: dominio 90%,
app e apresentacao 85% (camadas ainda vazias são ignoradas). A catraca em
`cobertura.txt` guarda a cobertura total: ela não pode cair.
"""
import json
import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
RATCHET = ROOT / "cobertura.txt"
REPORT = ROOT / "target" / "cobertura.json"
IGNORE = r"src/(main|shell|icons)\.rs|/rustc-|/library/|/\.cargo/"
MINIMUMS = {"dominio": 90.0, "app": 85.0, "apresentacao": 85.0}
TOLERANCE = 0.25  # variação entre ambientes (versões do qpdf/poppler)


def llvm_environment():
    """Usa o llvm-cov do sistema quando o rustup não tem llvm-tools."""
    env = dict(os.environ)
    sysroot = subprocess.run(
        ["rustc", "--print", "sysroot"], capture_output=True, text=True, check=True
    ).stdout.strip()
    has_rustup_tools = any(pathlib.Path(sysroot).glob("lib/rustlib/*/bin/llvm-cov"))
    if not has_rustup_tools:
        for variable, tool in (("LLVM_COV", "llvm-cov"), ("LLVM_PROFDATA", "llvm-profdata")):
            if variable not in env and shutil.which(tool):
                env[variable] = shutil.which(tool)
    return env


def measure():
    REPORT.parent.mkdir(exist_ok=True)
    subprocess.run(
        [
            "cargo", "llvm-cov", "--offline", "--json", "--summary-only",
            "--ignore-filename-regex", IGNORE,
            "--output-path", str(REPORT),
        ],
        cwd=ROOT, env=llvm_environment(), check=True,
    )
    data = json.loads(REPORT.read_text())["data"][0]
    files = [
        (pathlib.Path(entry["filename"]), entry["summary"]["lines"])
        for entry in data["files"]
    ]
    return files, data["totals"]["lines"]


def layer_of(path):
    parts = path.parts
    if "src" in parts:
        rest = parts[len(parts) - parts[::-1].index("src"):]
        if len(rest) > 1 and rest[0] in MINIMUMS:
            return rest[0]
    return None


def percent(covered, count):
    return 100.0 * covered / count if count else 100.0


def main():
    update = "--atualizar" in sys.argv[1:]
    files, totals = measure()
    failures = []

    print("\nCobertura de linhas por camada:")
    for layer, minimum in MINIMUMS.items():
        count = sum(lines["count"] for path, lines in files if layer_of(path) == layer)
        covered = sum(lines["covered"] for path, lines in files if layer_of(path) == layer)
        if count == 0:
            print(f"  {layer:<13} (vazia)")
            continue
        value = percent(covered, count)
        status = "ok" if value >= minimum else "ABAIXO"
        print(f"  {layer:<13} {value:6.2f}%  (mínimo {minimum:.0f}%, {covered}/{count})  {status}")
        if value < minimum:
            failures.append(f"{layer}: {value:.2f}% < {minimum:.0f}%")

    total = percent(totals["covered"], totals["count"])
    previous = float(RATCHET.read_text().split()[-1]) if RATCHET.exists() else 0.0
    print(f"  {'total':<13} {total:6.2f}%  (catraca {previous:.2f}%)")
    if total + TOLERANCE < previous:
        failures.append(f"total: {total:.2f}% < catraca {previous:.2f}%")

    if failures:
        print("\nCobertura insuficiente:\n  " + "\n  ".join(failures))
        return 1
    if update:
        # A catraca só sobe: uma queda dentro da tolerância não a rebaixa.
        ratchet = max(total, previous)
        RATCHET.write_text(f"linhas {ratchet:.2f}\n")
        print(f"\nCatraca gravada em {RATCHET.name}: {ratchet:.2f}%")
    elif total > previous + 0.5:
        print("\nA cobertura subiu: rode com --atualizar para gravar a catraca.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
