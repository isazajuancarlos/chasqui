<!-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas -->
# Conectar su WhatsApp — guía para el negocio

Esta guía es para **usted, el dueño del negocio**. Al terminar tendrá su WhatsApp
conectado y nos habrá entregado cuatro datos para que lo pongamos a funcionar.

No hace falta que sepa programar. Sí hace falta paciencia con la consola de Meta:
está pensada para desarrolladores y cambia a menudo. Lo que sigue es el camino
exacto, incluidas **tres trampas que la ayuda de Meta no menciona** y que sin
esta guía cuestan una tarde entera.

**Cuánto toma:** una hora larga si la verificación del negocio sale a la primera.

---

## Antes de empezar, tres cosas que conviene entender

**1. Las credenciales son suyas, no nuestras.** Usted crea la cuenta, usted pone
su NIT, usted es dueño del número. Nosotros ponemos el programa. Eso no es un
tecnicismo: significa que el costo de los mensajes lo ve y lo controla usted, que
si mañana deja de trabajar con nosotros el número sigue siendo suyo, y que las
conversaciones de sus clientes no pasan por ningún servidor nuestro.

**2. El número que conecte deja de funcionar en el teléfono.** Un número
conectado a la API de WhatsApp **ya no se puede usar en la aplicación de
WhatsApp**: vive en el servidor. Si hoy atiende por WhatsApp desde un celular con
ese número, hay dos caminos: conectar un número nuevo, o pasar el actual y
atender **desde el panel** que le entregamos. No hay vuelta atrás fácil, así que
decídalo antes.

**3. Meta le va a pedir verificar su negocio.** Le van a pedir documentos legales
reales. No es un trámite que se pueda saltar ni que podamos hacer por usted: va a
nombre de quien es dueño del negocio.

---

## Paso 1 — La cuenta de desarrollador

1. Entre a **developers.facebook.com** con su cuenta de Facebook.
   - Si no tiene, cree una. Sirve una personal; no necesita una página nueva.
   - Meta le pedirá **verificar su identidad** (correo y teléfono) antes de
     dejarle crear nada.
2. Cree un **portafolio comercial** (Meta lo llama «Business Portfolio» o
   «portafolio comercial») a nombre de su negocio.

## Paso 2 — Verificar el negocio

**Esta es la primera trampa, y la más cara: sin esto Meta no le da número, y no
se lo dice hasta que lo intenta.** Se ve así:

> **Cuenta restringida.** Para poder empezar a enviar mensajes, tendrás que
> verificar tu negocio.

Dónde está, que no es donde uno lo busca:

> **Configuración → Seguridad e idoneidad → Centro de seguridad → Verificación
> del negocio**

No está en «Información del negocio», que es donde parece que debería estar.

Al iniciarla le pedirán **elegir un caso de uso**. Escoja:

> «La app requiere acceso a los permisos en Meta for Developers»

Los otros casos piden documentos distintos y lo mandan por un camino más largo.

**A nombre de quién verificar.** Si su negocio es una empresa constituida,
verifique **la empresa**, con su NIT y su certificado de existencia. Si trabaja
como persona natural, verifique **la persona**, con su documento. Verificar a
nombre de una marca que no es una persona jurídica es el camino directo al
rechazo.

**Cuánto tarda.** Meta declara dos días hábiles. Puede salir mucho antes: en
nuestra prueba del 2 de agosto de 2026 se aprobó **el mismo día**. Pero cuente
con los dos días al planear.

## Paso 3 — Crear la aplicación

1. **Mis apps → Crear app.**
2. **El nombre no puede contener «whatsapp»** ni ninguna marca de Meta. Lo
   rechaza con un aviso rojo y sin explicar por qué. Ponga el nombre de su
   negocio: «atencion-acme».
3. En casos de uso escoja **«Conectarte con los clientes a través de WhatsApp»**.
4. Seleccione el **portafolio comercial** que creó en el paso 1.
5. Meta le pedirá **su contraseña** antes de crearla.

