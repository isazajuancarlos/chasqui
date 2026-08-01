<!-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas -->
# Plan de Chasqui

*Chasqui*: los mensajeros relevistas del Tahuantinsuyo, que recorrían los caminos
llevando los mensajes — y los quipus.

Plataforma de atención automatizada por WhatsApp, Instagram y Messenger. **Se
vende la plataforma; el cliente pone sus credenciales.**

---

## 1. De dónde sale este plan

De cinco peticiones reales de Workana, recogidas el 2026-08-01 en
`docs/peticiones-workana.txt`. No es una idea de producto: es lo que cinco
empresas distintas pidieron con sus palabras.

Lo primero que hay que ver es que **son el mismo producto cinco veces**.

| Requisito | Lo piden |
|---|:--:|
| WhatsApp | **5 de 5** |
| Respuestas a preguntas frecuentes | **5 de 5** |
| API de WhatsApp Business | 4 de 5 |
| IA para entender la consulta | 4 de 5 |
| Flujos de conversación | 3 de 5 |
| «Escalable y robusto» | 3 de 5 |
| Toma de pedidos y precios | 1 de 5 |
| Instagram y Facebook | 1 de 5 |
| «Aprender y adaptarse» | 1 de 5 |

**Ninguna pide nada que las otras no puedan usar.** Un producto configurable las
cubre las cinco, y la sexta que salga el mes que viene también. Ahí está el
negocio: en Workana cada una de esas es un proyecto a medida que alguien
construye desde cero; aquí es el mismo binario configurado, y a partir del
segundo cliente el margen es entero.

---

## 2. El modelo: la plataforma es nuestra, las credenciales son suyas

Cada cliente configura su propio token de Meta, su clave del modelo de lenguaje,
su catálogo y sus flujos.

Eso resuelve tres cosas de golpe:

1. **La verificación de negocio de Meta la pasa el cliente**, que ya tiene NIT y
   registro mercantil. Xiliux no es persona jurídica y no podría pasarla.
2. **El coste por conversación y por token lo paga el cliente**, en su propia
   cuenta y a la vista. No somos revendedores de infraestructura ajena con un
   margen que Meta puede cambiar mañana.
3. **El negocio es licencia**, igual que Quipu.

---

## 3. Lo que hay que decidir ANTES de escribir código

### 3.1. Las APIs no oficiales se rechazan por escrito

La cuarta petición pide «APIs de WhatsApp (oficiales o no oficiales, según
viabilidad)». Las no oficiales —Baileys, `whatsapp-web.js`— funcionan hasta que
Meta cierra el número del cliente. Y lo cierra.

**Se rechaza en la propuesta, no se calla.** El día que a un negocio que vive del
WhatsApp le bloquean el número, el proveedor que lo montó es quien responde. Y
decirlo por adelantado distingue de quien acepta cualquier cosa por cerrar el
trato: filtra al cliente que solo mira el precio, que es exactamente el que
después reclama.

### 3.2. «Aprender y adaptarse» se acota en el contrato

Lo que se entrega es un modelo que responde **sobre los documentos del cliente**
(recuperación aumentada), no un sistema que aprende solo de las conversaciones.

Sin acotarlo, el cliente espera lo segundo y la entrega nunca es «lo que pedí».
Es la clase de ambigüedad que convierte un proyecto entregado en una discusión.

### 3.3. El coste por conversación va delante

Meta cobra por conversación iniciada, y **ninguna de las cinco peticiones lo
menciona**. Si no se pone delante, el cliente descubre la factura después y la
culpa es del proveedor. Con el cliente pagando a Meta directamente, deja de ser
una sorpresa y pasa a ser una ventaja: ve el coste real y lo controla.

---

## 4. Lo que ninguno pide y todos necesitan

Aquí está la diferencia que ningún competidor de Workana va a mencionar, y es
donde el criterio jurídico se vende con la profundidad técnica como prueba.

### 4.1. Habeas data (Ley 1581 de 2012)

Un bot que atiende clientes **trata datos personales**. El cliente necesita:

- aviso de privacidad en el primer mensaje de cada conversación,
- política de tratamiento de datos publicada,
- saber dónde se guardan las conversaciones y cuánto tiempo,
- una forma de atender las solicitudes de supresión.

