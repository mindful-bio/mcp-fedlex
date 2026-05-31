#!/usr/bin/env bash
# ============================================================
# bundle.sh - Fasst den gesamten LikeC4-Plan zu einem einzigen,
# kopierfähigen Block zusammen (für Review-Prompts).
#
# Usage:  ./bundle.sh            # schreibt PLAN_BUNDLE.md + Ausgabe auf stdout
#         ./bundle.sh --stdout   # nur Ausgabe auf stdout (keine Datei)
# ============================================================
set -euo pipefail

# In das Verzeichnis dieses Skripts wechseln (= likec4/-Ordner)
cd "$(dirname "$0")"

FILES=(specification.c4 model.c4 views.c4)
OUT="PLAN_BUNDLE.md"

build() {
  echo "# mcp-fedlex - Vollständiger LikeC4-Architektur-Plan"
  echo
  echo "_Generiert am $(date '+%Y-%m-%d %H:%M:%S') aus ${#FILES[@]} Quelldateien:_ \`${FILES[*]}\`"
  echo
  echo "Föderierter \"Agentic Legal Navigator\" (MCP-Server, Rust/tokio). Der folgende"
  echo "Block enthält die komplette \`specification\`, das \`model\` und alle \`views\`."
  echo

  for f in "${FILES[@]}"; do
    if [[ ! -f "$f" ]]; then
      echo "FEHLER: Datei '$f' nicht gefunden." >&2
      exit 1
    fi
    echo "## ${f}"
    echo
    echo '```likec4'
    cat "$f"
    echo '```'
    echo
  done
}

if [[ "${1:-}" == "--stdout" ]]; then
  build
else
  build | tee "$OUT"
  echo "--------------------------------------------------------" >&2
  echo "Bundle geschrieben nach: $(pwd)/$OUT" >&2
  echo "Zeilen: $(wc -l < "$OUT" | tr -d ' ')  |  Bytes: $(wc -c < "$OUT" | tr -d ' ')" >&2
fi
