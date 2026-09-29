---
sidebar_position: 1
title: synapse-mcp
description: synapse-mcp es una biblioteca que enruta llamadas a herramientas MCP sobre Streamable HTTP hacia servidores MCP upstream registrados bajo demanda, inyectando la identidad del tenant desde un almacén de contexto compartido.
---

`synapse-mcp` es un gateway [Model Context Protocol](https://modelcontextprotocol.io/) (MCP)
bajo demanda. Un cliente, normalmente código que se ejecuta en un sandbox, habla MCP sobre
Streamable HTTP con `/mcp/{server}` en un listener de loopback. El gateway reenvía cada llamada
a herramienta al servidor MCP upstream registrado con ese nombre, por una conexión propia que
lleva los encabezados de identidad del tenant actual. El cliente nunca ve la URL del upstream y
no puede fijar su propia identidad.

Está construido sobre [`rmcp`](https://docs.rs/rmcp) 2.2, el SDK oficial de MCP para Rust.

## Cuándo usarlo {#when-to-use-it}

Úsalo junto a [`synapse-proxy`](../proxy/overview.md) cuando código no confiable tenga que
llamar a servidores MCP en nombre de un tenant: tu plano de control fija el tenant y registra
los servidores que puede usar la sesión, y el sandbox solo alcanza esos servidores, y solo como
ese tenant.

## Qué es {#what-it-is}

`synapse-mcp` es una biblioteca, no un programa. Proporciona dos routers de axum que tu
aplicación monta:

- `mcp_gateway_router` sirve `/mcp/{server}`, el endpoint MCP al que se conectan los clientes.
- `mcp_admin_router` sirve `POST /internal/mcp/servers` y
  `DELETE /internal/mcp/servers/{name}`, que registran los servidores upstream.

Ni el binario del proxy ni el del gateway los montan. No hay puerto por defecto ni archivo de
configuración: tu aplicación elige los listeners y construye la configuración.

```text
sandbox code ──MCP / Streamable HTTP──▶ your app, 127.0.0.1  /mcp/<server>
                                          │ registry.resolve(<server>)   → upstream URL, or error
                                          │ ContextStore.resolve()       → identity, or fail closed
                                          │ upstream rmcp client with the identity headers set
                                          ▼
                                       upstream MCP server
```

El gateway expone solo herramientas. `tools/list` y `tools/call` se reenvían al upstream; los
recursos, los prompts y las demás capacidades de MCP no se ofrecen.

## Montarlo junto al proxy {#mount-it-next-to-the-proxy}

El gateway lee la identidad de un `ContextStore` (de `synapse-context`). Comparte el almacén del
proxy, y una identidad fijada mediante el `POST /internal/bind` del proxy se aplica de inmediato
a las llamadas MCP. Este esbozo construye el proxy a partir de su archivo de configuración,
añade las rutas de administración de MCP al listener de administración del proxy y sirve el
gateway MCP en su propio listener de loopback:

```rust
use std::sync::Arc;

use synapse_mcp::{
    mcp_admin_router, mcp_gateway_router, GatewayMetrics, IdentityHeaderRule, McpGatewayConfig,
    McpRegistry,
};
use synapse_proxy::{admin::admin_router, config::Config, metrics::Metrics, ProxyBuilder};

async fn serve_mcp() -> anyhow::Result<()> {
    let built = ProxyBuilder::from_config(Config::load()?).build()?;
    let (metrics, _registry) = Metrics::new()?;

    let registry = Arc::new(McpRegistry::new());
    let config = Arc::new(McpGatewayConfig {
        inject: vec![
            IdentityHeaderRule { context_key: "org".into(), header: "x-org-id".into(), required: true },
            IdentityHeaderRule { context_key: "user".into(), header: "x-user-id".into(), required: true },
        ],
    });

    let admin = admin_router(built.context.clone()).merge(mcp_admin_router(registry.clone()));
    let gateway = mcp_gateway_router(
        registry,
        built.context.clone(),
        config,
        Some(GatewayMetrics::new(&metrics.meter())),
    );

    let admin_listener = tokio::net::TcpListener::bind(&built.admin_addr).await?;
    let mcp_listener = tokio::net::TcpListener::bind("127.0.0.1:8789").await?;
    tokio::try_join!(
        axum::serve(admin_listener, admin),
        axum::serve(mcp_listener, gateway),
    )?;
    Ok(())
}
```

Un servicio real también sirve los listeners de plano de datos y de métricas del proxy, y
exporta el `Registry` que devuelve `Metrics::new`; el
[`main.rs`](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-proxy/src/main.rs)
del proxy muestra cómo. Pasa `None` en lugar de `GatewayMetrics` para no registrar métricas.

## Más información {#learn-more}

- [Registro](./registration.md): añade y elimina servidores MCP upstream.
- [Inyección de identidad](./identity-injection.md): qué encabezados se envían al upstream y
  cuándo una llamada falla de forma cerrada.
- [Seguridad](./security.md): las comprobaciones de loopback y de `Host`, y lo que revelan los
  errores.
- [Hoja de ruta](./roadmap.md): lo que todavía no está soportado.
- Las métricas del gateway están en el
  [catálogo de métricas](../../reference/metrics-catalogue.md#synapse-mcp), y la API está en
  [docs.rs](https://docs.rs/synapse-mcp).
