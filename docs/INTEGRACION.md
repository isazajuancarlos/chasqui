<!-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas -->
# Cómo se integra Chasqui con lo que ya tiene el cliente

Pregunta de Juan, 2026-08-01: *«¿cómo se integra a aplicaciones web y móviles y
otras que requiera el cliente, como los ejemplos de Workana?»*

La respuesta corta es que **son cuatro costuras distintas**, y confundirlas es el
error que convierte un producto en un proyecto a medida. Tres de las cinco
peticiones de Workana piden algo de aquí, y ninguna pide lo mismo:

| Petición | Lo que pide de verdad | Costura |
|---|---|---|
| #1 «integrarse con nuestras operaciones actuales», toma de pedidos y precios | leer **sus datos** | 2 |
| #3 WhatsApp, Instagram y Facebook | **más canales** | 1 |
| #5 «escalar y añadir funcionalidades» | que no haya que reescribirlo | las cuatro |

---

## Costura 1 — El canal: **dónde** ocurre la conversación

Ya está resuelta, y es la razón por la que `AdaptadorCanal` existe desde el
primer día aunque hoy solo se venda WhatsApp.

Una web y una app móvil **no son un caso especial: son un canal más**. Lo que
cambia de WhatsApp a un chat en la web es la traducción del mensaje que entra y
la forma de enviarlo — exactamente lo que ese trait aísla. Debajo, ni el aviso de
privacidad, ni la base de conocimiento, ni el escalado a una persona, ni la
ventana de conversación cambian una línea.

Lo que costaría cada uno:

- **Chat en la web del cliente** — un adaptador `web` + un endpoint (`POST` para
  mandar, SSE para recibir) + un `<script>` que el cliente pega en su sitio. La
  autenticación **no** puede ser la firma HMAC de Meta: ahí el emisor es un
  navegador y el secreto sería público. Va con clave de sitio + lista de dominios
  permitidos, configurada por el cliente.
- **App móvil del cliente** — la misma API del canal web con una clave de app.
  No hay adaptador nuevo: una app y una web son el mismo canal con otro cliente.
- **Otro mensajero (Telegram, correo, SMS)** — un adaptador y ya.

**Determinación**: no se construye todavía. Ninguna de las cinco peticiones pide
un chat web, y construir para el caso que nadie pidió es la sobrecorrección de la
directiva 19. Lo que sí estaba bien era dejar la interfaz puesta: cuando un
cliente lo pida, es un archivo, no un rediseño.

## Costura 2 — Los datos del cliente: que el bot responda con **su** información

Esto es lo que pide la petición #1 y es lo que de verdad separa un bot que
impresiona en la demostración de uno que sirve: contestar «su pedido 1234 sale
mañana» exige mirar el sistema del cliente.

**No es un canal, es una fuente.** Y no se resuelve programando la integración de
cada cliente —eso es volver a Workana, un proyecto desde cero por cliente—, sino
con **acciones HTTP configurables**: el cliente declara en el panel

- los disparadores (igual que en una pregunta frecuente),
- qué datos capturar de la conversación (`numero_pedido`),
- la URL de su API, con esos datos interpolados,
- sus credenciales, cifradas en reposo **igual que el token de Meta**,
- y la plantilla de respuesta, con los campos de la respuesta JSON interpolados.

Así, «¿dónde está mi pedido 1234?» se convierte en un `GET
https://cliente/api/pedidos/1234` y en «Su pedido va en camino, llega el 3 de
agosto», **sin una línea de código nuestra**. Y el cliente que no tenga API no
configura nada y no pierde nada.

Tres cosas que hay que acotar antes de escribir esto, porque son las que salen
caras:

1. **Tiempo de espera corto y un solo intento.** El manejador del webhook está
   esperando; si la API del cliente tarda 30 s, Meta reintenta por encima y el
   cliente recibe la respuesta cuatro veces.
