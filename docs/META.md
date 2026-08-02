<!-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas -->
# Probar Chasqui contra Meta de verdad

**El riesgo número uno antes de vender nada.** Todas las pruebas del repositorio
—262— hablan con un Meta de mentira en localhost. Miden el cableado y el códec,
no el producto contra el mundo. Hasta que un mensaje de WhatsApp real entre y
salga, no se compromete una fecha de entrega con nadie.

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

### 0. El muro real: la verificación del negocio

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

- El **cuerpo exacto** que mandó Meta (está en el registro): compararlo campo a
  campo con lo que espera `canales/whatsapp.rs`.
- Qué manda al escribir una **nota de voz**, una **foto** y una **ubicación**:
  hoy se marcan «no soportado» y van a una persona, pero conviene ver el nombre
  real del tipo.
- Cuánto tarda el envío de verdad.
- Qué devuelve Meta cuando **el token caduca** a las 24 h.

Con eso se cierra esta tarea. Sin eso, «funciona» quiere decir «funciona contra
un Meta que escribimos nosotros», que no es lo mismo.

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
