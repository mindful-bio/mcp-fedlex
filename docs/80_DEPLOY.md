# 80 — Produktiver Betrieb (Kubernetes)

> **Was dieses Dokument ist.** Wie der Reader produktiv läuft (k3s/ArgoCD), wie der mTLS-
> Pfad zur Quota-Redis aufgesetzt und rotiert wird, und wie man nach einem Rollout
> verifiziert, dass die Instanz tatsächlich antwortet (Smoke-Test). Die Manifeste liegen
> im Infra-Repo unter `manifests/workloads/mcp-fedlex/`.

---

## 1. Topologie

```
Internet ──▶ Cloudflare ──▶ Ingress (nginx) ──▶ Service mcp-reader ──▶ Reader-Pods (distroless)
                                                                          │ rediss:// (mTLS)
                                                                          ▼
                                                                  Service mcp-reader-redis
                                                                          │
                                                                          ▼
                                                                  Redis (nur TLS-Port 6379)
```

- **Reader-Image:** distroless, kein Shell/`wget`/`curl`. Health-Probes laufen als
  HTTP-GET-Proben (kein `exec`), Verifikation von aussen via Port-Forward/Ingress.
- **Auth:** JWT/JWKS (kein Dev-Token in Produktion). Secrets als SealedSecrets im Repo.
- **Quota-Redis:** ausschliesslich TLS (`--port 0 --tls-port 6379`), Client-Cert-Pflicht
  (`--tls-auth-clients yes`) plus `requirepass` (Defense-in-Depth, ADR-005).
- **NetworkPolicy:** Default-Deny; nur Reader→Redis auf 6379 erlaubt.

## 2. mTLS-Material erzeugen & rotieren

Das Skript `gen-redis-mtls.sh` erzeugt CA, Server-/Client-Zertifikat und das Redis-
Passwort und versiegelt alles mit `kubeseal` zu vier SealedSecrets:

| SealedSecret | Inhalt | gemountet in |
| --- | --- | --- |
| `mcp-reader-redis-ca` | `ca.crt` | Reader **und** Redis |
| `mcp-reader-redis-tls` | Server-`tls.crt`/`tls.key` | Redis |
| `mcp-reader-redis-client-tls` | Client-`tls.crt`/`tls.key` | Reader |
| `mcp-reader-redis-auth` | `password` + `redis_url` (`rediss://…`) | Reader (Env) + Redis |

```bash
cd manifests/workloads/mcp-fedlex
./gen-redis-mtls.sh > tls.sealed.yaml     # erzeugt + versiegelt alle vier
git add tls.sealed.yaml && git commit -m "rotate mcp-fedlex redis mTLS" && git push
# ArgoCD synchronisiert; danach Redis- und Reader-Pods rollen lassen (siehe §4).
```

> Das Skript prüft beim Start selbst, ob der Sealed-Secrets-Controller erreichbar ist,
> und bricht mit klarer Meldung ab, statt eine leere/halbe Datei zu schreiben. Controller-
> Name/Namespace sind über `SEALED_CONTROLLER_NAME` / `SEALED_CONTROLLER_NS` übersteuerbar
> (Default: `sealed-secrets-controller` / `kube-system`).

## 3. Erstinstallation / Sync

`kustomization.yaml` listet die SealedSecrets **vor** den Workloads, die sie mounten —
sonst hängen die Pods in `ContainerCreating` (`MountVolume.SetUp failed … secret not found`).

```bash
kubectl kustomize manifests/workloads/mcp-fedlex | kubectl apply -f -   # oder via ArgoCD
```

## 4. Pods nach Secret-Änderungen neu ausrollen

Neu erstellte Secrets werden **nicht** automatisch in bereits hängende Pods gemountet.
Nach einem Sync, der Secrets neu anlegt, die betroffenen Pods einmal löschen:

```bash
kubectl -n mcp-fedlex delete pod -l app.kubernetes.io/name=mcp-reader-redis
kubectl -n mcp-fedlex delete pod -l app.kubernetes.io/name=mcp-reader
```

Erfolg prüfen — der Redis-Service muss Endpoints haben (sonst 503 am Ingress):

```bash
kubectl -n mcp-fedlex get endpoints mcp-reader-redis mcp-reader   # beide != <none>
kubectl -n argocd get application mcp-fedlex                       # Synced / Healthy
```

## 5. Post-Deploy-Smoke-Test (Pflicht)

`/livez` ist anschlagsfrei und sagt **nichts** über die Quota-Redis-Verbindung. Die
Wahrheit steht in `/readyz` (prüft Redis + Fedlex-SPARQL) und im JSON-RPC-Pfad.
Nach jedem Rollout:

```bash
# öffentlich, erwartet JSON (kein 503-HTML):
curl -s https://mcp-fedlex.ch/rpc -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | jq
# ohne Token erwartet: {"error":{"code":-32001,"message":"missing credential"}}

# end-to-end mit gültigem Token:
scripts/smoke.sh https://mcp-fedlex.ch "<bearer-token>"
```

## 6. Runbook — Ingress liefert 503

Beobachtet am 2026-06-18: Ingress 503-HTML, ansV-Client „error decoding response body“.
Symptomkette und Diagnose:

1. **Reader-Pods nicht ready** (`0/1 Running`), `/readyz` → `{"failing":["redis"]}`.
2. **Redis-Service ohne Endpoints** (`<none>`) → Reader findet kein Backend.
3. **Ursache:** Redis-Pod in `ContainerCreating`, Event `MountVolume.SetUp failed …
   secret "mcp-reader-redis-*" not found` → die mTLS-SealedSecrets fehlten/waren nicht
   erzeugt.

Diagnose-Kommandos:

```bash
kubectl -n mcp-fedlex get pods
kubectl -n mcp-fedlex get events --sort-by=.lastTimestamp | tail -20
kubectl -n mcp-fedlex get endpoints mcp-reader-redis mcp-reader
kubectl -n mcp-fedlex get secret | grep mcp-reader-redis
# /readyz ohne Shell im Pod prüfen (distroless): Port-Forward statt exec
kubectl -n mcp-fedlex port-forward svc/mcp-reader 18080:8080 &
curl -s http://127.0.0.1:18080/readyz
```

Behebung: SealedSecrets erzeugen (§2), in `kustomization.yaml` aufnehmen (vor den
Workloads), syncen, betroffene Pods neu ausrollen (§4), Smoke-Test (§5).

> **Merksatz.** `wget`/`curl`/`exec` funktionieren im distroless-Pod **nicht** — immer
> über Port-Forward oder den Ingress prüfen.
