<!-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas -->
# Instalar Chasqui

Para quien administre el servidor. Una página.

Chasqui es **un ejecutable y un archivo SQLite**. No hay contenedor que
orquestar, ni base de datos que administrar aparte, ni servicio nuestro del que
dependa: si el servidor de usted está encendido, Chasqui atiende. Esa es también
la razón por la que las conversaciones de sus clientes no salen de aquí.

## Lo que hace falta antes

- Linux con systemd (las plantillas son para eso; sin systemd, ver «otro
  arranque» al final).
- **Un nombre de dominio con certificado TLS válido.** Meta exige `https` para
  dar de alta un webhook, así que esto no es opcional ni para probar.
- Un proxy inverso delante. Aquí se documenta nginx; sirve cualquiera.
- Del negocio: su app de Meta creada, y a mano el *App Secret* y el
  *phone number id* de su número.

## 1. Poner los archivos

```bash
sudo useradd --system --home /var/lib/chasqui --shell /usr/sbin/nologin chasqui
sudo install -d -m 0750 -o chasqui -g chasqui /var/lib/chasqui
sudo install -d -m 0750 -o chasqui -g chasqui /var/respaldos/chasqui
sudo install -d -m 0755 /opt/chasqui
sudo install -m 0755 chasqui /opt/chasqui/chasqui
```

## 2. Los secretos

```bash
sudo install -d -m 0750 -o root -g chasqui /etc/chasqui
sudo install -m 0640 -o root -g chasqui despliegue/chasqui.env.ejemplo /etc/chasqui/chasqui.env
sudo nano /etc/chasqui/chasqui.env      # rellenar las tres primeras
openssl rand -base64 48                 # para CHASQUI_FRASE
```

> **`CHASQUI_FRASE` cifra el token de Meta y la clave del modelo. Si se pierde,
> lo guardado no se puede descifrar.** Guárdela donde guarde las claves del
> negocio — no solo en este archivo, que vive en el mismo disco que la base.

## 3. Arrancar

```bash
sudo cp despliegue/chasqui.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now chasqui
systemctl status chasqui
```

Si no arranca, el motivo está completo en `journalctl -u chasqui -n 30`: falta un
secreto, o la frase es corta, o el valor es uno de ejemplo. **Que no arranque es
la intención**; un Chasqui a medio configurar aceptaría webhooks falsificados sin
dar ningún error.

## 4. El proxy y el certificado

```bash
sudo cp despliegue/chasqui.nginx /etc/nginx/sites-available/chasqui
sudo sed -i 's/CHATBOT.SUDOMINIO.COM/chat.suempresa.com/g' /etc/nginx/sites-available/chasqui
sudo ln -s /etc/nginx/sites-available/chasqui /etc/nginx/sites-enabled/
# Y las dos zonas de límite, en el bloque http {} de /etc/nginx/nginx.conf:
#   limit_req_zone $binary_remote_addr zone=chasqui_login:10m rate=5r/m;
#   limit_req_zone $binary_remote_addr zone=chasqui_panel:10m rate=30r/s;
sudo nginx -t && sudo systemctl reload nginx
sudo certbot --nginx -d chat.suempresa.com
```

## 5. Comprobar que está de verdad

La verdad es lo que responde el dominio, no `systemctl is-active`: un servicio
puede estar «activo» y reventar en la primera petición.

```bash
curl -s https://chat.suempresa.com/salud                       # -> vivo
curl -sI https://chat.suempresa.com/ | grep -iE 'strict|content-security|x-frame'
# El apretón de manos del webhook, como lo hace Meta:
curl -s "https://chat.suempresa.com/webhook/whatsapp?hub.mode=subscribe\
&hub.verify_token=EL_TOKEN_QUE_PUSO&hub.challenge=12345"       # -> 12345
```

Si lo último no devuelve `12345` **tal cual**, el alta en la consola de Meta
fallará sin explicar por qué.

