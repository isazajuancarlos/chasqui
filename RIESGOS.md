<!-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas -->
# Lo que puede salir mal

Sacado de leer las cinco peticiones de `docs/peticiones-workana.txt` el
2026-08-01. **Todo esto se decide antes de cotizar**, no cuando ya aparece.

Cada punto lleva la señal que lo delata en el texto del cliente, para poder
reconocerlo en la sexta petición sin releer esto entero.

---

## 1. Ninguna dice el volumen. Ninguna dice el presupuesto.

**La señal:** «capacidad para manejar un volumen considerable de mensajes»,
«manejar un volumen significativo de interacciones», «solución escalable y
robusta». Tres de las cinco lo dicen; **ninguna pone un número**.

**Por qué importa:** «escalable» significa cosas distintas para cien
conversaciones al día y para cinco mil. Cotizar sin ese número es cotizar a
ciegas, y el ajuste posterior siempre lo paga quien cotizó.

**Qué hacer:** convertirlo en cifra **antes** de dar precio. Las preguntas del
final de la propuesta existen para eso — la tercera es «¿cuántas conversaciones
recibe al día?». Si el cliente no lo sabe, ese dato es el primer entregable, no
una molestia.

---

## 2. El plazo depende de Meta, y Meta no es tuya

**La señal:** ninguna petición lo menciona. Ese es el problema.

**Por qué importa:** para usar la API oficial, el negocio del cliente tiene que
verificarse ante Meta con sus documentos. Ese trámite **puede tardar días o
semanas** y no lo controlas ni tú ni el cliente. Si prometes «dos semanas» y Meta
tarda tres en verificarlo, incumples por algo ajeno — y en Workana eso es una
mala reseña que te sigue.

**Qué hacer:** el plazo comprometido cuenta **desde que la cuenta de WhatsApp
Business esté verificada y con acceso a la API**. Escrito así, en la propuesta.
Ofrecer acompañar el trámite es buen servicio; asumir su plazo es un error.

---

## 3. «Posibilidad de escalar y añadir funcionalidades en el futuro»

**La señal:** literal en la quinta petición.

**Por qué importa:** suena a visión de producto y funciona como puerta abierta a
trabajo no pagado. «Ya que estamos, añade…» durante el proyecto.

**Qué hacer:** el alcance entregable se lista **cerrado** en la propuesta.
«Escalable» se responde con la arquitectura —se puede añadir después—, no con la
promesa de añadirlo dentro de este precio. Lo que venga después es otra
cotización, y decirlo desde el principio no ofende a nadie razonable.

---

## 4. «El freelancer deberá proponer la tecnología y arquitectura»

**La señal:** literal en la quinta petición.

**Por qué importa:** casi siempre significa que el cliente no sabe qué quiere.
No es malo —es una oportunidad de decidir tú— pero garantiza que «lo que pedí»
sea distinto de «lo que entendí» si no se fija por escrito.

**Qué hacer:** la propuesta ES la especificación. Que la apruebe antes de
empezar, con la lista de lo que incluye y lo que no. Es el documento al que se
vuelve cuando aparece la discusión.

---

## 5. Si el cliente no tiene sus contenidos por escrito, el proyecto se para

**La señal:** las cinco dan por hecho que las «preguntas frecuentes» y la
«información de productos y precios» existen en algún sitio. En negocios
pequeños, muchas veces solo están en la cabeza de alguien.

**Por qué importa:** el asistente responde con los documentos del cliente. Sin
documentos no hay nada que responder, y el proyecto queda esperando un material
que nadie tiene tiempo de escribir. El plazo se lo come la espera y la culpa
acaba siendo del proveedor.

**Qué hacer:** pedir cinco ejemplos de preguntas reales **antes de cotizar** —es
la primera pregunta de la propuesta—. Si no los tiene, armar los contenidos es un
trabajo aparte que se cotiza aparte, y decirlo antes evita que se convierta en un
regalo.

---

## 6. Nadie dice quién atiende lo que el bot no sabe

