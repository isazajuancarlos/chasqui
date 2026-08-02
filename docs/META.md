<!-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas -->
# Probar Chasqui contra Meta de verdad

> ## HECHO — 2026-08-02, 11:10
>
> **Un WhatsApp de verdad entró y Chasqui contestó de verdad.** Desde el móvil de
> Juan al número de prueba `+1 555 670 5915`, con el binario de release detrás de
> un túnel de Cloudflare:
>
> ```
> 16:10:23  →  entrante   «hola»
> 16:10:26  ←  saliente   el aviso de privacidad
> 16:10:27  ←  saliente   «Hola. Esto es una prueba de Chasqui y funciona.»
> ```
>
> Los dos salientes traen `wamid` de acuse de Meta, así que no es que se
> intentaran: se entregaron. En la base quedaron 1 conversación y 3 mensajes, y
> la conversación en estado `bot`.
>
> Esto es lo que este documento existía para conseguir. **Lo de abajo se conserva
> como el manual del camino**, porque el cliente lo va a recorrer entero con su
> propio NIT y su propia app.

**El riesgo número uno antes de vender nada.** Todas las pruebas del repositorio
hablan con un Meta de mentira en localhost. Miden el cableado y el códec, no el
producto contra el mundo. Hasta que un mensaje de WhatsApp real entre y salga, no
se compromete una fecha de entrega con nadie.

No hace falta ni cliente ni dinero: la consola de desarrollador de Meta da un
número de prueba gratuito. **Sí hace falta la verificación del negocio**, y eso
es un cambio: hasta el 2026-08-02 aquí ponía que no, y era falso — ver
«El muro real» más abajo.

---

## Lo que YA está comprobado, y con qué

Ejecutado el 2026-08-01 con un túnel público de Cloudflare —internet de verdad,
TLS de verdad, un proxy en medio—:

| Comprobación | Resultado |
|---|---|
| El servidor responde por HTTPS desde fuera | ✅ 200 |
| El alta del webhook devuelve el reto **tal cual, en texto plano** | ✅ |
| El alta con el token equivocado | ✅ 403 |
| Un webhook firmado: **la firma HMAC sobrevive al proxy** | ✅ 200 |
| El mismo cuerpo sin firma | ✅ 403 |
| Firmado con otro secreto | ✅ 403 |

La cuarta es la que valía la pena. Si el proxy tocara un solo byte del cuerpo
—una reserialización, un cambio de codificación—, la firma dejaría de casar y
**nada funcionaría sin que nada lo explicara**. En local es invisible: no hay
proxy. Ahora se sabe que sobrevive.

Se reproduce entero con:

```bash
./probar-con-meta.sh
```

## Lo que NO está comprobado, y solo puede hacerlo Juan

Todo lo que exige una cuenta de Meta:

1. **Que el cuerpo que manda Meta sea el que este código sabe leer.** El
   traductor está escrito contra la forma documentada; documentada y real no
   siempre coinciden, y los campos que sobran nunca aparecen en la documentación.
2. **Que el envío funcione con un token real** y qué errores devuelve de verdad
   cuando algo va mal.
3. **La ventana de 24 horas y el cobro por conversación**, tal como Meta los
   aplica.
4. **El alta del webhook en la consola**, que es donde se descubre si falta un
   permiso.

---

## Los pasos

### 0. El muro real: la verificación del negocio — **superado el 2026-08-02**

> **Ya está.** La verificación se aprobó el **mismo día** en que se envió:
> Centro de seguridad → «Verificación para ISAZA ARENAS JUAN CARLOS · Verificado
> originalmente el Aug 02, 2026 · ✅ Verificada», y en Cuentas de WhatsApp la
> cuenta pasó a «Verificación del negocio: Verificado / Estado de la cuenta:
> Aprobada». Meta anunciaba **dos días laborables** y tardó horas — la espera que
> este documento daba por segura (veredicto hacia el 2026-08-05) no ocurrió.
> Acto seguido, «Solicitar número de prueba» dejó de fallar y entregó el número.
> **Lo que sigue en esta sección se conserva porque es lo que verá el cliente**:
> él tendrá que pasar el mismo muro con su NIT.

**Medido el 2026-08-02 con la cuenta de Juan.** El número de prueba gratuito NO
se entrega si el portafolio comercial no está verificado. La consola lo dice en
el Administrador de WhatsApp, sobre la propia cuenta de prueba:

> **Cuenta restringida.** Para poder empezar a enviar mensajes, tendrás que
> verificar tu negocio.

Y en el asistente de la app, al pulsar «Solicitar número de prueba»:

