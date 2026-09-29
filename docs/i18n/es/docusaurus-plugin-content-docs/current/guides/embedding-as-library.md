---
sidebar_position: 9
title: Synapse como biblioteca embebida
description: Depende del crate synapse-gateway sin su servidor HTTP y llama a Gateway::chat, chat_stream y embed en el mismo proceso desde tu servicio en Rust.
---

Embebe Synapse cuando tu servicio esté escrito en Rust y quieras enrutamiento, fallback,
decisiones de Jev y contabilidad de costes sin ejecutar ni operar un proceso de gateway
aparte. Construyes un `Gateway` en código y lo llamas directamente: sin salto HTTP, sin
despliegue adicional y con el mismo comportamiento de enrutamiento que el binario. Ejecuta el
binario en su lugar cuando varios servicios en distintos lenguajes compartan un gateway, o
cuando quieras las métricas y el registro de costes del gateway en un solo sitio.

La referencia de la API está en [docs.rs](https://docs.rs/synapse-gateway).

## Añade la dependencia {#add-the-dependency}

Desactiva las features por defecto, que añaden el servidor HTTP y el registro en SQLite:

```toml
[dependencies]
synapse-gateway = { version = "0.5", default-features = false }
anyhow = "1"
futures = "0.3"
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

El crate de biblioteca se llama `synapse`, así que lo importas como `synapse::…`. Añade
`ledger-sqlite`, `ledger-postgres`, `ledger-pubsub` o `ledger-sns` a `features` para tener un
backend de registro; consulta [Instalación](../get-started/installation.md#cargo).

## Construye un gateway {#build-a-gateway}

`Gateway::builder()` recibe la tabla de rutas, el catálogo de proveedores, la tabla de precios
y un handle del registro, todos obligatorios, además de partes opcionales. Este ejemplo sirve
la ruta `chat` del inicio rápido:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use synapse::config::vertex_project_from_env;
use synapse::gateway::{ChatOutcome, Gateway, RequestCtx};
use synapse::ledger::{LedgerHandle, NoopLedger};
use synapse::pricing::PricingTable;
use synapse::providers::vertex_auth::VertexAuth;
use synapse::providers::Catalog;
use synapse::routing::request::ChatRequest;
use synapse::routing::stream::StreamItem;
use synapse::routing::table::RouteTable;
use synapse::vertex_native::VertexNativeProvider;

const ROUTES: &str = r#"
[routes."chat"]
legs = [
  { provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" },
  { provider = "openai", model = "gpt-4o-mini" },
]
"#;

const PRICING: &str = r#"
"vertex:gemini-3.5-flash-lite" = { input = 0.30, output = 2.50 }
"openai:gpt-4o-mini" = { input = 0.15, output = 0.60 }
"#;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env: HashMap<String, String> = std::env::vars().collect();
    let timeout = Duration::from_secs(120);
    let routes = RouteTable::from_toml_str(ROUTES)?;
    let catalog = Catalog::build(&env, &routes.referenced_providers(), timeout)?;

    let gateway = Gateway::builder()
        .routes(routes)
        .catalog(catalog)
        .pricing(PricingTable::from_toml_str(PRICING)?)
        .ledger(LedgerHandle::spawn(Arc::new(NoopLedger), 1024))
        .vertex_native(vertex_project_from_env(&env).map(|project| {
            VertexNativeProvider::new(
                Arc::new(VertexAuth::from_adc()),
                project,
                "global".into(),
                timeout,
                None,
            )
        }))
        .default_tenant("my-service")
        .build()?;

    let ctx = RequestCtx {
        tenant: Some("my-team".into()),
        ..Default::default()
    };
    let req: ChatRequest = serde_json::from_value(serde_json::json!({
        "model": "chat",
        "messages": [{ "role": "user", "content": "Say hello in one word." }],
    }))?;

    let (outcome, routing) = gateway.chat_routed(req.clone(), &ctx).await?;
    if let ChatOutcome::Plain(c) = outcome {
        println!("{} answered: {}", c.model, c.content);
        println!("tokens: {} in, {} out", c.input_tokens, c.output_tokens);
    }
    println!("routing: {:?}", routing.headers());

    let mut stream = gateway.chat_stream(req, &ctx).await?;
    println!("streaming from {}", stream.model());
    while let Some(item) = stream.next().await {
        match item {
            Ok(StreamItem::Delta(text)) => print!("{text}"),
            Ok(_) => {}
            Err(e) => eprintln!("stream failed: {e}"),
        }
    }
    Ok(())
}
```

Lo que conviene saber sobre cada pieza:

- **`RouteTable` y `PricingTable`** analizan el mismo TOML que `routes.toml` y
  `pricing.toml`. En un servicio real, léelos de ficheros con `std::fs::read_to_string`.
- **`Catalog::build`** crea un cliente para cada proveedor que referencian las rutas, leyendo
  las credenciales del mapa que le pasas, y falla si falta alguno, como la validación estricta
  del binario. No lee por sí solo el entorno del proceso.
- **`LedgerHandle::spawn`** arranca el escritor en segundo plano en el runtime de Tokio
  actual. `NoopLedger` descarta el uso; pasa un almacén como
  `synapse::ledger::sqlite::SqliteLedger` (feature `ledger-sqlite`) o un `FanoutLedger` de
  varios almacenes para conservarlo. El segundo argumento es el tamaño de la cola.
- **`vertex_native`** activa el carril Vertex nativo. El último argumento de
  `VertexNativeProvider::new` sustituye el host de la API, para pruebas; `None` lo deriva de la
  región de cada tramo.

## Chat {#chat}

`Gateway::chat(req, &ctx)` pasa una petición por los guardrails, la planificación de la ruta y
la cadena de fallback, registra el uso y devuelve un `ChatOutcome`:

- `ChatOutcome::Plain(Completion)` para un completion normal, con `provider`, `model`,
  `content`, `tool_calls`, `finish_reason`, `input_tokens` y `output_tokens`.
- `ChatOutcome::Hybrid(HybridOutcome)` para una
  [extracción híbrida](jev-lane.md#hybrid-extraction), con `answers`, `survivors`, `degraded`
  y las `extractions` de cada candidato.

`ChatRequest` es el cuerpo de petición de OpenAI, incluidos los bloques `vertex` y `jev`, así
que la forma más sencilla de construir uno es deserializar JSON, como arriba. `RequestCtx`
lleva la atribución que en el binario llevan los encabezados HTTP; consulta
[Atribución por tenant](tenant-attribution.md#in-an-embedded-gateway). Establece su
`request_id` para correlacionar las filas del registro con tus propios ids.

Los errores son `synapse::error::GatewayError`: `UnknownModel`, `BadRequest`,
`NativeFeatureUnsupported`, `ContentBlocked`, `AllLegsFailed` (con el fallo de cada tramo) y
`Upstream`, los mismos casos que la API HTTP asigna a códigos de estado. El enum también
declara `UpstreamTimeout` y `AllCircuitsOpen`, que el gateway no devuelve actualmente.

## Lee el informe de enrutamiento {#read-the-routing-report}

`Gateway::chat_routed(req, &ctx)` devuelve `(ChatOutcome, RoutingReport)`. El informe indica
cómo se enrutó la petición, lo que importa en las rutas con [router Jev](jev-router.md):

| Campo | Significado |
|---|---|
| `mode` | `RoutingMode::Jev`, `Static` o `StaticOverride`. |
| `tier` | El nivel que sirvió la petición, si la ruta tiene niveles. |
| `tier_decided` | El nivel que eligió Jev, solo cuando sirvió un nivel distinto. |
| `effort` | El esfuerzo del tramo que sirvió la petición, o `"client"`. |
| `degraded` | `"timeout"`, `"error"`, `"low_confidence"` o `"jev_unavailable"` cuando no se usó la decisión de Jev. |

`RoutingReport::headers()` devuelve los mismos valores que los encabezados de respuesta
`x-synapse-*` que envía el binario.

## Stream {#stream}

`Gateway::chat_stream(req, &ctx)` se compromete con el primer tramo que produce salida y
devuelve un `GuardedStream`, un `Stream` de elementos `Result<StreamItem, LegError>`:

- `StreamItem::Delta` lleva texto, `StreamItem::ToolCallDelta` un fragmento de llamada a
  herramienta, y `StreamItem::Done` el motivo de finalización y el número de tokens.
- `stream.model()` es el modelo que sirve la petición, y `stream.routing()` el
  `RoutingReport`.
- El uso se registra exactamente una vez, cuando el stream se consume por completo o se
  descarta, así que descartarlo antes de tiempo también escribe una fila en el registro.

Se aplican las [reglas de streaming](streaming-and-tools.md#fallback-while-streaming): no hay
fallback después del primer elemento.

## Embeddings {#embeddings}

Para servir alias de embeddings, pasa al builder la tabla de embeddings y un embedder por
proveedor, y después llama a `Gateway::embed(req, ctx)`, que recibe el contexto por valor:

```rust
use synapse::embeddings::vertex::VertexEmbedder;
use synapse::embeddings::{EmbeddingInput, EmbeddingRequest};
use synapse::routing::embeddings::EmbeddingRouteTable;

const EMBED_ROUTES: &str = r#"
[embeddings."embed"]
dimensions = 768
legs = [{ provider = "vertex", model = "text-embedding-004" }]
"#;

let project = vertex_project_from_env(&env)
    .ok_or_else(|| anyhow::anyhow!("set VERTEX_PROJECT_ID"))?;

let gateway = Gateway::builder()
    // ... rutas, catálogo, precios y registro como arriba
    .embed_routes(EmbeddingRouteTable::from_toml_str(EMBED_ROUTES)?)
    .embedder(
        "vertex",
        Arc::new(VertexEmbedder::new(
            Arc::new(VertexAuth::from_adc()),
            project,
            "global".into(),
            timeout,
        )),
    )
    .build()?;

let resp = gateway
    .embed(
        EmbeddingRequest {
            input: EmbeddingInput::Many(vec!["first chunk".into(), "second chunk".into()]),
            model: "embed".into(),
            dimensions: None,
        },
        RequestCtx {
            tenant: Some("my-team".into()),
            ..Default::default()
        },
    )
    .await?;
```

`EmbeddingRouteTable` lee las tablas `[embeddings."<alias>"]` del mismo TOML que tus rutas;
consulta [Embeddings](embeddings.md).

## Lo que el builder deja fuera {#what-the-builder-leaves-out}

El binario configura a partir de su entorno varias partes que un gateway construido con el
builder solo tiene si las añades:

| Parte | Por defecto | Cómo añadirla |
|---|---|---|
| Carril Vertex nativo | Desactivado: las peticiones nativas fallan con `400`. | `.vertex_native(Some(VertexNativeProvider::new(...)))` |
| Carril Jev y router Jev | Desactivados: las peticiones del carril Jev fallan con `400`, y las rutas `jev` omiten la decisión y empiezan en `default_tier`, informando de `jev_unavailable`. | `.jev_native(Some(JevNativeProvider::new(api_key, None, timeout)))` |
| Guardrails | Desactivados. | `.guard(GuardEngine::from_config(&GuardrailsConfig::from_toml_str(..)?)?)` |
| Métricas | No se registran en ningún sitio. | `.metrics(Arc::new(GatewayMetrics::new(&meter)))` con un meter de tu `MeterProvider` de OpenTelemetry |
| Tipos de tarea de IA | `simple`, salvo que el `RequestCtx` establezca `ai_task_type`. | `.ai_task_types(AiTaskTypeTable::from_toml_str(..)?)` |
| Tiempos de espera | 120 s para el primer fragmento, 60 s de inactividad. | `.timeouts(StreamTimeouts { first_chunk, idle })` |
| Tenant por defecto | `unattributed`. | `.default_tenant("...")` |
| Embeddings | Sin alias. | `.embed_routes(...)` y `.embedder(...)`, como arriba. |

Con la feature `server`, `synapse::telemetry::install` construye los mismos exportadores de
Prometheus y OTLP que usa el binario y devuelve el `GatewayMetrics` que hay que pasar a
`.metrics(...)`. Mantén vivo el `MetricsExporter` que devuelve durante toda la vida del
proceso.
