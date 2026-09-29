---
sidebar_position: 4
title: Seguridad de MCP
description: Las protecciones en las que se apoya synapse-mcp, desde los listeners de loopback y las comprobaciones de Host hasta la identidad que falla de forma cerrada y lo que revelan los errores.
---

`synapse-mcp` está en una frontera de confianza: código de sandbox a un lado, servidores MCP
internos e identidad de tenant al otro. Estas son las protecciones que ofrece y las que debes
aportar tú al montarlo.

## Escuchar solo en loopback {#listen-on-loopback-only}

Enlaza el router del gateway a `127.0.0.1` o `::1`, para que solo los procesos del mismo host,
contenedor o pod puedan alcanzarlo. El crate no elige la dirección; la elige tu aplicación.

## Comprobación del encabezado Host {#host-header-check}

El gateway mantiene activada la protección contra DNS rebinding de `rmcp`. Una petición cuyo
encabezado `Host` no sea `localhost`, `127.0.0.1` o `::1`, en cualquier puerto, se rechaza con
`403` antes de llegar al gateway.
Esto impide que una página web alcance el gateway mediante un nombre de host que resuelve a
loopback. También significa que los clientes deben dirigirse al gateway con uno de esos nombres;
un Service de Kubernetes o la IP de un pod no funcionarán.

## La identidad falla de forma cerrada {#identity-fails-closed}

Una llamada cuyas claves de identidad `required` no estén todas fijadas se rechaza antes de
contactar con ningún upstream, y los encabezados de identidad que envía el cliente nunca se
reenvían. Consulta [Inyección de identidad](./identity-injection.md).

## Superficies de control sin autenticación {#unauthenticated-control-surfaces}

Las rutas de administración (`/internal/mcp/servers`) y el `/internal/bind` del proxy no tienen
autenticación. Quien pueda llamarlas puede apuntar un nombre de servidor a cualquier URL, o fijar
cualquier identidad. Asegúrate de que el código que estás aislando no pueda alcanzarlas: si el
código del sandbox comparte una interfaz de loopback con el listener de administración, puede
volver a fijar su propia identidad.

## Qué revelan los errores {#what-errors-reveal}

- Las llamadas a servidores desconocidos o caducados fallan con
  `unknown or expired mcp server '<name>'`.
- Los fallos al conectar con un upstream se registran en el log como avisos con los detalles,
  incluida la URL, y el cliente solo recibe `upstream MCP server unavailable`.
- Cada `tools/list` y `tools/call`, incluida la conexión con el upstream, está limitado a 30
  segundos. Una llamada que dura más falla con `upstream MCP call timed out`.
- Los errores que devuelve el upstream en una conexión ya establecida, para `tools/list` o
  `tools/call`, se pasan al cliente como texto del mensaje de error. Asegúrate de que tus
  servidores upstream no incluyan secretos en los mensajes de error.

## Relacionado {#related}

- [Seguridad](../../operating/security.md) del gateway.
- [Listeners y endpoints](../proxy/listeners-and-endpoints.md) para el listener de
  administración del proxy.