> No puedes continuar con esta operación porque tu cuenta de WhatsApp Business
> está restringida.

Las dos cuentas de WhatsApp del portafolio Xiliux salieron restringidas —la
«Test WhatsApp Business Account» incluida—, así que no se arregla eligiendo
otra: es el **negocio**, no la cuenta. La verificación se lanza desde
Configuración → Información del negocio → **Verificación del negocio → Iniciar
verificación**, y pide documento legal a nombre de quien se verifica.

Esto no cambia el modelo de venta —el cliente pone SUS credenciales y es él
quien pasa la verificación con su NIT—, pero sí quiere decir que **la prueba
punta a punta de Chasqui espera a que Juan verifique su propio negocio**.

Cómo se envía, con lo que no dice la ayuda de Meta:

- Está en **Configuración → Seguridad e idoneidad → Centro de seguridad →
  Verificación del negocio**, no en «Información del negocio».
- Hay que **elegir el caso de uso**, y para esto es «La app requiere acceso a los
  permisos en Meta for Developers». Los otros piden cosas distintas.
- **Se verifica una persona o empresa REAL, con su documento.** Enviada el
  2026-08-02 a nombre de *Isaza Arenas Juan Carlos* —persona natural—, porque
  Xiliux es una marca y no una persona jurídica: verificar a nombre de la marca
  es el camino al rechazo.
- Meta declara **dos días laborables** de revisión. Enviada el domingo 2026-08-02,
  el veredicto se espera hacia el **2026-08-05**.

### 1. Crear la app en Meta (una vez)

El flujo cambió: ya no se elige «Tipo: Negocio» y se añade el producto WhatsApp
a mano. **El tipo lo fija el caso de uso, y no se puede convertir después**: una
app de «Tipo: Ninguno» no ofrece WhatsApp entre sus productos, y en Configuración
→ Básica no hay ningún control para cambiarlo. Comprobado el 2026-08-02: una app
de tipo Ninguno lista 11 productos y ninguno es WhatsApp.

1. `developers.facebook.com` → **Mis apps** → **Crear app**.
2. Nombre: **no puede contener «whatsapp»** ni ninguna marca de Meta; lo rechaza
   con un aviso rojo.
3. Casos de uso → **«Conectarte con los clientes a través de WhatsApp»**.
4. Negocio: seleccionar el **portafolio comercial** (lo exige este caso de uso).
5. Meta pide **la contraseña de la cuenta** antes de crearla.

Con eso la app queda con el producto WhatsApp y la sección «Casos de uso →
Personalizar → Paso 1. Pruébalo», que es donde se pide el número de prueba.

Meta da entonces:

- un **número de prueba** con un `phone_number_id`,
- un **token temporal** (24 h; para probar sobra),
- y hasta **5 números de destino** que hay que verificar con un código. **Ponga
  el suyo**: es al que le va a escribir el bot.

Entregado el 2026-08-02, ya con el negocio verificado, para la app
`chasqui-atencion` (`1033169316303198`):

| Dato | Valor |
|---|---|
| Número de prueba | `+1 (555) 670-5915` |
| `phone_number_id` | `1221911424344798` |
| WhatsApp Business Account ID | `1903916413906447` |

El **token no se escribe aquí**: dura 24 h y se copia de la consola en el
momento de probar. El destinatario todavía está sin registrar — ese es el
siguiente clic, y pide un código al teléfono de Juan.

### 2. Levantar Chasqui con los secretos de la app

El secreto de la app está en **Configuración → Básica → Clave secreta de la app**.
El token de verificación lo inventa usted: es una contraseña compartida entre
Meta y Chasqui.

```bash
export CHASQUI_SECRETO_APP="<la clave secreta de la app>"
export CHASQUI_TOKEN_VERIFICACION="<invente una cadena larga>"
export CHASQUI_PHONE_NUMBER_ID="<el del número de prueba>"
export CHASQUI_TOKEN_META="<el token temporal>"

./probar-con-meta.sh
```

Con `CHASQUI_PHONE_NUMBER_ID` y `CHASQUI_TOKEN_META` puestos, el guion da de alta
la cuenta y carga una pregunta frecuente, así que **el bot contesta de verdad**.
Sin ellos solo se prueba el túnel.

El guion imprime la URL pública que hay que pegar en Meta.

### 3. Dar de alta el webhook

En **WhatsApp → Configuración → Webhooks → Editar**:

- **URL de devolución de llamada**: la que imprimió el guion, terminada en
  `/webhook/whatsapp`.
