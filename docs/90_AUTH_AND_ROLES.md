# 90 — Authentifizierung & Rollen

> **Was dieses Dokument ist.** Das Identitäts- und Rechtemodell des Readers: woher die
> Identität kommt, welche Claims gelten, welche Rolle welche Werkzeuge sieht und welche
> Quota sie erhält. Quelle: `crates/mcp-reader/src/auth.rs`, `tool.rs`, `quota.rs`.

---

## 1. Grundsatz — Identität nie aus LLM-Parametern (ADR-002)

Die Identität (Mandant, Session, Rolle) stammt **ausschliesslich** aus einem geprüften
Credential im `Authorization`-Header, nie aus Tool-Argumenten. `VerifiedClaims` ist nur
aus einem verifizierten Token konstruierbar; `tenant`/`session` werden zusätzlich
hart validiert (Defense-in-Depth gegen fehlkonfigurierte IdPs). Damit kann ein LLM weder
den Mandanten wechseln noch seine Rolle hochstufen noch die Quota umgehen.

Der Reader ist **fail-closed**: ohne gültiges Credential `-32001 missing/invalid credential`.

## 2. Claims-Schema (JWT)

| Claim | Bedeutung | Beispiel |
| --- | --- | --- |
| `iss` | Aussteller, muss `MCP_JWT_ISSUER` entsprechen | `https://idp.example.com/` |
| `aud` | Zielgruppe, geprüft falls `MCP_JWT_AUDIENCE` gesetzt | `mcp-fedlex` |
| `exp` | Ablauf (Pflicht) | — |
| `tenant` | Mandant (pseudonyme Audit-Identität) | `kanzlei-a` |
| `sid` | Session (pseudonyme Audit-Identität) | `sess-1` |
| `role` | `reader` \| `navigator` \| `validator` | `navigator` |

Ein unbekannter `role`-String wird abgewiesen (kein stillschweigendes Downgrade).

## 3. Rollen & sichtbare Pools (RBAC, Least-Privilege)

Es gilt strikte Schachtelung **Reader ⊆ Navigator ⊆ Validator**. Eine Rolle sieht in
`tools/list` nur ihre erlaubten Pools und darf auch nur diese aufrufen — selbst wenn ein
LLM einen fremden Tool-Namen errät.

| Pool | Reader | Navigator | Validator | Inhalt (Beispiele) |
| --- | :--: | :--: | :--: | --- |
| `LocalNavigation` | ✅ | ✅ | ✅ | `read_article`, `get_structure`, `search_text` … |
| `LodFederation` | — | ✅ | ✅ | föderierte Referenzauflösung |
| `Discovery` | — | ✅ | ✅ | `search_law`, `resolve_sr_number`, `find_related_topic` |
| `JoluxMetadata` | — | ✅ | ✅ | `check_in_force`, `get_impacts`, `get_taxonomy` … |
| `Workspace` | — | ✅ | ✅ | Arbeitskontext |
| `Validation` | — | — | ✅ | `compare_versions` |

> **ansV läuft mit der Rolle `Navigator`** — Discovery und JOLux-Metadaten erlaubt,
> Validierungs-Tools nicht.

## 4. Quota pro Rolle (verteiltes Token-Bucket, ADR-002)

Jede Rolle hat einen eigenen Bucket pro `(tenant, session)`. Höhere Rollen bekommen
grössere Buckets. Das **Kostengewicht** pro Aufruf ist pool-gebunden (nie aus einem
LLM-Parameter): lokale Navigation = 1, Live-SPARQL (`Discovery`/`JoluxMetadata`) = 5.

| Rolle | Kapazität | Nachfüllrate |
| --- | --: | --: |
| Reader | 60 | 1.0 / s |
| Navigator | 120 | 2.0 / s |
| Validator | 240 | 4.0 / s |

Fällt das Quota-Redis aus, greift **nicht** die reguläre Kapazität, sondern ein enges
Fallback-Bucket (Kapazität 5) — fail-closed statt fail-open.

## 5. Auth-Modi (Auswahlreihenfolge)

Konfiguriert über Umgebungsvariablen, erste passende gewinnt. Vollständige Referenz:
[`70_CONFIG.md`](./70_CONFIG.md §2).

1. **JWKS** (`MCP_JWT_JWKS_URL`) — rotierende Schlüssel vom IdP, periodischer Refresh,
   fail-closed bis zum ersten erfolgreichen Abruf. **Empfohlen für Produktion.**
2. **HS256** (`MCP_JWT_HS256_SECRET`) — symmetrisches Secret.
3. **RS256** (`MCP_JWT_RS256_PUBKEY_FILE`) — PEM-Public-Key.
4. **Dev-Token** (`MCP_DEV_TOKEN`) — statisch, Rolle `Validator`, Mandant `dev`.
   **Nur lokal**, nie in Produktion.

## 6. Audit-Log (pro `tools/call`)

Jeder Aufruf erzeugt eine PII-gescrubbte Logzeile (ADR-001). Auf der Allowlist stehen nur
die pseudonymen Audit-Identitäten und Provenance — **nie** rohe Tool-Argumente oder
Antwortinhalte:

```json
{"event":"tools/call","tool.name":"read_article","auth.role":"Navigator",
 "auth.tenant":"kanzlei-a","auth.session":"sess-1",
 "provenance.eli":"eli/cc/1999/404","provenance.valid_as_of":"2024-01-01",
 "outcome":"ok","span.duration_ms":"29"}
```
