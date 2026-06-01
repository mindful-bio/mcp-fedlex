#!/usr/bin/env python3
"""
bundle.py - Python-Port von bundle.sh.

Fasst specification.c4 + model.c4 + views.c4 zu einem einzigen, kopierfaehigen
PLAN_BUNDLE.md zusammen (fuer Review-Prompts). Titel/Name werden aus
likec4.config.json gelesen (im likec4-Ordner oder im Projekt-Root).

VS Code:
    Diese Datei oeffnen und oben rechts auf den Play-Button (>) klicken.

Shell:
    python3 likec4/bundle.py            # schreibt PLAN_BUNDLE.md + Ausgabe auf stdout
    python3 likec4/bundle.py --stdout   # nur Ausgabe auf stdout (keine Datei)
"""
from __future__ import annotations

import datetime
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
FILES = ["specification.c4", "model.c4", "views.c4"]
OUT = HERE / "PLAN_BUNDLE.md"


def load_meta() -> tuple[str, str]:
    """Liest name + title aus der naechstgelegenen likec4.config.json."""
    for cfg in (HERE / "likec4.config.json", HERE.parent / "likec4.config.json"):
        if cfg.exists():
            try:
                data = json.loads(cfg.read_text(encoding="utf-8"))
                return data.get("name", HERE.parent.name), data.get("title", "")
            except (json.JSONDecodeError, OSError):
                break
    return HERE.parent.name, ""


def build() -> str | None:
    """Baut den Bundle-Text. None bei fehlender Quelldatei."""
    name, title = load_meta()
    ts = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")

    parts: list[str] = []
    parts.append(f"# {name} - Vollstaendiger LikeC4-Architektur-Plan\n")
    parts.append(
        f"_Generiert am {ts} aus {len(FILES)} Quelldateien:_ `{' '.join(FILES)}`\n"
    )
    if title:
        parts.append(
            f"{title}. Der folgende Block enthaelt die komplette `specification`, "
            "das `model` und alle `views`.\n"
        )

    for fname in FILES:
        path = HERE / fname
        if not path.exists():
            print(f"FEHLER: Datei '{fname}' nicht gefunden.", file=sys.stderr)
            return None
        parts.append(f"## {fname}\n")
        parts.append("```likec4")
        parts.append(path.read_text(encoding="utf-8").rstrip("\n"))
        parts.append("```\n")

    return "\n".join(parts) + "\n"


def main() -> int:
    content = build()
    if content is None:
        return 1

    if "--stdout" in sys.argv[1:]:
        sys.stdout.write(content)
        return 0

    OUT.write_text(content, encoding="utf-8")
    sys.stdout.write(content)
    print("--------------------------------------------------------", file=sys.stderr)
    print(f"Bundle geschrieben nach: {OUT}", file=sys.stderr)
    print(
        f"Zeilen: {content.count(chr(10))}  |  Bytes: {len(content.encode('utf-8'))}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
