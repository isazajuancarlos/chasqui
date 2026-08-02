# Chasqui

> **Configuración general: `~/.claude/CLAUDE.md`** — reglas que no se rompen,
> las directivas numeradas y las máquinas montadas. Se carga en toda sesión y
> **no se copia aquí**; este archivo lleva SOLO lo propio de Chasqui.
>
> **Esta instancia se arranca desde esta carpeta**: `cd /mnt/data/chasqui && claude`.
> Desde el 2026-08-01 no se trabajan varios proyectos desde una sola sesión.

Plataforma de atención automatizada por WhatsApp, Instagram y Messenger.
**Proyecto de VENTA: repositorio privado, licencia propietaria** (directiva 10).

*Chasqui*: los mensajeros relevistas del Tahuantinsuyo, que recorrían los caminos
llevando los mensajes — y los quipus.

El plan completo está en `PLAN.md`, y sale de cinco peticiones reales de Workana
guardadas en `docs/peticiones-workana.txt`. **No se decide nada de alcance sin
mirar ese archivo**: es la única evidencia de lo que el mercado pide, y evita
construir lo que a uno le parece.

## Reglas que no se rompen

- **La plataforma es nuestra, las credenciales son del cliente.** Él pone su
  token de Meta, su clave del modelo y su catálogo. Eso hace que la verificación
  de negocio de Meta la pase él —tiene NIT— y que el coste por conversación lo
  vea y lo controle. El negocio es licencia, no reventa de infraestructura.
- **Chasqui NO se instala en nuestro VPS.** Se vende por Workana y corre en el
  servidor del cliente. Alojarlo nosotros rompe las dos cosas que sostienen el
  precio: deja de ser cierto que las conversaciones no salen de su servidor, y
  nos convierte en **encargados del tratamiento** de datos personales ajenos
  (Ley 1581) —responsabilidad que hoy no tenemos y que nadie está pagando—.
  La única puerta es un **servicio de cobro mensual** decidido a propósito, y
  ese día hay que rehacer el modelo legal (contrato de encargo) antes que el
  despliegue. *Escrito el 2026-08-01, después de proponer justo lo contrario:
  el despliegue del producto son PLANTILLAS que se entregan, no una máquina
  nuestra.* Para probar contra Meta está `./probar-con-meta.sh`, que levanta un
  túnel efímero con base en `/tmp`.
- **Nunca APIs no oficiales de WhatsApp** (Baileys, `whatsapp-web.js`). Meta
  cierra el número del cliente, y el proveedor que lo montó es quien responde.
  Se rechaza por escrito en la propuesta, no se calla.
- **El escalado a una persona va en el MVP.** Un bot sin salida a humano frustra
  y lo apagan a las dos semanas.
- **Aviso de privacidad en el primer mensaje.** El bot trata datos personales
  (Ley 1581) y el cliente es el responsable del tratamiento. Que las
  conversaciones no salgan de su servidor es la razón por la que Chasqui puede
  cobrar más que el freelancer de al lado — no es un extra.
- **El canal es una interfaz desde el primer día**, aunque solo haya una
  implementación. WhatsApp, Instagram y Messenger comparten consola y modelo de
  webhook en Meta: los tres son un adaptador más, no triple trabajo. Ya hay una
  petición que los pide. **El cuarto, `web`, no es de Meta**: no llega por
  webhook, no hay firma, y la respuesta la sondea el navegador — por eso
  `Canal::objeto_meta` devuelve `Option` y `/webhook/{canal}` solo acepta los de
  `Canal::DE_META`.
- **Ni una columna `cliente_id` en ninguna tabla.** El aislamiento entre clientes
  es que son máquinas distintas, y esa es justo la frase que se le vende. Una
  base por instancia; el día que aparezca esa columna, la frase deja de ser
  cierta.
- **Un secreto ausente no tiene valor por defecto: el binario no arranca**
  (directiva 20). Sin `CHASQUI_SECRETO_APP` el servidor acepta webhooks
  falsificados y *funciona*, que es peor que caerse. Lo mismo con el aviso de
  privacidad y el plazo de retención, que los tiene que decidir el responsable
  del tratamiento: sin ellos no se atiende y no se borra, y se dice.

## Stack

Rust (`axum` + `SQLx`/SQLite) con Angular Material embebido en el binario
(directiva 26). Un ejecutable en el servidor del cliente, una base por instancia.
Angular 22 + `@angular/material`; **PrimeNG prohibido** (ver general).

## Construir y probar

```bash
# La interfaz primero: es un artefacto de npm que NO viaja en el repositorio, y
# `build.rs` la empotra desde interfaz/dist/interfaz/browser.
cd interfaz && npm install && npm run build && cd ..

cargo build --release           # el binario `chasqui`, con el panel dentro
cargo test --workspace --no-fail-fast
```

- **`--no-fail-fast` no es opcional**: sin él, `cargo test` aborta en el primer
  binario que falla y las pruebas de los otros crates NO llegan a ejecutarse —
  un rojo que además esconde el estado del resto.
- Una sola prueba:
  `cargo test -p chasqui-api --test frontera_meta -- <nombre> --nocapture`.
  Por crate: `cargo test -p chasqui-dominio` (rápido, no toca disco ni red).
- La suite entera tarda **un par de minutos en debug y no está colgada**: casi
  todo es `chasqui-datos`, donde cada contraseña pasa por Argon2 y en debug eso
  va más de diez veces más lento que en release.
- Si se cambia la interfaz **hay que reconstruirla y volver a compilar**: lo
  empotrado es `dist/`, no `src/`. `build.rs` observa esa carpeta, y si no está
  empotra una página que dice qué ejecutar — nunca una en blanco.
