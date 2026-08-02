<!-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas -->
# Chasqui

Plataforma de atención automatizada por WhatsApp, Instagram y Messenger.

*Chasqui*: los mensajeros relevistas del Tahuantinsuyo, que recorrían los caminos
llevando los mensajes — y los quipus.

**Se vende la plataforma; el cliente pone sus credenciales.** Su token de Meta,
su clave del modelo, su catálogo. Así la verificación de negocio de Meta la pasa
él, el coste por conversación lo ve y lo controla, y las conversaciones no salen
de su servidor.

Eso último no es un detalle técnico: un bot que atiende clientes trata datos
personales, y con el binario en casa del cliente la pregunta de dónde viven esos
datos se responde sola.

## Estado

El plan está en [`PLAN.md`](PLAN.md) y sale de cinco peticiones reales del
mercado, no de suposiciones. Lo construido hasta hoy:

| Pieza del MVP | Estado |
|---|---|
| Canal WhatsApp Cloud API (webhook + envío) | **funciona**, probado contra el binario real |
| Instagram y Messenger | **funcionan** (un solo adaptador para los dos) |
| Base de conocimiento y respuestas | **funciona** (disparadores exactos) |
| Respuestas **con los documentos del cliente** | **funciona** — modelo configurable, y si no está en los documentos no lo inventa |
| Retención y supresión (habeas data) | **funciona** — plazo configurable, purga diaria y derecho de supresión |
| Escalado a una persona | **funciona**, con cola y con el bot callándose |
| Aviso de privacidad | **funciona**, y sin él el bot no atiende |
| Credenciales del cliente cifradas en reposo | **funciona** (`guaca`, sobre Quipu) |
| Leer los datos del cliente (su API) | **funciona** — ver [`docs/INTEGRACION.md`](docs/INTEGRACION.md) |
| Panel para atender y configurar | **funciona** — Angular Material dentro del binario |
| Respaldo en caliente, verificado | **funciona** — `VACUUM INTO`, y la copia se abre y se compara antes de darse por buena |
| Despliegue en el servidor del cliente | **funciona** — plantillas de systemd y nginx, y [`despliegue/INSTALAR.md`](despliegue/INSTALAR.md) |
| Flujos de varios pasos y toma de pedidos | **funciona** — el bot recoge los datos uno a uno, arma el pedido y llama a una persona para despacharlo |
| Chat en la web del cliente, y su aplicación móvil | **funciona** — un `<script>` que se pega en su sitio, y la misma API para su app |
| Entregar el pedido al sistema del cliente (webhook saliente) | pendiente — hoy llega por el panel |

## Cómo se instala y se pone a andar

Un solo ejecutable y un archivo SQLite. Nada más. Para poner esto en el servidor
de un cliente —usuario propio, systemd, TLS, respaldos— la guía de una página es
[`despliegue/INSTALAR.md`](despliegue/INSTALAR.md); lo de aquí es la secuencia de
configuración del negocio.

```bash
export CHASQUI_FRASE="…"              # cifra las credenciales del cliente (32+)
export CHASQUI_SECRETO_APP="…"        # el de la app de Meta: verifica los webhooks
export CHASQUI_TOKEN_VERIFICACION="…" # el del alta del webhook en la consola
export CHASQUI_BD="sqlite://chasqui.db"

# 1. El aviso de privacidad. No hay valor por defecto A PROPÓSITO: tiene que
#    nombrar al responsable del tratamiento, que es el cliente. Sin él, no atiende.
chasqui ajuste poner guion.aviso_privacidad "Sus datos los trata ACME S.A.S. …"

# 2. El número del cliente. El token entra por la entrada estándar, no por un
#    argumento: los argumentos quedan en el historial del shell y en `ps`.
chasqui cuenta añadir --canal whatsapp --id <phone_number_id> --nombre Ventas

# 3. Las preguntas frecuentes.
chasqui conocimiento cargar preguntas.json

# 4. Opcional: sus documentos y el modelo que los redacta. La clave es del
#    cliente; la URL puede ser un proveedor o un modelo en su propia máquina.
chasqui documento cargar politicas.md
chasqui modelo configurar --url https://api.proveedor.com/v1 --nombre modelo-x

# 5. El plazo de retención. NO hay valor por defecto: lo decide el cliente, que
#    es el responsable del tratamiento. Sin él no se borra nada y se avisa.
chasqui ajuste poner retencion.dias 90

# 6. Opcional: las consultas a SU sistema, para que el bot conteste con sus
#    datos («su pedido 1234 llega el martes»). El archivo puede llevar sus
#    credenciales: se cifran al guardarlas y luego se borra el archivo.
chasqui consulta cargar consultas.json

# 6b. Opcional: los flujos de varios pasos —tomar un pedido, agendar una cita—.
#     Se rechazan al cargarlos si están mal, no a mitad de una conversación.
chasqui flujo cargar flujos.json

# 7. Quién entra al panel a atender. La contraseña, por la entrada estándar.
chasqui usuario crear --correo ana@acme.com --nombre Ana

# 8. A servir. Detrás de nginx con TLS; Meta exige https, y la cookie del panel
#    va marcada `Secure` salvo que se pida lo contrario con CHASQUI_SIN_TLS=1.
chasqui servir
```

