#!/usr/bin/env bash
# Post-Deploy-Smoke-Test für mcp-fedlex.
#
# Prüft eine laufende Instanz end-to-end und failt mit Exit != 0, sobald etwas
# nicht JSON-konform antwortet (z. B. 503/HTML statt JSON-RPC). Genau dieser
# Test fängt den klassischen "Service liefert 503 statt Antwort"-Fall ab.
#
# Verwendung:
#   scripts/smoke.sh <base-url> <bearer-token>
#   scripts/smoke.sh http://localhost:8080 dev-secret-change-me
#
# Exit-Codes: 0 = alles grün, 1 = ein Check rot, 2 = Aufruf-/Setup-Fehler.

set -euo pipefail

BASE_URL="${1:-${MCP_BASE_URL:-http://localhost:8080}}"
TOKEN="${2:-${MCP_TOKEN:-}}"

if [[ -z "$TOKEN" ]]; then
  echo "FEHLER: kein Token. Aufruf: $0 <base-url> <bearer-token>" >&2
  exit 2
fi

BASE_URL="${BASE_URL%/}"
fail=0

red()   { printf '  \033[31m✗ %s\033[0m\n' "$1"; fail=1; }
green() { printf '  \033[32m✓ %s\033[0m\n' "$1"; }

# --- 1. Liveness -------------------------------------------------------------
echo "[1/4] GET /livez"
code=$(curl -fsS -o /dev/null -w '%{http_code}' "$BASE_URL/livez" 2>/dev/null || echo "000")
if [[ "$code" == "200" ]]; then green "livez 200"; else red "livez lieferte HTTP $code (erwartet 200)"; fi

# --- 2. Readiness ------------------------------------------------------------
echo "[2/4] GET /readyz"
code=$(curl -fsS -o /dev/null -w '%{http_code}' "$BASE_URL/readyz" 2>/dev/null || echo "000")
if [[ "$code" == "200" ]]; then green "readyz 200"; else red "readyz lieferte HTTP $code (erwartet 200; Redis/Fedlex erreichbar?)"; fi

# --- 3. initialize (JSON-RPC-Handshake) -------------------------------------
echo "[3/4] POST /rpc initialize"
init=$(curl -fsS -X POST "$BASE_URL/rpc" \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":0,"method":"initialize"}' 2>/dev/null || echo '')
if echo "$init" | jq -e '.result.serverInfo.name == "mcp-fedlex-reader"' >/dev/null 2>&1; then
  green "initialize: serverInfo ok ($(echo "$init" | jq -r '.result.protocolVersion'))"
else
  red "initialize lieferte kein gültiges serverInfo-JSON. Roh: ${init:0:200}"
fi

# --- 4. tools/call read_article (mit Provenance) -----------------------------
echo "[4/4] POST /rpc tools/call read_article (Provenance-Gate)"
call=$(curl -fsS -X POST "$BASE_URL/rpc" \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call",
       "params":{"name":"read_article",
                 "arguments":{"eli":"eli/cc/1999/404","eid":"art_1"},
                 "as_of":"2024-01-01"}}' 2>/dev/null || echo '')
if echo "$call" | jq -e '.result.provenance.eli // .result.content // .result' >/dev/null 2>&1 \
   && echo "$call" | jq -e '.error | not' >/dev/null 2>&1; then
  green "tools/call ok, Provenance: $(echo "$call" | jq -rc '.result.provenance // "n/a"')"
else
  red "tools/call lieferte einen Fehler oder kein JSON. Roh: ${call:0:200}"
fi

echo
if [[ "$fail" -eq 0 ]]; then
  echo "Smoke-Test GRÜN für $BASE_URL"
  exit 0
else
  echo "Smoke-Test ROT für $BASE_URL" >&2
  exit 1
fi
