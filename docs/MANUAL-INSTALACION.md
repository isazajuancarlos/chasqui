<!-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas -->
# Instalar Chasqui en casa de un cliente — guía de operación

Para quien instala, no para quien compra. El manual del cliente es
[`MANUAL-CLIENTE-META.md`](MANUAL-CLIENTE-META.md); el servidor —usuario,
systemd, TLS, respaldos— está en [`../despliegue/INSTALAR.md`](../despliegue/INSTALAR.md).
Esto es lo que va **entre** esos dos: el orden real, las comprobaciones y las
cuatro cosas que fallan.

**Regla de oro del orden**: nada de Chasqui funciona hasta que Meta entregue el
número, y Meta no entrega número hasta que el negocio esté verificado. Empiece
por ahí el primer día, aunque el resto no esté listo — es lo único que depende
de un tercero.

---

## Requisitos del servidor — medidos, no supuestos

`ldd` sobre el binario, el 2026-08-02:

| Necesita | Nota |
|---|---|
| Linux x86-64 | |
| **glibc 2.34 o superior** | Debian 12+, Ubuntu 22.04+, Rocky 9+. Debian 11, RHEL 8 y Amazon Linux 2 **no sirven** |
| `libm`, `libgcc_s` | están en cualquier Linux |

Y nada más: **ni OpenSSL, ni SQLite, ni un runtime**. Todo va dentro del
ejecutable, incluido el panel. Se copia y se ejecuta.

Comprobación en el servidor del cliente antes de subir nada:

```bash
ldd --version | head -1      # tiene que decir 2.34 o más
```

Si algún día aparece un cliente con un servidor viejo, la salida es compilar
estático contra musl (`--target x86_64-unknown-linux-musl`). Requiere un
compilador de C para musl en la máquina de compilación, que hoy no está montado
— y no se montó porque los servidores reales de nuestros clientes son Debian 13
y Ubuntu 24.04, muy por encima del mínimo.

## 0. Lo que hay que tener antes de tocar el servidor

Del cliente, por un canal seguro (nunca WhatsApp ni correo en claro):

| Dato | Va a |
|---|---|
| `phone_number_id` | `chasqui cuenta añadir --id` |
| Token **permanente** de usuario del sistema | entrada estándar de esa misma orden |
| Clave secreta de la app | `CHASQUI_SECRETO_APP` |
| WABA ID | la suscripción del paso 4 |

Decidido con el cliente y por escrito, porque **el binario no arranca sin ello**
y es a propósito:

- el **texto del aviso de privacidad**, que tiene que nombrarlo a él como
  responsable del tratamiento;
- el **plazo de retención** en días.

Ninguno tiene valor por defecto. Un aviso genérico nombraría al responsable
equivocado y un plazo inventado sería una decisión del encargado sobre datos
ajenos.

## 1. Compilar y entregar

```bash
cd interfaz && npm install && npm run build && cd ..   # el panel, PRIMERO
cargo build --release
cargo test --workspace --no-fail-fast                  # sin esto no se entrega
```

La interfaz va **antes**: `build.rs` empotra `interfaz/dist/`, y si no está,
empotra una página que dice qué ejecutar. Al cliente viaja un solo archivo,
`target/release/chasqui`, más las plantillas de `despliegue/`.

> **No confíe en un `grep` al binario para comprobar que el panel es el nuevo.**
> `rust-embed` comprime lo que empotra: la cadena no aparece aunque esté ahí, y
> el resultado es un falso negativo. Para saber qué sirve, pregúnteselo al
> servidor corriendo: `curl -s http://127.0.0.1:PUERTO/ | grep chunk`.

## 2. Configurar la instancia

La secuencia completa está en el `README.md`. Dos cosas que no se pueden hacer
de otra forma:

- **El token entra por la entrada estándar, nunca como argumento.** Los
  argumentos quedan en el historial del shell y los ve cualquiera con `ps`.
- **`CHASQUI_FRASE` cifra las credenciales en reposo.** Si se pierde, los tokens
  guardados no se pueden descifrar y hay que volver a darlos de alta. Va al
  gestor de contraseñas antes de arrancar nada.

## 3. Dar de alta el webhook

En la consola de Meta, **Casos de uso → Personalizar → Paso 2: Configuración de
producción → Configurar webhooks**:

- URL: `https://SU-DOMINIO/webhook/whatsapp`
- Token de verificación: el mismo `CHASQUI_TOKEN_VERIFICACION`

