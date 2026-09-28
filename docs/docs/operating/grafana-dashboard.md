---
sidebar_position: 3
title: Grafana dashboard
description: Import or provision the example Grafana dashboard for the gateway's Prometheus metrics.
---

# Grafana dashboard

The repository ships an example Grafana dashboard for the gateway: [synapse-gateway-dashboard.json](pathname:///dashboards/synapse-gateway-dashboard.json). It charts traffic, latency, token usage, cost-ledger health and embeddings from the metrics the gateway serves on its metrics port. The source file lives at `docs/static/dashboards/synapse-gateway-dashboard.json`.

You need a Prometheus that scrapes the gateway's metrics port (`9090` by default). [Metrics](./metrics.md) explains the endpoint and the scrape job.

## What it shows

| Row | Panels |
|---|---|
| **Diagnostics** (collapsed) | `count by (__name__)({job="$job"})`, which lists every series scraped for the job. If it is empty, the scrape is misconfigured. |
| **Traffic** | Request rate by `route`, by `lane` and by upstream `model`. |
| **Latency** | Request duration p50, p95 and p99 overall, and p95 by `route`. |
| **Tokens** | Input and output token rate by `model`, and the tokens consumed in the last hour. |
| **Resilience** | Circuit-breaker state, leg call rate by `outcome`, retry attempts and breaker transitions. |
| **Cost ledger** | Ledger error rate by `backend`, and the rate of rows dropped because the queue was full. |
| **Embeddings** | Embedding request rate by `route` and by `provider`, and embedding latency p95 by `route`. |

:::note Resilience panels stay empty
The chat paths don't retry legs or use circuit breakers, so the gateway never records the `synapse_resilience_*` metrics and the Resilience row shows no data. [Fallback chains](../guides/fallback-chains.md) describes how the gateway moves between legs.
:::

The traffic, latency and token panels count only requests that produced a response. [What the request metrics count](./metrics.md#what-the-request-metrics-count) explains what that leaves out.

## Before you import

Two things in the JSON are fixed, so check them against your Grafana and Prometheus:

- **Data source UID `prometheus`.** Every panel names its data source by UID `prometheus`, and the file has no import inputs, so Grafana doesn't ask you to choose one. If your Prometheus data source has a different UID, the panels report that the data source was not found. Either give the data source the UID `prometheus`, or rewrite the UID in a copy of the file:

  ```bash
  sed 's/"uid": "prometheus"/"uid": "my-prometheus-uid"/' \
    synapse-gateway-dashboard.json > synapse-gateway-dashboard.local.json
  ```

- **The `$job` variable.** Every query filters on `{job="$job"}`. The variable is a text box at the top of the dashboard and defaults to `synapse-gateway`. If your scrape job has another name, type it there, or change the variable's default and save the dashboard.

## Import it

1. In Grafana, open **Dashboards → New → Import**.
2. Upload `synapse-gateway-dashboard.json`, or paste its contents.
3. Choose a folder and select **Import**.
4. If your scrape job isn't named `synapse-gateway`, set the **job** variable.

Open the Diagnostics row first if the panels show no data: it tells you whether Prometheus has any series for the job.

## Provision it with Docker Compose

Grafana can load the data source and the dashboard from files at startup. This extends the stack in [Docker Compose](../deployment/docker-compose.md). Add a `grafana` directory next to `compose.yaml`:

```text
grafana/
├── dashboards/
│   └── synapse-gateway-dashboard.json
└── provisioning/
    ├── dashboards/
    │   └── synapse.yml
    └── datasources/
        └── prometheus.yml
```

The data source file gives Prometheus the UID the dashboard expects:

```yaml title="grafana/provisioning/datasources/prometheus.yml"
apiVersion: 1
datasources:
  - name: Prometheus
    uid: prometheus
    type: prometheus
    url: http://prometheus:9090
    isDefault: true
```

The dashboard provider loads every JSON file in the dashboards directory into a folder named Synapse:

```yaml title="grafana/provisioning/dashboards/synapse.yml"
apiVersion: 1
providers:
  - name: synapse
    folder: Synapse
    type: file
    options:
      path: /var/lib/grafana/dashboards
```

Add the service to `compose.yaml`:

```yaml
  grafana:
    image: grafana/grafana:12.1.0
    ports:
      - "3000:3000"
    volumes:
      - ./grafana/provisioning:/etc/grafana/provisioning:ro
      - ./grafana/dashboards:/var/lib/grafana/dashboards:ro
    depends_on:
      - prometheus
```

Run `docker compose up -d`, open `http://localhost:3000` (the first login is `admin` / `admin`), and find the dashboard under **Dashboards → Synapse → synapse-gateway**.

## Provision it on Kubernetes

The Grafana that ships with kube-prometheus-stack runs a sidecar that loads any ConfigMap labelled `grafana_dashboard: "1"`. Wrap the JSON in a ConfigMap:

```bash
python3 - <<'PY'
js = open('docs/static/dashboards/synapse-gateway-dashboard.json').read().rstrip('\n')
body = '\n'.join(('    ' + l) if l.strip() else '' for l in js.splitlines())
head = ("apiVersion: v1\nkind: ConfigMap\nmetadata:\n"
        "  name: grafana-dashboard-synapse-gateway\n  namespace: monitoring\n"
        "  labels:\n    grafana_dashboard: \"1\"\n"
        "  annotations:\n    grafana_folder: Synapse\n"
        "data:\n  synapse-gateway.json: |-\n")
open('grafana-dashboard-synapse-gateway.yaml', 'w').write(head + body + '\n')
PY
```

Change `namespace` to the one your Grafana sidecar watches. The `grafana_folder` annotation takes effect only if the sidecar is configured to read folder annotations; otherwise the dashboard lands in the sidecar's default folder. Rerun the script whenever you edit the JSON.

Prometheus must scrape the gateway's metrics port. With the Prometheus Operator, point a ServiceMonitor at the Service port that exposes `9090`, with path `/metrics`. The job label defaults to the Service name, so either name the Service `synapse-gateway` or set the dashboard's `$job` variable to match.

## What it doesn't chart

The dashboard has no panels for these metrics. Query them directly, or add panels of your own:

- the Gemini passthrough counters, `synapse_passthrough_total` and `synapse_passthrough_fallback_total`;
- the Jev router metrics, `synapse_routing_decisions_total` and `synapse_routing_decision_duration_seconds` (see [Jev router](../guides/jev-router.md#metrics-and-logs));
- the Jev extraction counter, `synapse_jev_extraction_total` (see [Jev lane](../guides/jev-lane.md#ledger-and-metrics));
- the guardrail metrics, `synapse_guard_scans_total`, `synapse_guard_matches_total` and `synapse_guard_scan_duration_seconds` (see [Guardrails](../configuration/guardrails-policy.md#metrics));
- `synapse_resilience_call_duration_seconds`.

Cost and embedding token usage go to the [cost ledger](../guides/cost-ledger.md), not to Prometheus, so the dashboard has no cost panels. [Metrics](./metrics.md#queries) has starting queries for everything above.

## Editing the dashboard

Keep the dashboard `uid` (`synapse-gateway`) unchanged, so links and provisioned copies keep pointing at the same dashboard. If you edit the dashboard in Grafana, export it as JSON with the "export for sharing externally" option off, so the data source stays referenced by UID, and replace the JSON file.
