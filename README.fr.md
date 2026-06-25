# mcp-fedlex

[![Release](https://img.shields.io/badge/release-v0.2.0-green.svg)](./CHANGELOG.md)
[![MCP](https://img.shields.io/badge/MCP-2025--11--25-blue.svg)](./docs/adr/ADR-008-mcp-protocol-version-upgrade.md)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-edition%202024-orange.svg)](./Cargo.toml)

[🇬🇧 English](./README.md) · [🇩🇪 Deutsch](./README.de.md) · 🇫🇷 Français · [🇮🇹 Italiano](./README.it.md)

Un **serveur Model-Context-Protocol pour Fedlex** (droit fédéral suisse) — un
produit de [mindful.bio](https://mindful.bio). Il donne à un LLM un accès
**vérifiable** au droit fédéral consolidé, au lieu de le laisser formuler
librement :

> 📖 **Description complète du projet** (cinq langues : outils, démarrage rapide,
> architecture) : **[mcp-fedlex.ch](https://mcp-fedlex.ch)**.
> Plateforme applicative en production bâtie sur ce serveur :
> **[ansv.ch](https://ansv.ch)**. Voir aussi la section
> [Écosystème](#écosystème) ci-dessous.

- 📌 **Provenance par construction** — chaque réponse porte son `eli` et sa date
  de référence `valid_as_of`. La date est **estampillée côté serveur** et ne peut
  être falsifiée par aucun argument d'outil. Distinction structurelle : une
  **preuve normative** (`kind: "norm"`) vs. un **indice de découverte**
  (`kind: "hint"`, candidat — *pas* une preuve), de sorte qu'un moteur de
  raisonnement ne comptabilise jamais par erreur un résultat comme norme établie.
- 🔒 **RBAC au moindre privilège** — 25 outils répartis en quatre pools actifs,
  filtrés par rôle (Reader ⊆ Navigator ⊆ Validator). L'identité provient toujours
  du justificatif vérifié, jamais d'un paramètre du LLM.
- 🧯 **Cloisonnement des locataires & quota** — appliqués côté serveur par jeton
  (seau à jetons distribué et fail-closed via Redis) ; la découverte en direct
  pèse plus lourd dans le quota que la navigation locale, afin de protéger le
  point d'accès public de Fedlex.
- 🧾 **Journal d'audit par appel** — chaque ligne `tools/call` consigne le
  locataire, la session, l'outil, l'ELID et la date de référence ; les arguments
  bruts et le contenu des réponses sont caviardés en mode fail-closed
  (nettoyeur PII, ADR-001).
- 🦀 **Rust, aucun réseau dans les tests** — les tests unitaires et d'intégration
  s'exécutent hors ligne ; la conformité en direct contre Fedlex est séparée
  (`-- --ignored`).

> **Remarque :** ce dépôt GitHub est un **miroir public**. La source de vérité
> (CI/CD, releases) réside sur un GitLab auto-hébergé ; les issues et PR ici sont
> examinées, mais traitées là-bas.

## Ce qu'il sait faire

25 outils répartis en quatre pools actifs, filtrés par RBAC
(Reader ⊆ Navigator ⊆ Validator). Le **Reader** ne voit que `LocalNavigation` ;
le **Navigator** (mode de fonctionnement d'ansV) ajoute `Discovery` et
`JoluxMetadata` ; le **Validator** ajoute en plus `Validation`.

**Navigation dans le texte de l'acte (AKN, pool `LocalNavigation`, 11 outils)**
`read_article` · `read_element` · `read_document` · `get_structure` · `search_text`
· `get_metadata` · `get_references` · `get_modifications` · `list_components`
· `extract_tables` · `detect_foreign_content`.

**Découverte d'actes (pool `Discovery`, 3 outils)**
`search_law` · `resolve_sr_number` · `find_related_topic`. Les résultats portent
une **provenance d'indice** (`kind: "hint"`) — des candidats, pas des preuves
normatives.

**Métadonnées & relations (JOLux, pool `JoluxMetadata`, 10 outils)**
`check_in_force` · `list_versions` · `resolve_consolidation_at` · `get_impacts` ·
`get_outgoing_impacts` · `get_article_history` · `get_citations` · `get_taxonomy` ·
`get_subdivisions` · `list_annexes`.

**Validation (pool `Validation`, 1 outil)**
`compare_versions` (comparaison de versions, Validator uniquement).

> `Discovery` et `JoluxMetadata` interrogent **en direct** le point d'accès SPARQL
> public de Fedlex et pèsent plus lourd dans le quota (coût 5 au lieu de 1),
> tandis que `LocalNavigation` est servi depuis le cache de manifestations du pod.

## En route localement en 2 minutes

Prérequis : Docker avec Compose. Aucune chaîne d'outils Rust nécessaire.

```bash
cp .env.example .env          # définir le jeton de dev & co. (les défauts suffisent pour tester)
docker compose up --build     # démarrer Reader + Redis
```

Le Reader écoute alors sur `http://localhost:8080`. Vérifier la santé :

```bash
curl -s http://localhost:8080/livez      # -> "ok" (liveness)
curl -s http://localhost:8080/readyz      # -> vérifie Redis + Fedlex SPARQL
```

Lister les outils (jeton de dev depuis votre `.env`) :

```bash
TOKEN=dev-secret-change-me
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | jq
```

Lire un article de la Constitution fédérale à une date de référence :

```bash
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{
          "name":"read_article",
          "arguments":{"eli":"eli/cc/1999/404","eid":"art_1"},
          "as_of":"2024-01-01"
        }
      }' | jq
```

La réponse contient le texte de la norme **et** un bloc `provenance`
(`eli`, `valid_as_of`). Le paramètre facultatif `as_of` (ISO `YYYY-MM-DD`) règle
la date de référence ; sans lui, la date du jour s'applique.

## Se connecter à un client MCP

Le serveur parle MCP via JSON-RPC (protocole `2025-11-25` ; un ancien client
demandant explicitement `2024-11-05` reçoit toujours `2024-11-05`). Il existe
trois routes HTTP :

- **`POST /mcp`** — le **point d'accès Streamable HTTP** de la révision cible
  `2025-11-25` (recommandé). Applique deux gardes de transport *avant* tout
  travail : un en-tête `Origin` étranger est rejeté avec **403** (protection
  contre le DNS rebinding), un en-tête `MCP-Protocol-Version` défini mais non pris
  en charge avec **400**.
- **`POST /rpc`** — le point d'accès hérité (même chaîne `McpService` sans les
  deux gardes). Conservé pour les anciens clients sans handshake.
- **`GET /sse`** — ouvre le flux SSE et annonce `/rpc` comme adresse POST.

Méthodes : `initialize` (handshake avec négociation de version), `tools/list`
(filtré par RBAC), `tools/call` (limité par quota, à travers la porte de
provenance), `ping` (keep-alive), ainsi que la notification
`notifications/initialized` (acquittée par **202 Accepted** sans corps).
**Auth :** jeton Bearer dans l'en-tête `Authorization` à **chaque** requête
(sauf notifications).

Exemple `initialize` :

```bash
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":0,"method":"initialize"}' | jq
# -> { "protocolVersion":"2025-11-25",
#      "serverInfo":{"name":"mcp-fedlex-reader", ...},
#      "capabilities":{"tools":{}} }
```

> Sans `protocolVersion` dans la requête, le serveur négocie la révision par
> défaut `2025-11-25`. Un client qui envoie explicitement
> `"protocolVersion":"2024-11-05"` reçoit toujours `2024-11-05` (compatibilité
> ascendante pour les anciens clients).

Pour les clients à configuration JSON (p. ex. Claude Desktop via un connecteur
pont SSE/HTTP), l'URL de base `http://localhost:8080` et le jeton Bearer suffisent.

### Inspecter dans le navigateur (MCP Inspector)

Pour parcourir les outils de façon interactive, utilisez l'
[MCP Inspector](https://github.com/modelcontextprotocol/inspector) officiel. Un
fichier [`inspector.json`](./inspector.json) prêt à l'emploi est fourni dans le
dépôt : **une seule commande** se connecte avec l'URL **et** le jeton pré-remplis —
sans manipulation manuelle de l'interface :

```bash
npx -y @modelcontextprotocol/inspector --config inspector.json --server fedlex
```

L'Inspector s'ouvre dans le navigateur déjà **connecté** ; choisissez l'onglet
**Tools**, p. ex. `read_article` avec `eli = eli/cc/1999/404` et `eid = art_1`.

`inspector.json` pointe vers `http://localhost:8090/mcp` car le port hôte est
configurable — si `8080` est déjà pris, démarrez le serveur sur un autre port :

```bash
MCP_HOST_PORT=8090 docker compose up --build   # le port du conteneur reste 8080
```

Le conteneur écoute toujours sur `8080` ; `MCP_HOST_PORT` ne remappe que le côté
hôte (voir `docker-compose.yml`). Gardez la valeur dans `inspector.json` cohérente.

> Vous préférez configurer à la main ? Transport **Streamable HTTP**, URL
> `http://localhost:8090/mcp` et un en-tête **activé**
> `Authorization: Bearer <jeton>`. Jeton par défaut : `dev-secret-change-me` (votre `.env`).

## Configuration

Toute la configuration passe par des variables d'environnement. Référence complète
avec valeurs par défaut et champs obligatoires :
**[`docs/70_CONFIG.md`](./docs/70_CONFIG.md)**. Le modèle de rôles et de jetons
(jeton de dev vs. JWT/JWKS) figure dans
**[`docs/90_AUTH_AND_ROLES.md`](./docs/90_AUTH_AND_ROLES.md)**.

> La configuration Compose est destinée au **développement** (Redis en clair,
> jeton de dev statique). Exploitation en production sur Kubernetes (JWT/JWKS,
> Redis mTLS, SealedSecrets) : **[`docs/80_DEPLOY.md`](./docs/80_DEPLOY.md)**.

## Obtenir une image versionnée

Pour un usage tiers, il existe des **images SemVer citables** (immuables, liées à
un tag Git) en plus des tags glissants du déploiement continu interne :

| Tag | Objet | Stabilité |
|-----|-------|-----------|
| `:v0.2.0` | release citable (liée au tag Git `v0.2.0`, MCP `2025-11-25`) | immuable — **recommandée pour les tiers** |
| `:v0.1.0` | release plus ancienne (liée au tag Git `v0.1.0`, MCP `2024-11-05`) | immuable |
| `:latest` | dernier état de `main` | glissant |
| `:<short-sha>` | commit exact | immuable, interne |

```bash
docker pull registry.mindful-server.com/mindful-bio/mcp-fedlex:v0.2.0
```

Les releases sont documentées dans [`CHANGELOG.md`](./CHANGELOG.md) ; la
`serverInfo.version` rapportée (voir `initialize`) correspond au SemVer de
`Cargo.toml`. Une nouvelle release naît d'un tag Git `vX.Y.Z` — la CI construit
automatiquement l'image du même nom.

## Compiler & tester depuis les sources

```bash
cargo build --workspace
cargo test  --workspace                 # tests unitaires/d'intégration, sans réseau
cargo test  --workspace -- --ignored      # conformité en direct contre Fedlex (réseau)
```

## Architecture & décisions

- Plan d'architecture LikeC4 : [`likec4/`](./likec4)
- Lexique des capacités (espace fonctionnel JOLux) : [`docs/10_LEXICON_jolux.md`](./docs/10_LEXICON_jolux.md)
- Plan de mise en œuvre & liste de contrôle : [`docs/30_PLAN.md`](./docs/30_PLAN.md)
- Points ouverts & utilisabilité : [`docs/60_OPEN_ITEMS_AND_USABILITY.md`](./docs/60_OPEN_ITEMS_AND_USABILITY.md)
- Constats de revue (registre vivant) : [`docs/65_REVIEW_FINDINGS.md`](./docs/65_REVIEW_FINDINGS.md)
- Architecture Decision Records : [`docs/adr/`](./docs/adr)
- Contribuer : [`CONTRIBUTING.md`](./CONTRIBUTING.md) · Sécurité : [`SECURITY.md`](./SECURITY.md)

## Écosystème

`mcp-fedlex` est la couche de données à provenance garantie d'une petite famille
de produits de [mindful.bio](https://mindful.bio) :

| Projet | De quoi il s'agit | Lien |
|--------|-------------------|------|
| **mcp-fedlex** (ce dépôt) | Le serveur MCP : accès vérifiable et à date de référence au droit fédéral suisse. Description complète du projet en cinq langues (outils, démarrage rapide, architecture). | **[mcp-fedlex.ch](https://mcp-fedlex.ch)** |
| **ansV** | La **plateforme applicative** qui utilise ce serveur comme client Navigator — analyses juridiques avec une chaîne de preuves traçable. | **[ansv.ch](https://ansv.ch)** |
| **mindful.bio** | L'entreprise derrière les deux projets. | **[mindful.bio](https://mindful.bio)** |

## Licence

[Apache-2.0](./LICENSE) © mindful.bio