El panel queda en la raíz (`https://…/`): la cola de quien espera, la
conversación con su historial y un cuadro para **responder como persona**, los
**pedidos** que armaron los flujos, y la configuración de los textos, las
preguntas frecuentes, las consultas y los **flujos de varios pasos** —con sus
pasos editables uno a uno, sin tocar un JSON por SSH—. Va **dentro del binario** — un solo ejecutable, sin carpetas que copiar ni fuentes
que pedirle a un CDN, que es lo que hace cierta la frase de que las
conversaciones no salen del servidor del cliente.

`chasqui escalados` enumera lo mismo desde la línea de órdenes.
`chasqui olvidar --contacto 573001…` ejerce el derecho de supresión y deja la
constancia; `chasqui purgar` borra lo que pasó del plazo (y se hace solo cada
24 h mientras el servidor esté levantado).

## El chat en la web, y en la aplicación móvil

Es **el único canal que funciona el primer día**: no necesita app de Meta, ni
verificación de negocio, ni token que caduque, y no tiene coste por conversación.

```bash
# El origen tal cual lo manda el navegador: sin barra final, y «www» delante
# es un origen DISTINTO (dé de alta los dos si su sitio responde por los dos).
chasqui cuenta añadir --canal web --id https://acme.com --nombre "Sitio de ACME"
```

Y en su sitio, antes de `</body>`:

```html
<script src="https://chat.acme.com/web/widget.js" async
        data-titulo="Escríbanos" data-tema="claro"></script>
```

Ya está: el chat aparece abajo a la derecha. Va dentro de un *shadow DOM*, así
que ni el CSS del sitio se mete en el chat ni el del chat en el sitio, y no
descarga nada de internet — la promesa de que la instalación no sale a ninguna
parte sigue siendo cierta.

**El tema lo manda el sitio, no el sistema del visitante.** `data-tema` admite
`claro` (por defecto), `oscuro`, y `auto` para un sitio que ya sigue al sistema.
El defecto es claro porque el chat vive dentro del diseño de otro: seguir al
sistema operativo de quien visita ponía una caja negra encima de una página
clara, y eso se vio mirándolo en un navegador — no lo dijo ninguna prueba.

Debajo es **el mismo bot**: el aviso de privacidad, las preguntas frecuentes, los
documentos, los flujos, el escalado a una persona y la retención valen igual, sin
una línea de código repetida. Quien atiende ve las conversaciones de la web en la
misma cola que las de WhatsApp.

**Una aplicación móvil es el mismo canal**, no un adaptador nuevo: habla las
mismas tres rutas (`POST /web/conversacion`, `POST /web/mensaje`,
`GET /web/mensajes`) y dice de qué sitio es con la cabecera `X-Chasqui-Sitio`,
porque no manda `Origin`.

El aspecto se comprueba con `herramientas/capturar-widget.sh`, que monta una
página **hostil a propósito** —CSS que fuerza letra serif granate, botones
amarillos y campos negros— y saca tres fotos: escritorio, móvil y tema oscuro. No
es ceremonia: los dos únicos fallos que ninguna prueba automática vio —el chat
saliendo oscuro sobre una web clara, y un `<form>` que navegaba la página del
cliente— aparecieron ahí.

Dos cosas que conviene saber antes de instalarlo:

- **Si su sitio tiene CSP**, hay que dejarle sitio al chat:
  `script-src https://chat.acme.com` y `connect-src https://chat.acme.com`.
- **El `<script>` no lleva `integrity`** a propósito: el widget cambia con cada
  versión de Chasqui, y un hash fijo dejaría de cargarlo en silencio al
  actualizar. El servidor es suyo, no una CDN ajena — el ataque que ese hash
  pararía exige que ya tengan la máquina donde viven las conversaciones.

## Los flujos: recoger un pedido paso a paso

Se declaran en un JSON y no se programan:

```json
[{ "titulo": "Toma de pedido",
   "disparadores": ["quiero hacer un pedido"],
   "confirmacion": "Anotado: {cantidad} de {producto} para {direccion}.",
   "pasos": [
     { "clave": "producto",  "pregunta": "¿Qué producto desea?" },
     { "clave": "cantidad",  "pregunta": "¿Cuántas unidades?", "tipo_dato": "numero" },
     { "clave": "direccion", "pregunta": "¿A qué dirección lo enviamos?" }
   ]}]
```

