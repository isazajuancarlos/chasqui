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
| Base de conocimiento y respuestas | **funciona** (disparadores; el modelo de lenguaje todavía no) |
| Escalado a una persona | **funciona**, con cola y con el bot callándose |
| Aviso de privacidad | **funciona**, y sin él el bot no atiende |
| Credenciales del cliente cifradas en reposo | **funciona** (`guaca`, sobre Quipu) |
| Leer los datos del cliente (su API) | **funciona** — ver [`docs/INTEGRACION.md`](docs/INTEGRACION.md) |
| Panel para atender y configurar | **funciona** — Angular Material dentro del binario |
| Flujos y toma de pedidos | pendiente |

## Cómo se instala y se pone a andar

Un solo ejecutable y un archivo SQLite. Nada más.

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

# 4. Opcional: las consultas a SU sistema, para que el bot conteste con sus
#    datos («su pedido 1234 llega el martes»). El archivo puede llevar sus
#    credenciales: se cifran al guardarlas y luego se borra el archivo.
chasqui consulta cargar consultas.json

# 5. Quién entra al panel a atender. La contraseña, por la entrada estándar.
chasqui usuario crear --correo ana@acme.com --nombre Ana

# 6. A servir. Detrás de nginx con TLS; Meta exige https, y la cookie del panel
#    va marcada `Secure` salvo que se pida lo contrario con CHASQUI_SIN_TLS=1.
chasqui servir
```

El panel queda en la raíz (`https://…/`): la cola de quien espera, la
conversación con su historial y un cuadro para **responder como persona**, y la
configuración de los textos, las preguntas frecuentes y las consultas. Va
**dentro del binario** — un solo ejecutable, sin carpetas que copiar ni fuentes
que pedirle a un CDN, que es lo que hace cierta la frase de que las
conversaciones no salen del servidor del cliente.

`chasqui escalados` enumera lo mismo desde la línea de órdenes.

Los tres webhooks son `/webhook/whatsapp`, `/webhook/instagram` y
`/webhook/messenger`.

## Qué se probó, y contra qué

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