**La señal:** ninguna de las cinco menciona a la persona que recibe la
conversación cuando el asistente se queda corto.

**Por qué importa:** el traspaso a un humano es lo que evita que el bot frustre.
Si no hay nadie al otro lado, la función existe y no sirve — y el cliente
concluye que el bot es malo.

**Qué hacer:** preguntarlo antes de empezar (es la cuarta pregunta de la
propuesta). Si el cliente no tiene a nadie, hay que decírselo: el asistente puede
recoger el mensaje y avisar, pero alguien tiene que leerlo.

---

## 7. Las APIs no oficiales

**La señal:** «APIs de WhatsApp (oficiales o no oficiales, según viabilidad)»,
en la cuarta petición.

**Por qué importa:** funcionan hasta que Meta cierra el número del cliente. Y lo
cierra. Cuando eso pasa, el negocio pierde su canal de ventas y sus
conversaciones, y quien lo montó responde.

**Qué hacer:** rechazarlo **por escrito** en la propuesta, explicando el motivo.
No es solo protegerse: es lo que distingue de quien acepta cualquier cosa por
cerrar, y filtra al cliente que solo mira el precio — que es exactamente el mismo
que después reclama.

---

## 8. «Aprender y adaptarse a nuevas situaciones»

**La señal:** literal en la cuarta petición.

**Por qué importa:** el cliente espera un sistema que mejora solo con el uso. Lo
que se entrega es un modelo que responde con **sus** documentos. Si no se acota,
la entrega nunca es «lo que pedí» aunque funcione perfectamente.

**Qué hacer:** decir en la propuesta qué es y qué no, con la ventaja por delante:
cuando quiera que sepa algo nuevo, lo carga y lo sabe al momento — no hay que
esperar a que «aprenda», y nadie se lleva una sorpresa con lo que contesta.

---

## 9. El coste operativo, si no lo pones tú, lo descubre él

**La señal:** ninguna de las cinco menciona lo que cuesta operar el sistema.

**Por qué importa:** Meta cobra por mensaje. Si el cliente lo descubre en la
primera factura, la culpa es del que no se lo dijo.

**Qué hacer:** ponerlo delante con **sus** números. Y aquí el dato juega a favor:
en Colombia, responder dentro de la ventana de 24 horas es **gratis**, que es
justo lo que hace un asistente de atención. Se convierte en argumento en vez de
sorpresa. Ver `PROPUESTA.md`.

---

## 10. Habeas data: nadie lo pide y todos lo necesitan

**La señal:** ninguna de las cinco lo menciona.

**Por qué importa:** un asistente que atiende clientes trata datos personales, y
el responsable del tratamiento es el cliente (Ley 1581 de 2012). Necesita aviso
de privacidad, política publicada y saber dónde viven las conversaciones y
cuánto tiempo.

**Qué hacer:** llevarlo resuelto, no mencionado. **Antes de mandar la primera
propuesta hay que tener la plantilla de aviso y de política de tratamiento
lista.** Es lo que sostiene la diferencia; llegar sin ella la convierte en una
frase bonita — y si el cliente la pide en la reunión, hay que poder enseñarla.

---

## 11. No competir por precio

**La señal:** no está en las peticiones; está en el mercado.

**Por qué importa:** quien acepta la API no oficial siempre va a cobrar menos, y
esa competencia no se gana bajando.

**Qué hacer:** la propuesta dice **lo que no se va a hacer y por qué**. Eso
espanta al cliente que solo mira el precio, y ese es el objetivo: es el mismo que
después reclama, pide cosas fuera de alcance y deja mala reseña si no se las das.

---

## Resumen: lo que hay que tener resuelto antes de mandar una propuesta

- [ ] Cinco preguntas reales de sus clientes, por escrito
- [ ] Número de conversaciones al día
- [ ] Si tiene cuenta de WhatsApp Business verificada, o hay que crearla
- [ ] Quién atiende lo que el asistente traspase
- [ ] Plantilla de aviso de privacidad y política de tratamiento **lista**
- [ ] TRM del día, con su fecha, en el documento