`tipo_dato` (`numero`, `correo`, `codigo`) valida lo que llega; sin él, el paso
recoge texto libre. Al terminar, el pedido se guarda —`chasqui pedidos` y el
panel lo enumeran— y **se llama a una persona para que lo despache**: un pedido
que solo queda en la base es un pedido que nadie atiende.

Mientras un flujo está en curso **se queda con los mensajes**, porque un paso de
texto libre no puede distinguir una dirección de un cambio de tema. Por eso tiene
tres salidas, y la primera pregunta las anuncia: pedir una persona, escribir
«cancelar», o no contestar un paso con dato tipado dos veces seguidas. En los tres
casos, **lo ya recogido viaja al escalado** para que quien atienda no vuelva a
preguntarlo todo.

Un flujo mal configurado —una `{clave}` en la confirmación que no es ningún
paso— se rechaza **al cargarlo**, no al enviarlo: si no, el cliente recibiría la
frase con un hueco justo al confirmar su pedido.

`chasqui respaldar <carpeta>` copia la base **en caliente**, sin parar el
servicio, y **verifica la copia antes de darla por buena**: la abre, le pasa
`integrity_check` y compara los recuentos con el origen. `chasqui restaurar
<carpeta>` la devuelve, y se niega a pisar una instalación que ya tiene datos.
Ojo con una cosa que es jurídica y no técnica: mientras un respaldo exista,
`olvidar` no lo alcanza — el plazo que se conserven las copias es el plazo real
en que una supresión queda completa, y va declarado en la política de
tratamiento.

Los tres webhooks son `/webhook/whatsapp`, `/webhook/instagram` y
`/webhook/messenger`.

## Fotos, notas de voz y ubicaciones

Cuando alguien manda una foto, una nota de voz, un vídeo o un documento, Chasqui
**se lo trae y lo guarda**, y quien atiende lo ve —y lo abre— dentro de la
conversación en el panel. Importa más de lo que parece: con la API oficial de
WhatsApp el número del negocio **no vive en ningún teléfono**, así que el panel
es la única bandeja que existe. Lo que no se guarde aquí, el negocio no lo verá
nunca.

Los archivos van **dentro de la base**, no en una carpeta aparte. Así, borrar a
una persona borra también sus archivos, el plazo de retención los alcanza y el
respaldo se los lleva — sin nada que configurar y sin que se pueda olvidar. Hay
un tamaño máximo, y lo que lo pasa **queda anotado diciendo que no cabe**: quien
atiende se entera y puede pedirlo por otra vía, en vez de creer que el cliente
no mandó nada.

Una **ubicación** no se descarga: las coordenadas vienen en el propio mensaje y
se guardan con él. Y se acompañan del municipio, el departamento y el país —«a
1,6 km de Amagá, Antioquia, Colombia»— **resueltos dentro del servidor del
cliente**, con una tabla de 69 561 ciudades empotrada en el binario.

Eso último no es un capricho técnico. Traducir unas coordenadas preguntándole a
un servicio de mapas obligaría a mandarle la ubicación exacta de los clientes del
cliente, en cada consulta, a una empresa que él no eligió. Chasqui no lo hace: no
sale ni un byte de la máquina, y funciona sin internet.

La distancia va escrita a propósito. La ciudad conocida más cercana a un pueblo
pequeño está a 10 km de mediana y a veces a 30 o 40, así que decir «Amagá» a
secas convertiría una aproximación en una afirmación. Y si no hay ninguna ciudad
a menos de 100 km, no se inventa: salen solo las coordenadas.

En los cuatro casos el bot no responde por su cuenta: **llama a una persona**,
porque una foto no se contesta a ciegas.

## Qué se probó, y contra qué

**Contra WhatsApp de verdad, el 2026-08-02.** Un mensaje escrito a mano desde un
teléfono al número de la Cloud API entró por el webhook y Chasqui contestó: el
aviso de privacidad primero y la respuesta después, las dos con acuse de Meta.
Se midieron cuatro segundos entre el «hola» y la primera respuesta. Lo que costó
llegar ahí, paso a paso y con las tres paredes que hay por el camino, está en
[`docs/META.md`](docs/META.md).

Además de las pruebas de cada pieza, hay una que **arranca el binario de verdad,
le da de alta una cuenta por su línea de órdenes y le manda un webhook firmado
como lo firma Meta**, con un sustituto de la API Graph en localhost
(`crates/chasqui-api/tests/frontera_meta.rs`). Es la que comprueba lo que ninguna
prueba de unidad puede: que la firma se verifica sobre los bytes que llegaron,
que el token cifrado se descifra y llega a la cabecera `Authorization`, que el
aviso de privacidad sale **antes** que la respuesta, y que 120 conversaciones a la
vez se atienden todas.

## Licencia

Propietario. Copyright (C) 2026 Juan Carlos Isaza Arenas. Ver [`NOTICE`](NOTICE).
