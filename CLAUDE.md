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
- **`/security-review` YA ARRANCA aquí.** Hasta el 2026-08-05 no lo hacía, y no
  era un fallo suyo: construye su diff con `git diff origin/HEAD…` y este
  repositorio **no tenía remoto ni rama `main`**, así que no existía base contra
  la que diferenciar. Ese día chasqui se subió a GitHub (privado), la rama
  `desarrollo` pasó a ser `main` y el problema desapareció solo. Sigue valiendo
  la forma manual cuando se quiera fijar la base:

  ```bash
  python3 /mnt/data/conocimiento/diff_para_revisar.py /mnt/data/chasqui \
      --rama <rama> --base main --salida /tmp/revision.diff
  ```

  (Aquella nota decía «el día que exista `main`, esto se arregla solo». Existe.)

## El CI — cuatro trabajos, y por qué son cuatro

`.github/workflows/ci.yml`, desde el **2026-08-05**. Antes de esa fecha chasqui
no tenía remoto ni CI, y eso no era una decisión: **`cargo clippy -D warnings` no
lo había corrido nadie nunca**. Al correrlo salieron DOS capas de avisos, y la
segunda estaba TAPADA por la primera —clippy se para en el primer crate con
errores—. Las 410 pruebas pasaban, o sea que el repositorio parecía sano por el
único lado que alguien miraba.

| trabajo | comando exacto |
|---|---|
| `test + clippy` | `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace --no-fail-fast` |
| `cargo-audit (RustSec)` | `cargo audit` (el único *ignore* vive en `.cargo/audit.toml`) |
| `cargo-deny (supply chain)` | `cargo deny check` (config en `deny.toml`) |
| `interfaz (ng build)` | `npm ci && npm run build` en `interfaz/` |

Para reproducir el verde en local se copian **esos** comandos, no unos parecidos.
Tres cosas que no son opcionales y ya costaron algo:

- **`--all-targets`**: sin él clippy no mira el código de prueba, que es
  exactamente donde estaban los nueve avisos de la primera capa.
- **`--no-fail-fast`**: sin él `cargo test` aborta en el primer binario que falla
  y las pruebas de los otros crates no llegan a ejecutarse.
- **El trabajo `interfaz` existe porque `cargo test` NO cubre el frontal.** Lo que
  se empotra es `dist/`, y en debug `build.rs` lo sustituye por una página de
  aviso: romper la interfaz salía verde hasta que alguien construyera el release.
  **`npm test` no se corre a propósito** — está cableado a vitest y no hay ni un
  `.spec.ts`, así que daría cero pruebas, y cero no es verde, es nada.

**El `ignore` de `cargo-audit` es uno y está medido**, no copiado de otro
repositorio: RUSTSEC-2023-0071 (Marvin, `rsa` 0.9.x). `rsa` figura en el
`Cargo.lock` porque `sqlx-macros-core` declara los tres backends para el macro
`migrate!`, pero **`cargo tree --workspace -i rsa` imprime «nothing to print»** —
no lo alcanza nadie en el grafo—, y el control positivo es que el mismo comando
sí resuelve `sqlx-macros-core`. chasqui pide `sqlx` con `default-features = false`
y solo `sqlite`. cargo-audit lo marca porque lee el LOCK, no el grafo. **Deja de
ser ignorable el día que se habilite el backend `mysql`**, no en una fecha.

**Las alertas de Dependabot no vienen encendidas** y `dependabot.yml` no las
enciende: son un ajuste del repositorio. Se comprueban con
`gh api -i repos/isazajuancarlos/chasqui/vulnerability-alerts | head -1` — 204 es
encendidas, 404 apagadas. Se encendieron el 2026-08-05, el mismo día en que se
midió que los tres repositorios privados de la casa las tenían apagadas mientras
los tres públicos las tenían.

Corre en `push` **solo a `main`** y en `pull_request` contra `main`.
**Consecuencia declarada: una rama empujada sin PR no tiene CI**; el flujo es
rama → PR → merge. Es deliberado: chasqui es privado y sus minutos de Actions
salen del cupo de la cuenta.

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

**El modo demostración** (`chasqui-api/src/demo.rs`) enseña las dos mitades del
producto en una pantalla: el chat del cliente a la izquierda, lo que ve el equipo
a la derecha. Tres cosas que no son negociables y están fijadas con pruebas de
frontera:

- **Apagado no existe.** Sin el ajuste `demo.activa = si`, las seis rutas
  devuelven 404. Un cliente que compre Chasqui no gana superficie por el hecho
  de que nosotros tengamos una demostración.