- **Token de verificación**: el mismo `CHASQUI_TOKEN_VERIFICACION`.

Meta llama en ese momento y espera el reto de vuelta. Si dice «no se pudo
verificar», mire el registro del servidor: dice exactamente qué pasó.

Después, **suscríbase al campo `messages`**. Sin eso el webhook queda dado de alta
y no llega nada — y no hay ningún error que lo diga.

**Y eso todavía no basta. Son DOS suscripciones, no una** (medido el 2026-08-02,
y costó dos mensajes de prueba perdidos):

1. la del **campo** `messages`, que se pone en la consola — hecha eso, la consola
   ya enseña los mensajes en «Revisa los webhooks de prueba»;
2. la de **la app a la cuenta de WhatsApp Business**, que la consola **NO hace**
   y no menciona en ninguna pantalla.

Sin la segunda, Meta recibe el mensaje, lo guarda, lo enseña en su propia
consola… y no lo entrega a nadie. El síntoma es exactamente «no llega nada» con
todo en verde, que es el peor de los diagnósticos. Se comprueba y se arregla por
la API, con el token del número de prueba:

```bash
# Quién está suscrito HOY. Si solo sale «WA DevX Webhook Events 1P App»,
# que es de Meta, su app no está y por eso no le llega nada.
curl -s -H "Authorization: Bearer $CHASQUI_TOKEN_META" \
  "https://graph.facebook.com/v23.0/<WABA_ID>/subscribed_apps"

curl -s -X POST -H "Authorization: Bearer $CHASQUI_TOKEN_META" \
  "https://graph.facebook.com/v23.0/<WABA_ID>/subscribed_apps"
```

**No se da por buena la respuesta `{"success":true}`**: se vuelve a leer la lista
y se comprueba que la app aparece por su nombre.

### 4. Escribirle al número de prueba

Desde su teléfono, al número de prueba: «hola».

Lo que hay que ver, en orden:

1. En el registro del servidor, el webhook entrando.
2. En su teléfono, **dos mensajes**: el aviso de privacidad primero y la
   respuesta después.

Si llega el aviso pero no la respuesta, el problema está en la base de
conocimiento. Si no llega ninguno, en el token de envío. Si no entra el webhook,
en la suscripción al campo `messages`.

### 5. Lo que hay que anotar mientras se hace

Esto es el objetivo de todo el ejercicio: **traer de vuelta lo que la
documentación no dice.**

- ~~El **cuerpo exacto** que mandó Meta~~ — **hecho el 2026-08-02**, ver abajo.
- ~~Qué manda al escribir una **nota de voz** y una **foto**~~ — **hecho**: los
  tipos reales son `audio`, `image` y `location`, y sí coinciden con la
  documentación (que no era de dar por hecho, viendo que los campos del cuerpo no
  coincidían).
- ~~La DESCARGA de un medio contra Meta de verdad~~ — **hecha el 2026-08-02, ver
  abajo**.
- ~~Cuánto tarda el envío de verdad~~ — **medido**: unos 4 s del mensaje entrante
  a la primera respuesta en el móvil, y 0,64 s entre el aviso de privacidad y la
  respuesta (el segundo envío ya no paga el webhook).
- Qué devuelve Meta cuando **el token caduca** a las 24 h.

Con eso se cierra esta tarea. Sin eso, «funciona» quiere decir «funciona contra
un Meta que escribimos nosotros», que no es lo mismo.

### 5.1 El cuerpo real, capturado el 2026-08-02

Juan le escribió «Respuesta de prueba» desde su móvil al número de prueba, y el
cuerpo salió en «Revisa los webhooks de prueba» de la consola. Comparado campo a
campo con `canales/whatsapp.rs`:

**Lo que el traductor lee, estaba todo y en su sitio**: `object`, `entry[].id`,
`changes[].field = "messages"`, `value.metadata.phone_number_id`,
`messages[].id`, `.from`, `.timestamp` (cadena de segundos), `.type = "text"` y
`.text.body`.

**Seis campos que la documentación no menciona**, y que sí manda:

| Campo | Ejemplo | Qué es |
|---|---|---|
| `contacts[].profile.username` | `jcharlesisaza` | el alias público del perfil |
| `contacts[].user_id` | `CO.16300…` | identificador con prefijo de país |
| `contacts[].country_code` | `CO` | país del remitente |
| `messages[].from_user_id` | `CO.16300…` | el mismo, en el mensaje |
| `messages[].from_logical_id` | `63363669340208` | sin documentar |
| `messages[].internal_1p_only_data` | `{account_context:{waac_id, cs_id, account_context_type}}` | interno de Meta; el nombre lo dice |