- `./probar-con-meta.sh` levanta el binario tras un túnel público de Cloudflare y
  comprueba el apretón de manos con la consola de Meta (alta del webhook, reto,
  firma HMAC sobreviviendo a un proxy). Crea su propia base temporal y la borra.
  Lo que falta después está en `docs/META.md`.
- `interfaz/npm test` está cableado a vitest pero **hoy no hay ni un `.spec.ts`**:
  correrlo da cero pruebas, que no es lo mismo que verde.

## Arquitectura

Tres crates, y la frontera entre ellos es la decisión de diseño principal:

| Crate | Qué lleva | Qué NO puede entrar |
|---|---|---|
| `chasqui-dominio` | los tipos y **las reglas**: cuándo sale el aviso, cuándo calla el bot, cuándo se llama a una persona | base de datos, red |
| `chasqui-datos` | lo que toca el disco: SQLite, migraciones empotradas, bóveda de credenciales | decisiones sobre qué se responde |
| `chasqui-api` | `axum` + la CLI + el panel empotrado | lógica de atención |

**El camino de un mensaje** (`chasqui-api/src/atender.rs`): buscar la cuenta →
registrar el entrante (ANTES de responder: es lo que hace que el reintento de
Meta no conteste dos veces) → `chasqui_dominio::atender` decide → el adaptador
envía. Si aparece un `if` sobre el contenido del mensaje en `atender.rs`, está en
el sitio equivocado: esa regla va al dominio, donde se prueba sin levantar nada.

**Canales** (`chasqui-api/src/canales/`): el rasgo `AdaptadorCanal` solo traduce
el cuerpo del webhook a `MensajeEntrante` y envía texto. Dos implementaciones
cubren los tres canales — `whatsapp.rs` y `meta_mensajeria.rs` (Instagram y
Messenger comparten forma). Los tres se montan siempre en `Servicio::nuevo`: que
una cuenta exista es cosa de la base, no del servidor.

**El panel es el PRIMER consumidor de `/api`, no una pantalla con acceso
privilegiado a la base** (`docs/INTEGRACION.md`, costura 4). Es lo que convierte
«integrar el panel con la aplicación del cliente» en darle un token. Toda ruta de
`/api` exige sesión por el extractor `Agente` en la firma del manejador, así que
**una ruta nueva sin sesión no compila**. En `lib.rs::router`, el `fallback`
estático va el ÚLTIMO o taparía la API; y cae a `index.html` para que recargar
`/conversacion/7` en el navegador no dé 404.

**Sesión**: `guaca::sesion`, con el secreto derivado de `CHASQUI_FRASE` por
HMAC-SHA256 con etiqueta de dominio — no es un defecto inventado, es un cálculo
determinista sobre un secreto que ya es obligatorio. Cookie `HttpOnly`, `Secure`
salvo `CHASQUI_SIN_TLS=1`.

**Cifrado en reposo** de las credenciales del cliente: `guaca`, fijado en
`Cargo.toml` **por commit inmutable y no por tag**, y es el mismo `rev` que usa
`medico`.

**Migraciones**: `crates/chasqui-datos/migraciones/*.sql`, empotradas con
`sqlx::migrate!` y aplicadas al abrir. Hacen falta las features `migrate` **y**
`macros` de SQLx: con solo `migrate` compila el motor y no el macro, y el error
no lo dice.

**La CLI y el servidor son el mismo binario** (`chasqui-api/src/main.rs`):
`servir`, `cuenta`, `ajuste`, `conocimiento`, `consulta`, `flujo`, `pedidos`,
`documento`, `modelo`, `usuario`, `escalados`, `purgar`, `olvidar`, `respaldar`,
`restaurar`. Las credenciales entran por la
entrada estándar, nunca por argumento: los argumentos quedan en el historial y en
`ps`. El README lleva la secuencia completa de instalación.

**Las pruebas que valen son las que cruzan la frontera** (directiva 28):
`tests/frontera_meta.rs` y `tests/frontera_panel.rs` arrancan **el binario de
verdad**, le dan de alta cuentas por su línea de órdenes y le hablan por HTTP con
un sustituto de la API Graph en localhost. Todo lo demás va de estructura a
estructura y mide el códec, no el producto.

## Estado

Construido el 2026-08-01. **El avance real lo lleva el `README.md`**; `PLAN.md`
es el plan y `docs/META.md` lo que falta para probar contra Meta de verdad.

El MVP está completo: flujos de varios pasos y toma de pedidos entraron el
2026-08-01 (commits `74b3d90`, `f27ca89`), y el canal web con su widget el mismo
día. Lo único que queda del alcance es la **entrega del pedido al sistema del
cliente por webhook saliente**, y está *decidido que no se construye hasta que
aparezca el cliente que ya tenga ese sistema* (`docs/INTEGRACION.md`, costura 3):
ninguna de las cinco peticiones lo nombra, y un webhook hacia una URL que nadie
configura es código tras una bandera que nadie enciende. El pedido armado sale
hoy por el panel (`GET /api/pedidos`, `chasqui pedidos`) y llamando a una
persona.

**Probado contra Meta el 2026-08-02** (cierra #307): un «hola» escrito a mano
desde un móvil al número de la Cloud API entró por el webhook y Chasqui contestó
—aviso de privacidad primero, respuesta después—, las dos con acuse de Meta. El
camino completo y sus tres paredes están en `docs/META.md`; la del final es la
que no avisa: **la consola suscribe el CAMPO `messages` pero NO suscribe la app a
la cuenta de WhatsApp Business**, y sin eso Meta recibe el mensaje, lo enseña en
su propia consola y no lo entrega a nadie.

Lo que sigue sin medirse es el **coste por conversación**: el número de prueba va
como `non_paid_messaging`.