> **Segunda trampa: el tipo de app no se puede cambiar después.** Lo fija el caso
> de uso que escoja aquí. Si se equivoca y crea una app «de tipo Ninguno», no
> ofrecerá WhatsApp entre sus productos y **no hay ningún control para
> corregirlo**: hay que crear otra app y empezar de nuevo.

## Paso 4 — El número

Dentro de la app: **Casos de uso → Personalizar → Paso 1. Pruébalo**.

Para probar, Meta regala un número de prueba de Estados Unidos. Sirve para ver el
bot funcionando **antes** de comprometer su número real, y solo puede escribirle
a hasta cinco teléfonos que usted registre con un código.

Para operar de verdad, en **Paso 2: Configuración de producción → Registra tu
número de teléfono**, agregue el número del negocio. Meta lo verifica con un
código por SMS o llamada.

Recuerde el punto 2 del principio: ese número deja de funcionar en la aplicación
de WhatsApp.

## Paso 5 — Lo que tiene que entregarnos

Cuatro datos. Los tres primeros no cambian; el cuarto sí, y ojo con él.

| Dato | Dónde está |
|---|---|
| **Identificador del número** (`phone_number_id`) | Casos de uso → Personalizar → Paso 1, junto al número |
| **Identificador de la cuenta de WhatsApp** (WABA) | En la misma pantalla |
| **Clave secreta de la app** | Configuración → Básica → «Clave secreta de la app» → Mostrar |
| **Token de acceso** | Ver abajo |

**Tercera trampa: el token que ofrece la consola NO dura 24 horas.** Aunque la
documentación diga lo contrario, es un token **de sesión**: se muere cuando se
acaba su sesión en la consola, a una hora fija. En nuestra prueba duró **tres
horas y media**. Si nos entrega ese token, el bot dejará de responder esa misma
tarde.

Para operar hay que crear un **token permanente**, que se hace una vez:

> Configuración del portafolio comercial → **Usuarios → Usuarios del sistema** →
> Agregar → rol de administrador → **Generar token**, escogiendo su app y los
> permisos `whatsapp_business_messaging` y `whatsapp_business_management`.

Ese token no caduca. **Trátelo como la llave de su WhatsApp**: quien lo tenga
puede escribirle a sus clientes en nombre de su negocio. No lo mande por
WhatsApp ni por correo sin cifrar; se lo pediremos por un canal seguro.

## Paso 6 — Lo que hacemos nosotros

Con esos cuatro datos configuramos el servidor y damos de alta el webhook, que es
la dirección a la que Meta le entrega los mensajes. Ahí hay un paso que la
consola **no hace sola** y que casi nadie documenta: además de suscribir el campo
de mensajes, hay que suscribir **la aplicación a la cuenta de WhatsApp**. Sin eso
Meta recibe los mensajes, los muestra en su propio panel… y no se los entrega a
nadie. Todo se ve en verde y no llega nada.

Lo mencionamos aquí para que sepa que existe, no porque tenga que hacerlo usted.

---

## Lo que le costará a usted, en plata

Nosotros cobramos la licencia del programa. Meta cobra aparte, directamente a
usted, **por conversación** y no por mensaje: una conversación cubre 24 horas.
Las que **inicia su cliente** escribiéndole son gratis en el esquema actual de
Meta; las que inicia el negocio (una promoción, un recordatorio) se cobran. Las
tarifas las fija Meta por país y las cambia; consúltelas en su consola, donde las
verá con su propio precio.

## Si algo falla

| Lo que ve | Qué es |
|---|---|
| «Cuenta restringida» | Falta la verificación del negocio (paso 2) |
| No aparece WhatsApp entre los productos | La app se creó con el tipo equivocado; hay que crear otra |
| El bot deja de responder de un momento a otro | El token caducó. Casi siempre es haber usado el de la consola en vez del permanente |
| Los mensajes llegan a la consola pero el bot no contesta | Falta suscribir la app a la cuenta de WhatsApp — díganoslo |

En el panel que le entregamos, cuando el bot no puede responder por un problema
de credenciales **la conversación no se pierde**: pasa a la cola de su equipo con
el aviso de qué ocurrió, para que una persona conteste mientras se arregla.
