---
name: "🐞 Bug-Report"
about: "Ein reproduzierbares Fehlverhalten melden (kein Sicherheitsproblem)."
title: "bug: <kurze Zusammenfassung>"
labels: ["bug", "needs-triage"]
assignees: []
---

> **Sicherheitsrelevant?** Bitte **nicht** hier melden — siehe [`SECURITY.md`](../../SECURITY.md)
> (vertraulicher Meldeweg: security@mindful.bio).

## Was ist passiert?

<!-- Eine klare, knappe Beschreibung des Fehlverhaltens. -->

## Erwartetes Verhalten

<!-- Was hätte stattdessen passieren sollen? -->

## Reproduktion (so minimal wie möglich)

Bitte den exakten Pfad nennen, idealerweise gegen einen frischen Lauf:

```bash
cp .env.example .env
docker compose up --build      # Reader auf http://localhost:8080
# ... der Aufruf, der den Fehler auslöst, z. B.:
curl -s -X POST localhost:8080/rpc \
  -H 'authorization: Bearer dev-secret-change-me' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

1.
2.
3.

## Beobachtete Ausgabe / Logs

<!--
Roh-Antwort, Exit-Code, relevante Logzeilen.
WICHTIG: keine echten Tokens, JWTs, Secrets oder personenbezogenen Daten einkleben.
-->

```text

```

## Umgebung

- mcp-fedlex Version / Image-Tag (z. B. `v0.1.0`, `latest`, `<sha>`):
- Aufrufweg: [ ] `docker compose`  [ ] Kubernetes (k3-infra)  [ ] `cargo run`  [ ] anderer:
- Client: [ ] `curl`  [ ] ansV  [ ] anderer MCP-Client:
- Ausgehandelte MCP-Protokollversion (aus `initialize`), falls bekannt:
- OS / Architektur:

## Checkliste

- [ ] Ich habe geprüft, dass dies **kein** Sicherheitsproblem ist (sonst SECURITY.md).
- [ ] `scripts/smoke.sh <base-url> <token>` wurde ausgeführt und das Ergebnis ist oben dokumentiert.
- [ ] Ausgabe/Logs enthalten **keine** Secrets oder personenbezogenen Daten.