## 6. Configurar el negocio

Con el servicio ya arriba, y como el usuario `chasqui`:

```bash
sudo -u chasqui env $(sudo cat /etc/chasqui/chasqui.env | grep -v '^#' | xargs) \
     /opt/chasqui/chasqui usuario crear --correo ana@suempresa.com --nombre Ana
```

El resto se hace desde el panel, en `https://chat.suempresa.com/`. La secuencia
completa —aviso de privacidad, número, preguntas frecuentes, plazo de
retención— está en el [`README.md`](../README.md).

**El aviso de privacidad y el plazo de retención no tienen valor por defecto y
sin ellos Chasqui no atiende ni borra nada.** No es un descuido: los dos los
tiene que decidir el responsable del tratamiento, que es el negocio.

## 7. Los respaldos

```bash
sudo cp despliegue/chasqui-respaldo.service despliegue/chasqui-respaldo.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now chasqui-respaldo.timer
sudo systemctl start chasqui-respaldo        # una vez a mano, para verlo funcionar
ls /var/respaldos/chasqui/
```

No para el servicio: la copia se toma con `VACUUM INTO`, bajo una transacción de
lectura. Y **se verifica sola** —se abre, se le pasa `integrity_check` y se
comparan los recuentos con el origen—; si algo no cuadra, la unidad queda en
`failed` y sale en `systemctl --failed`.

> **El plazo de podado (`-mtime +30` en la unidad) es una decisión jurídica.**
> Cuando alguien ejerce su derecho de supresión, `chasqui olvidar` borra su
> historial de la base viva, pero no de las copias de días anteriores: mientras
> un respaldo exista, ese dato existe. Ese número es, por tanto, el plazo real en
> que la supresión queda completa, y **tiene que ser el mismo que declare su
> política de tratamiento**.

### Ensaye la restauración antes de necesitarla

Un respaldo que nunca se restauró es una suposición. En una máquina aparte:

```bash
sudo systemctl stop chasqui
sudo -u chasqui /opt/chasqui/chasqui restaurar /var/respaldos/chasqui/chasqui-2026-08-01
sudo systemctl start chasqui
```

Se niega a pisar una instalación que ya tiene datos; `--forzar` es para cuando
eso es exactamente lo que quiere. Y **restaure con la misma `CHASQUI_FRASE`**:
sin ella la base se abre pero el token de Meta no se descifra, y Chasqui se para
en vez de responder con un token que no es el suyo.

## Actualizar

```bash
sudo systemctl start chasqui-respaldo        # primero el respaldo
sudo systemctl stop chasqui
sudo install -m 0755 chasqui-nuevo /opt/chasqui/chasqui
sudo systemctl start chasqui                 # las migraciones se aplican solas
```

## Otro arranque (sin systemd)

Chasqui solo necesita las variables de entorno y un directorio donde escribir.
Cualquier supervisor sirve; lo que no puede faltar es el usuario propio, el
`umask 0077` y que escuche solo en `127.0.0.1` con el proxy delante.

## Si algo va mal

| Síntoma | Casi siempre es |
|---|---|
| No arranca | falta un secreto en `/etc/chasqui/chasqui.env`; el motivo exacto está en `journalctl -u chasqui` |
| El alta del webhook en Meta falla | el `hub.verify_token` no coincide, o el dominio no tiene TLS válido |
| El webhook llega y no pasa nada | falta **suscribir el campo `messages`** en la consola de Meta: sin eso el alta queda hecha y no llega ni un mensaje, sin ningún error |
| Firma inválida en el log | el `App Secret` no es el de esa app, o hay un proxy en medio reescribiendo el cuerpo |
| «no se pudo descifrar el token» | la `CHASQUI_FRASE` no es la misma con la que se guardó |
| Entra al panel y lo saca | falta el TLS: sin `https` la cookie de sesión no se guarda |