- **No hay panel, hay panel DE UNA conversación.** Autoriza el mismo
  identificador opaco de 256 bits que el widget ya le dio al visitante. Enseñar
  el panel entero sería enseñarle a un desconocido lo que otro visitante acaba
  de escribir —a veces su correo, en el flujo de cotización—, y eso es una
  brecha con el dueño de la demo como responsable.
- **Publicar credenciales del panel y tener chat en vivo son incompatibles.** Se
  puede una cosa o la otra en la misma instancia, nunca las dos. **Lo impide el
  código desde el 2026-08-02**: con `demo.activa = si`, `GET /api/demo` responde
  404 diga lo que diga `demo.credenciales`, y lo dice en el log para que quien
  administra se entere de que puso dos ajustes que se excluyen. Antes la regla
  vivía solo en esta línea, o sea que la sostenía el acordarse de ella.

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

**Los archivos que manda quien escribe —fotos, notas de voz, documentos— viven
DENTRO de la base**, en la tabla `medio`, no en una carpeta al lado. La razón no
es comodidad: con archivos sueltos harían falta dos mecanismos nuevos, uno que
los borre cuando alguien ejerce su derecho de supresión y otro que los copie al
respaldar. Como filas, `olvidar`, `purgar` y `respaldar` (que es `VACUUM INTO`)
siguen siendo correctos sin tocarlos —hay pruebas que lo fijan—, y la frase que
se vende, «una base por instancia», sigue siendo literal. El precio es el tope
de tamaño (`Servicio::tope_medio`), y **lo que no cabe se anota diciendo que no
cabe**, nunca se descarta callando.

**El tope de tamaño tiene una segunda mitad: `DESCARGAS_A_LA_VEZ`** (4). Como
`atender` arranca una tarea por archivo que llega, lo que le cuesta a la memoria
del servidor es el PRODUCTO de los dos —4 × 16 MiB = 64 MiB—, no el tope solo. El
techo lo fija un `const _: () = assert!(…)` que **falla al compilar**, y eso nace
de un fallo de la prueba que lo vigilaba: `frontera_medios` leía la constante en
los dos lados del `assert`, así que subirla a 1000 dejaba la prueba en verde con
un techo de 16 GiB. Una prueba que se mide contra su propio parámetro no mide
nada.

**Las coordenadas se traducen a «municipio, departamento, país» DENTRO del
binario** (`chasqui-dominio/src/geo.rs`), con una tabla de GeoNames `cities5000`
empotrada (1,7 MB, 69 561 ciudades). Nunca se le pregunta a un servicio de mapas:
esa consulta llevaría la ubicación exacta del cliente del cliente a un tercero, y
**reducir lo que se muestra no reduce lo que se transmite**. El nivel se eligió
midiendo —con `cities15000` un punto real de Amagá salía como «Caldas», otro
municipio— y la etiqueta lleva su distancia porque el error mediano para un
pueblo pequeño es de 10 km. Se regenera con `herramientas/geo_compacto.py`; el
`.bin` **sí se versiona**, para que compilar no necesite red. Atribución CC BY en
`NOTICE`: es obligatoria.

Dos cosas del camino que no se ven leyendo:

- **La descarga NO va en el camino del webhook.** Meta reintenta si no recibe el
  200 en segundos; bajar allí un vídeo haría que el mismo mensaje se atendiera
  dos y tres veces. La fila nace `pendiente` y otra tarea la completa. **Y al
  arrancar, `atender::reintentar_pendientes` retoma lo que quedó a medias**: sin
  eso, un reinicio a mitad de descarga dejaba la fila en `pendiente` para
  siempre. Vale para los tres canales y no hace caso especial por canal: en
  WhatsApp el billete se vuelve a resolver y el archivo se recupera; en Instagram
  y Messenger la URL ya caducó y el reintento falla — que es justo lo que se
  busca, porque cambia un `pendiente` que miente por un `fallido` que dice qué
  pasó. Lo fija `frontera_medios`, matando el binario a mitad de descarga.
- **`GET /api/medios/{id}` sirve un archivo que eligió un desconocido**, tipo
  incluido. Solo va `inline` lo que es imagen, audio o vídeo; lo demás se
  descarga, y todo lleva `Content-Security-Policy: sandbox`. Sin eso, un cliente
  manda un `.html` y su JavaScript corre en el origen del panel con la sesión
  del agente.

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