Con el binario en el servidor del cliente, esto se responde solo: **las
conversaciones no salen de su máquina**. Quien lo ofrece como servicio en la nube
no puede decir lo mismo, y encima se convierte en responsable del tratamiento de
los datos de los clientes de sus clientes.

Esto no es un extra: es la razón por la que Chasqui puede cobrar más que el
freelancer de al lado.

### 4.2. Qué pasa cuando el bot no sabe

Sin escalado a una persona, el bot frustra y el cliente lo apaga a las dos
semanas. **Va en el MVP, no en la versión 2.**

---

## 5. El MVP que cubre las cinco peticiones

1. **Canal WhatsApp Cloud API**: recepción por webhook y envío, con las
   credenciales del cliente.
2. **Base de conocimiento por cliente**: preguntas frecuentes, precios y
   documentos, con recuperación aumentada sobre un modelo configurable.
3. **Flujos con menú y captura de datos**, suficientes para tomar un pedido.
4. **Escalado a una persona** cuando el bot no sabe o el cliente lo pide.
5. **Panel de configuración** para que el cliente lo ajuste todo sin tocar
   código.

### Los tres canales por el precio de uno y dos adaptadores

WhatsApp, Instagram y Messenger son **las tres de Meta**: misma consola de
desarrollador, mismo modelo de webhook y token, endpoints distintos. Soportar los
tres no es triple trabajo si el diseño lo contempla desde el principio — y la
petición que los pide los tres sale casi gratis.

**Decisión de diseño**: el canal es una interfaz desde el primer día, aunque solo
haya una implementación. No porque «vaya a hacer falta», sino porque ya hay una
petición que lo pide.

---

## 6. Stack

Rust (`axum` + `SQLx`/SQLite) con Angular Material embebido en el binario, según
la directiva 26. Un solo ejecutable que funciona en el servidor del cliente, una
base por instancia.

---

## 7. Precios

En [`PROPUESTA.md`](PROPUESTA.md), con las cifras en pesos y dólares y el
razonamiento de dónde salen. Tres paquetes: 450, 850 y 1.200 dólares, más
acompañamiento mensual opcional.

Lo que hay que entender del modelo: **el primer cliente paga el desarrollo y del
segundo en adelante el margen es casi entero**, porque es el mismo binario
configurado. Por eso no se cotiza por horas — el trabajo baja y el valor
entregado no.

### El coste de operar es casi cero, y eso hay que decirlo antes de que pregunten

Meta cobra **por mensaje** desde julio de 2025, y en Colombia:

| | Coste |
|---|---|
| Responder dentro de la ventana de 24 h | **gratis** |
| Utilidad y autenticación | USD 0,0008 |
| Marketing | USD 0,0125 |

**Un asistente de atención responde a quien le escribe**, así que la mayor parte
del tráfico no cuesta nada. Colombia tiene además de las tarifas más bajas del
mundo. Solo se paga cuando el negocio inicia la conversación.

El coste real de operación no es Meta: es el modelo de lenguaje, y ronda un dólar
por cada mil conversaciones.

## 8. Lo que puede salir mal

En [`RIESGOS.md`](RIESGOS.md): once puntos sacados de leer las cinco peticiones,
cada uno con la señal que lo delata en el texto del cliente para reconocerlo en
la sexta sin releer nada.

Los tres que más caros salen si se olvidan:

1. **El plazo depende de la verificación de Meta**, que no controla nadie. Se
   cuenta desde que la cuenta está lista, y así se escribe.
2. **Ninguna petición dice el volumen**, y las tres que dicen «escalable» no
   ponen número. Eso se convierte en cifra antes de cotizar.
3. **Si el cliente no tiene sus contenidos por escrito**, el proyecto se para
   esperándolos y la culpa acaba siendo del proveedor.

## 9. Estado

Nada implementado. Este documento es el plan, no un informe de avance.

| Pieza | Estado |
|---|---|
| Canal WhatsApp | pendiente |
| Base de conocimiento y respuestas | pendiente |
| Flujos y toma de pedidos | pendiente |
| Escalado a una persona | pendiente |
| Panel de configuración | pendiente |
| Aviso de privacidad y retención | plantillas listas en `legal/`; falta el mecanismo en código |
