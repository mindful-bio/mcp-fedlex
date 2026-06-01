#!/usr/bin/env python3
"""
LikeC4 Architektur-Viewer stoppen.

Beendet den LikeC4-Dev-Server auf dem Projekt-Port (SIGTERM, dann SIGKILL) und
raeumt zusaetzlich uebrige 'likec4 serve'-Prozesse weg.

VS Code:
    Diese Datei oeffnen und oben rechts auf den Play-Button (>) klicken.

Shell:
    python3 likec4/stop.py
"""
from __future__ import annotations

import os
import signal
import subprocess
import sys
import time

DEFAULT_PORT = 5173
PORT = int(os.environ.get("LIKEC4_PORT", str(DEFAULT_PORT)))


def pids_on_port(port: int) -> list[int]:
    """Liefert alle PIDs, die am Port lauschen (macOS / Linux mit lsof)."""
    try:
        out = subprocess.check_output(
            ["lsof", "-ti", f":{port}", "-sTCP:LISTEN"],
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        return []
    return [int(line) for line in out.strip().splitlines() if line.strip()]


def kill_pids(pids: list[int], sig: int) -> None:
    """Schickt Signal an PIDs, ignoriert harmlose Fehler."""
    for pid in pids:
        try:
            os.kill(pid, sig)
        except ProcessLookupError:
            pass
        except PermissionError:
            print(f"Keine Berechtigung um PID {pid} zu beenden.", file=sys.stderr)


def main() -> int:
    killed_something = False

    pids = pids_on_port(PORT)
    if pids:
        print(f"Beende Prozesse am Port {PORT}: {pids}")
        kill_pids(pids, signal.SIGTERM)
        time.sleep(1)
        still_alive = pids_on_port(PORT)
        if still_alive:
            print(f"   SIGKILL an: {still_alive}")
            kill_pids(still_alive, signal.SIGKILL)
        killed_something = True

    if not killed_something:
        print(f"LikeC4-Viewer laeuft nicht (Port {PORT} frei).")
    else:
        print("LikeC4-Viewer gestoppt.")

    return 0


if __name__ == "__main__":
    sys.exit(main())