Ninguno rompe nada, y **la razón es una decisión de diseño que hasta hoy no
tenía prueba**: `entrantes` recorre `serde_json::Value` campo a campo en vez de
deserializar a una estructura cerrada. Con una estructura `deny_unknown_fields`,
estos seis campos habrían tumbado **todos** los mensajes de un cliente en
producción, sin que ninguna prueba local lo viera. Queda fijado en
`traduce_el_cuerpo_real_de_meta_con_sus_campos_no_documentados`, con la forma
literal y los datos personales cambiados.

Un detalle menor que confirma por qué se captura en vez de copiar del manual:
Meta manda `type` **después** de `text`, y la documentación lo pone antes.

También se vio que `account_context_type` vale `non_paid_messaging` — el número
de prueba no cobra conversación, así que **lo del cobro sigue sin medirse**.

### 5.2 Voz y foto contra WhatsApp real, 2026-08-02

| Se mandó | Tipo que dice Meta | Qué hizo Chasqui |
|---|---|---|
| Nota de voz de 7 s | `audio` | `NoSoportado("audio")`, **escalado** (`no_soportado`/`audio`) y respuesta pasando a una persona, en 4 s |
| Foto | `image` | `NoSoportado("image")`, registrado, **y ni una palabra** |

**Que la foto no obtuviera respuesta es el acierto, no el fallo.** La
conversación ya estaba en `esperando_humano` por la nota de voz, y la regla del
dominio es que ahí el bot calla: contestar sería hablar por encima de la persona
que ya tiene el caso. Tampoco se duplicó el escalado — hay uno, no dos. Es la
primera vez que esa regla se ve funcionando fuera de las pruebas.

### 5.3 La descarga de medios, contra Meta real (2026-08-02, 12:35)

Ese mismo día se construyó la descarga de archivos, y esto es lo que quedó
medido con un teléfono de verdad:

| Lo que se mandó | Lo que guardó Chasqui |
|---|---|
| Una foto | `image/jpeg`, **56 243 bytes**, JPEG 899×1599 |
| Una nota de voz | `audio/ogg`, **11 146 bytes**, Ogg/Opus mono 16 kHz |
| Una ubicación | `6.0265235, -75.6994566` — sin descargar nada |

Con eso quedan **confirmadas las tres suposiciones que salían de la
documentación** y que el sustituto de Graph no podía desmentir: la ficha del
medio trae `url`, la descarga funciona mandando el token en `Authorization`, y
lo que baja es el archivo entero y no una versión recortada.

Y una trampa de método que costó un rato: para saber si el panel empotrado era
el nuevo, lo primero que se hizo fue `grep` al binario buscando `api/medios`.
Dio **cero**, y casi se acusa a `build.rs` de no empotrar la interfaz. La sonda
estaba mal: `rust-embed` comprime lo que empotra, así que la cadena no aparece
aunque esté. Lo que resolvió la duda fue **preguntarle al servidor que estaba
corriendo** —`curl` al `chunk-*.js` que sirve— y ahí sí estaba. El panel viejo
que se veía en pantalla era la caché del navegador, no el binario.

---

## Lo que hay que saber del túnel

Es de **usar y tirar**: la URL muere al cerrar la terminal y Meta se queda
apuntando a una URL que ya no existe. Para una sesión de pruebas está bien; para
un cliente hace falta un dominio y un certificado de verdad — eso es la tarea del
despliegue, no esta.

**Tarda medio minuto largo en funcionar, y hay que dejarlo tardar.** El guion
espera 30 s antes de mirar siquiera, y no es cautela: es lo que costó descubrir
que sondeando cada segundo **no funciona nunca**.

El nombre del túnel no existe todavía cuando `cloudflared` imprime la URL. La
primera consulta llega antes de que exista, el resolutor se guarda ese «no
existe» durante su TTL negativo, y a partir de ahí da igual cuántas veces se
reintente: la respuesta sale de la caché. Medido el 2026-08-01 con el mismo
túnel — sondeando cada segundo, no resolvió ni a los tres minutos; sin tocar el
DNS durante 75 s, resolvió a la primera.

Lo que despistaba: el registro de `cloudflared` decía `Registered tunnel
connection` durante los tres minutos enteros. El túnel estaba bien; lo roto era
la forma de preguntar. Si algún día vuelve a fallar, la manera de separarlo es
`dig +short @1.1.1.1 <el-nombre>`: si el resolutor externo lo ve y el del sistema
no, es la caché y no Cloudflare.