Meta llama en ese momento y espera el reto de vuelta. Si dice que no pudo
verificar, el registro del servidor dice exactamente qué pasó.

Después, **suscriba el campo `messages`** en la tabla de abajo.

## 4. La suscripción que la consola NO hace

**Son dos suscripciones y la consola solo hace una.** Sin la segunda, Meta recibe
los mensajes, los enseña en su propio panel y **no se los entrega a usted**: todo
en verde y no llega nada. Es el peor diagnóstico posible y costó dos mensajes de
prueba descubrirlo.

```bash
# ¿Quién está suscrito? Si solo sale «WA DevX Webhook Events 1P App», que es de
# Meta, la app del cliente NO está.
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://graph.facebook.com/v23.0/<WABA_ID>/subscribed_apps"

curl -s -X POST -H "Authorization: Bearer $TOKEN" \
  "https://graph.facebook.com/v23.0/<WABA_ID>/subscribed_apps"
```

**No dé por buena la respuesta `{"success":true}`**: vuelva a leer la lista y
compruebe que aparece la app por su nombre.

## 5. Comprobar de verdad, no «parece que anda»

Comprobación autoritativa del alta, que no depende de leer la interfaz de Meta:

```bash
curl -s "https://graph.facebook.com/v23.0/<APP_ID>/subscriptions?access_token=<APP_ID>|<SECRETO_APP>"
```

Tiene que devolver su `callback_url`, `"active": true` y el campo `messages`.

Y después, con un teléfono de verdad, en este orden:

| Se manda | Lo que tiene que pasar |
|---|---|
| `hola` | Aviso de privacidad **primero**, respuesta después |
| Una pregunta que no está en la base | «No tengo esa respuesta» + aparece en la cola del panel |
| Una foto y una nota de voz | Se ven y se oyen **dentro del panel** |
| Una ubicación | Sale con su municipio y sus coordenadas |
| Responder desde el panel | Llega al teléfono |

Si el bot no contesta y no hay error en el registro, **mire la base antes de
suponer**: el registro solo escribe cuando algo falla, así que el silencio
también es la señal de que todo fue bien.

---

## Las cuatro cosas que fallan

**1. El token de la consola.** Dura una sesión, no 24 horas: medido, tres horas y
media. Si el cliente le pasa el token de «Paso 1. Pruébalo», el bot se calla esa
tarde. Exija el permanente de usuario del sistema.

**2. La caché del navegador.** Tras actualizar el binario, el panel puede seguir
sirviendo el JavaScript viejo desde la caché del navegador de quien atiende.
`Ctrl+Shift+R` la primera vez. Nos costó un rato de diagnóstico creyendo que el
binario estaba mal empotrado.

**3. El número deja de funcionar en el teléfono.** Dígalo antes de conectarlo, no
después. Un cliente que atendía desde el celular y se queda sin poder hacerlo es
una llamada muy incómoda.

**4. Las conversaciones no se recuperan.** Lo que estaba en WhatsApp antes de
conectar no viaja: la API entrega desde el momento del alta. El historial previo
se queda en el teléfono.

## Cuando algo se cae en producción

| Síntoma | Dónde mirar |
|---|---|
| El bot no responde a nadie | ¿Token vivo? `curl -H "Authorization: Bearer $TOKEN" https://graph.facebook.com/v23.0/<phone_number_id>` |
| Responde a unos y a otros no | Ventana de 24 h de Meta, o número no registrado si es cuenta de prueba |
| Los webhooks dejaron de llegar | El paso 4: Meta desactiva la suscripción tras muchos fallos seguidos |
| El panel no abre | El `fallback` estático va el último en el router; si alguien tocó `lib.rs::router`, mírelo |

En la cola del panel, un fallo de envío aparece como escalado con la explicación
en castellano y el volcado técnico debajo. **Ese volcado es para usted**: dice si
es el token, un permiso o la red.

## Al terminar

- Entregue [`MANUAL-CLIENTE-META.md`](MANUAL-CLIENTE-META.md) al cliente.
- Guarde `CHASQUI_FRASE` en el gestor de contraseñas. Sin ella no hay restauración
  posible.
- Compruebe que el respaldo automático corre: `systemctl status chasqui-respaldo.timer`.
- Y compruebe que **restaura**, no solo que copia. Un respaldo que nadie ha
  restaurado nunca es una suposición.