2. **Si la API del cliente falla, se escala a una persona** — no se inventa una
   respuesta. Es la directiva 20 aplicada al dato ajeno.
3. **Las credenciales del cliente hacia su propio sistema son datos suyos**: se
   cifran con la misma bóveda y nunca salen en un log ni en el panel.

**Determinación**: era la siguiente pieza de valor real, por encima del panel
Angular. **Construida el 2026-08-01.** Cómo se configura:

```json
[{
  "titulo": "Estado del pedido",
  "disparadores": ["mi pedido", "estado del pedido"],
  "tipo_dato": "numero",
  "pregunta": "¿Me confirma el número de pedido?",
  "metodo": "GET",
  "url": "https://cliente.example/api/pedidos/{dato}",
  "cabeceras": "Authorization: Bearer TOKEN-DEL-CLIENTE",
  "plantilla_respuesta": "Su pedido está {estado} y llega el {envio.fecha}."
}]
```

```bash
chasqui consulta cargar consultas.json   # las cabeceras se cifran al guardarlas
chasqui consulta listar                  # nunca imprime credenciales
```

Las decisiones que se tomaron por el camino, con su motivo:

- **El dato se captura por TIPO, no por expresión regular.** Tres tipos:
  `numero`, `correo`, `codigo`. Una expresión regular en un cuadro de un panel es
  código sin revisar, escrito por quien no programa, corriendo en el camino de
  cada mensaje.
- **Gana el candidato más largo, y un empate se pregunta.** En «2 unidades del
  pedido 1234», coger el primer número daría 2 y el cliente recibiría el estado
  de un pedido ajeno sin que nada fallara. Con dos números distintos no se
  escoge: se pregunta.
- **Se pregunta UNA vez por el dato que falta.** A la segunda va a una persona:
  preguntar otra vez es un bucle, y un bot que repite la misma pregunta es peor
  que uno que no sabe. La espera se guarda en la conversación, no en memoria, así
  que el número que llega tres minutos después sirve aunque el servidor se haya
  reiniciado.
- **Consultas y preguntas frecuentes compiten con la misma regla**: gana el
  disparador más largo. El cliente solo tiene que entender una regla para saber
  por qué el bot contestó lo que contestó.
- **Solo `GET` y `POST`.** Un `DELETE` configurable desde un panel es una forma
  muy cara de que un mensaje de WhatsApp borre un pedido.
- **Sin redirecciones.** Una API que contesta 302 se llevaría con ella la
  cabecera `Authorization` del cliente a un servidor que él no configuró.
- **Un dato con dos puntos seguidos no se consulta.** La barra ya va escapada,
  pero el punto no —un correo los lleva—, así que un `..` sobreviviría hasta un
  servidor que normalice después de decodificar. Lo escribió una prueba al
  ponerse en rojo.
- **Si falta un campo de la plantilla, no se manda nada.** «Su pedido está y
  llega el » parece que funcionó, y así nadie se entera de que la API del cliente
  cambió.

## Costura 3 — Avisar al sistema del cliente cuando pasa algo

Al revés que la anterior: un pedido tomado, una conversación escalada, un
contacto nuevo. El cliente configura una URL suya y Chasqui le manda el evento
firmado con HMAC (lo mismo que Meta hace con nosotros, y por lo mismo).

Con eso el bot entra en su CRM, su ERP o su correo sin que nosotros sepamos cuál
es.

## Costura 4 — La API de Chasqui, para que sus aplicaciones lo consuman

El panel Angular que va embebido en el binario va a hablar con una API REST:
conversaciones, historial, cola de escalados, ajustes, base de conocimiento.
**Esa misma API, con un token, es la integración para las aplicaciones del
cliente**: un agente humano puede atender desde la app interna del negocio en vez
de desde nuestro panel.

Es decir: el panel no es una pantalla con acceso privilegiado a la base, es el
primer consumidor de la API. Si se hace al revés, la integración exige después un
segundo sistema.

