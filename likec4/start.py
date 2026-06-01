#!/usr/bin/env python3
"""
LikeC4 Architektur-Viewer starten.

Startet den LikeC4-Dev-Server (likec4 serve via npx) und oeffnet automatisch den
Browser. Live-Reload bei Aenderungen in *.c4-Files.

VS Code:
    Diese Datei oeffnen und oben rechts auf den Play-Button (>) klicken
    (Python-Extension muss installiert sein).

Shell:
    python3 likec4/start.py

Stop:
    python3 likec4/stop.py     oder Strg+C im Terminal
"""
from __future__ import annotations

import os
import socket
import subprocess
import sys
import threading
import time
import webbrowser
from pathlib import Path

# Eigener Default-Port pro Projekt, damit alle vier fedlex-Viewer parallel laufen.
DEFAULT_PORT = 5173
PORT = int(os.environ.get("LIKEC4_PORT", str(DEFAULT_PORT)))
LIKEC4_DIR = Path(__file__).resolve().parent
URL = f"http://localhost:{PORT}"


def port_in_use(port: int) -> bool:
    """Prueft ob am Port schon jemand lauscht."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(0.5)
        try:
            sock.connect(("127.0.0.1", port))
            return True
        except (ConnectionRefusedError, socket.timeout, OSError):
            return False


def open_browser_when_ready() -> None:
    """Warte bis der Server lauscht, dann Browser oeffnen (max 60s)."""
    for _ in range(120):
        if port_in_use(PORT):
            time.sleep(0.5)
            webbrowser.open(URL)
            return
        time.sleep(0.5)
    print(f"Server nach 60s nicht erreichbar auf {URL}", file=sys.stderr)


def main() -> int:
    if port_in_use(PORT):
        print(f"LikeC4 laeuft bereits auf {URL}")
        webbrowser.open(URL)
        return 0

    print(f"Starte LikeC4-Viewer auf {URL} ...")
    print("   Browser oeffnet sich automatisch sobald der Server bereit ist.")
    print("   Stoppen: Strg+C hier, oder python3 likec4/stop.py.")
    print()

    threading.Thread(target=open_browser_when_ready, daemon=True).start()

    cmd = ["npx", "--yes", "likec4", "serve", "--listen", "0.0.0.0", "--port", str(PORT)]
    try:
        completed = subprocess.run(cmd, cwd=LIKEC4_DIR, check=False)
        return completed.returncode
    except FileNotFoundError:
        print("'npx' nicht im PATH gefunden. Node/npm installieren.", file=sys.stderr)
        return 127
    except KeyboardInterrupt:
        print("\nLikeC4-Viewer beendet (Strg+C).")
        return 0


if __name__ == "__main__":
    sys.exit(main())
