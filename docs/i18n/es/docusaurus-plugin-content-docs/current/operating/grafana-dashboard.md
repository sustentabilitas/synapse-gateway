---
sidebar_position: 3
title: Panel de Grafana
description: Importa o aprovisiona el panel de Grafana de ejemplo para las métricas de Prometheus del gateway.
---

El repositorio incluye un panel de Grafana de ejemplo para el gateway:
[synapse-gateway-dashboard.json](pathname:///dashboards/synapse-gateway-dashboard.json). Muestra
en gráficos el tráfico, la latencia, el uso de tokens, el estado del registro de costes y los
embeddings a partir de las métricas que el gateway sirve en su puerto de métricas. El archivo
fuente está en `docs/static/dashboards/synapse-gateway-dashboard.json`.

Necesitas un Prometheus que recopile el puerto de métricas del gateway (`9090` por defecto).
[Métricas](./metrics.md) explica el endpoint y el job de scrape.

## Qué muestra {#what-it-shows}

| Fila | Gráficos |
|---|---|
| **Diagnostics** (contraída) | `count by (__name__)({job="$job"})`, que lista cada serie recopilada para el job. Si está vacía, el scrape está mal configurado. |
| **Traffic** | Tasa de peticiones por `route`, por `lane` y por `model` upstream. |
| **Latency** | Duración de las peticiones p50, p95 y p99 en total, y p95 por `route`. |
| **Tokens** | Tasa de tokens de entrada y de salida por `model`, y los tokens consumidos en la última hora. |
| **Resilience** | Estado de los circuit breakers, tasa de llamadas a tramos por `outcome`, reintentos y transiciones de los breakers. |
| **Cost ledger** | Tasa de errores del registro por `backend`, y la tasa de filas descartadas porque la cola estaba llena. |
| **Embeddings** | Tasa de peticiones de embeddings por `route` y por `provider`, y latencia p95 de los embeddings por `route`. |

:::note Los gráficos de Resilience se quedan vacíos
Las rutas de chat no reintentan tramos ni usan circuit breakers, así que el gateway nunca
registra las métricas `synapse_resilience_*` y la fila Resilience no muestra datos.
[Cadenas de fallback](../guides/fallback-chains.md) describe cómo pasa el gateway de un tramo a
otro.
:::

Los gráficos de tráfico, latencia y tokens solo cuentan las peticiones que produjeron una
respuesta. [Qué cuentan las métricas de peticiones](./metrics.md#what-the-request-metrics-count)
explica qué queda fuera.

## Antes de importar {#before-you-import}

Hay dos cosas fijas en el JSON, así que compruébalas con tu Grafana y tu Prometheus:

- **UID de la fuente de datos `prometheus`.** Cada gráfico identifica su fuente de datos por el
  UID `prometheus`, y el archivo no tiene entradas de importación, así que Grafana no te pide que
  elijas una. Si tu fuente de datos de Prometheus tiene otro UID, los gráficos indican que no se
  encontró la fuente de datos. Asigna a la fuente de datos el UID `prometheus`, o reescribe el
  UID en una copia del archivo:

  ```bash
  sed 's/"uid": "prometheus"/"uid": "my-prometheus-uid"/' \
    synapse-gateway-dashboard.json > synapse-gateway-dashboard.local.json
  ```

- **La variable `$job`.** Cada consulta filtra por `{job="$job"}`. La variable es un cuadro de
  texto en la parte superior del panel y su valor por defecto es `synapse-gateway`. Si tu job de
  scrape tiene otro nombre, escríbelo ahí, o cambia el valor por defecto de la variable y guarda
  el panel.

## Importarlo {#import-it}

1. En Grafana, abre **Dashboards → New → Import**.
2. Sube `synapse-gateway-dashboard.json`, o pega su contenido.
3. Elige una carpeta y selecciona **Import**.
4. Si tu job de scrape no se llama `synapse-gateway`, define la variable **job**.

Si los gráficos no muestran datos, abre primero la fila Diagnostics: te dice si Prometheus tiene
alguna serie para el job.

## Aprovisionarlo con Docker Compose {#provision-it-with-docker-compose}

Grafana puede cargar la fuente de datos y el panel desde archivos al arrancar. Esto amplía el
stack de [Docker Compose](../deployment/docker-compose.md). Añade un directorio `grafana` junto a
`compose.yaml`:

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

El archivo de la fuente de datos asigna a Prometheus el UID que espera el panel:

```yaml title="grafana/provisioning/datasources/prometheus.yml"
apiVersion: 1
datasources:
  - name: Prometheus
    uid: prometheus
    type: prometheus
    url: http://prometheus:9090
    isDefault: true
```

El proveedor de paneles carga cada archivo JSON del directorio de paneles en una carpeta llamada
Synapse:

```yaml title="grafana/provisioning/dashboards/synapse.yml"
apiVersion: 1
providers:
  - name: synapse
    folder: Synapse
    type: file
    options:
      path: /var/lib/grafana/dashboards
```

Añade el servicio a `compose.yaml`:

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

Ejecuta `docker compose up -d`, abre `http://localhost:3000` (el primer inicio de sesión es
`admin` / `admin`) y busca el panel en **Dashboards → Synapse → synapse-gateway**.

## Aprovisionarlo en Kubernetes {#provision-it-on-kubernetes}

El Grafana que incluye kube-prometheus-stack ejecuta un sidecar que carga cualquier ConfigMap con
la etiqueta `grafana_dashboard: "1"`. Envuelve el JSON en un ConfigMap:

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

Cambia `namespace` por el que vigila tu sidecar de Grafana. La anotación `grafana_folder` solo
tiene efecto si el sidecar está configurado para leer anotaciones de carpeta; si no, el panel
acaba en la carpeta por defecto del sidecar. Vuelve a ejecutar el script cada vez que edites el
JSON.

Prometheus debe recopilar el puerto de métricas del gateway. Con el Prometheus Operator, apunta
un ServiceMonitor al puerto del Service que expone `9090`, con el path `/metrics`. La etiqueta
job toma por defecto el nombre del Service, así que llama al Service `synapse-gateway` o ajusta
la variable `$job` del panel para que coincida.

## Qué no muestra {#what-it-doesnt-chart}

El panel no tiene gráficos para estas métricas. Consúltalas directamente, o añade tus propios
gráficos:

- los contadores del passthrough de Gemini, `synapse_passthrough_total` y
  `synapse_passthrough_fallback_total`;
- las métricas del router Jev, `synapse_routing_decisions_total` y
  `synapse_routing_decision_duration_seconds` (consulta
  [Router Jev](../guides/jev-router.md#metrics-and-logs));
- el contador de extracción de Jev, `synapse_jev_extraction_total` (consulta
  [Carril Jev](../guides/jev-lane.md#ledger-and-metrics));
- las métricas de guardrails, `synapse_guard_scans_total`, `synapse_guard_matches_total` y
  `synapse_guard_scan_duration_seconds` (consulta
  [Guardrails](../configuration/guardrails-policy.md#metrics));
- `synapse_resilience_call_duration_seconds`.

El coste y el uso de tokens de los embeddings van al [registro de costes](../guides/cost-ledger.md),
no a Prometheus, así que el panel no tiene gráficos de costes.
[Métricas](./metrics.md#queries) tiene consultas de partida para todo lo anterior.

## Editar el panel {#editing-the-dashboard}

Mantén sin cambios el `uid` del panel (`synapse-gateway`), para que los enlaces y las copias
aprovisionadas sigan apuntando al mismo panel. Si editas el panel en Grafana, expórtalo como JSON
con la opción "export for sharing externally" desactivada, para que la fuente de datos siga
referenciada por UID, y reemplaza el archivo JSON.