**Construida el 2026-08-01.** La API vive en `/api/*`, exige sesión —el
extractor va en la firma de cada manejador, así que una ruta nueva sin sesión no
compila— y el panel Angular no llama a nada que la aplicación del cliente no
pueda llamar con las mismas credenciales. Lo que hay:

| Ruta | Para qué |
|---|---|
| `POST/DELETE /api/sesion`, `GET /api/yo` | entrar, salir, saber quién es |
| `GET /api/escalados`, `POST /api/escalados/{id}/atender` | la cola de quien espera |
| `GET /api/conversaciones`, `GET /api/conversaciones/{id}` | lista e historial |
| `POST /api/conversaciones/{id}/mensaje` | **que una persona responda** |
| `POST /api/conversaciones/{id}/devolver-al-bot` | terminar la atención humana |
| `GET/PUT /api/ajustes` | el aviso de privacidad y los textos |
| `GET/POST /api/conocimiento`, `PATCH /api/conocimiento/{id}` | preguntas frecuentes |
| `GET /api/consultas`, `PATCH /api/consultas/{id}` | las de la costura 2 |
| `GET /api/cuentas` | los canales dados de alta, **sin tokens** |

Las decisiones que se tomaron, con su motivo:

- **La sesión se firma con una clave DERIVADA de `CHASQUI_FRASE`**, no con otra
  variable de entorno. Quien tiene la frase ya puede descifrar los tokens de
  Meta: pedir dos secretos para el mismo nivel de acceso es ceremonia que acaba
  en dos secretos débiles. La etiqueta de dominio evita que la clave de sesión
  sea *igual* a la frase.
- **Cookie `HttpOnly`, `SameSite=Strict` y `Secure`.** Quitar `Secure` hay que
  pedirlo (`CHASQUI_SIN_TLS=1`): el defecto es el seguro.
- **Freno al ensayo de contraseñas** con `guaca::freno`, por par (correo,
  origen) y leyendo `X-Real-IP` y nunca `X-Forwarded-For`, que lo escribe quien
  llama. Un 429 con `Retry-After`.
- **«No existe» y «contraseña mal» dan la misma respuesta.** Distinguirlas
  convierte la pantalla de entrada en un comprobador de quién tiene cuenta.
- **Los tokens no salen por la API, nunca.** De las consultas se dice si tienen
  credenciales configuradas —«no responde» y «no tiene credenciales» son dos
  problemas distintos— pero no cuáles.
- **El alta de una cuenta de Meta NO está en el panel**, a propósito: el token
  permanente del cliente no tiene por qué pasar por un navegador. Se hace desde
  el servidor con `chasqui cuenta añadir`.

---

## Lo que hay que decirle al cliente, y que ningún competidor dice

**Todo esto es configuración, no desarrollo.** Ese es el producto: en Workana,
cada una de esas integraciones es un proyecto a medida que alguien construye
desde cero y que solo sirve para ese cliente. Aquí es el mismo binario con otra
fila en la base.

Y la contrapartida honesta, que va por escrito en la propuesta: **lo que se
integra es lo que el cliente pueda exponer**. Si su sistema no tiene API, la
integración no existe por mucho que se pague — lo que hay entonces es una base de
conocimiento que él mantiene, que es un producto legítimo y otro precio.

---

## Estado

| Costura | Hoy |
|---|---|
| 1 · Canal (WhatsApp, Instagram, Messenger) | **hecha**, con los tres adaptadores |
| 1 · Canal web / móvil | interfaz lista, adaptador **sin construir** (nadie lo ha pedido) |
| 2 · Leer los datos del cliente | **hecha** — `chasqui consulta cargar` |
| 3 · Avisar a su sistema | sin construir |
| 4 · API propia + panel | **hecha** — `/api/*` con sesión, y el panel dentro del binario |
