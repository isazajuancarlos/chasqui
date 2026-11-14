#!/usr/bin/env bash
# Despliega chasqui en el VPS. Se ejecuta DESDE la máquina de desarrollo.
#
#   ./despliegue/desplegar.sh debian@192.99.42.108
#
# Existe porque hasta el 2026-08-12 chasqui era el único servicio del VPS SIN
# guion: solo `INSTALAR.md` con la secuencia a mano. La configuración general
# pide que lo que toca producción vaya en un guion con la secuencia entera ya
# verificada, y el motivo es concreto — seis órdenes sueltas contra un servidor
# se equivocan en la tercera, y la que se salta suele ser el respaldo.
#
# Todo idempotente: se puede volver a lanzar.
#
# ── DOS COSAS QUE `INSTALAR.md` DICE MAL, medidas contra el servidor ──────────
#
#  1. Dice `/opt/chasqui`. **No existe.** La unidad tiene
#     `WorkingDirectory=/home/debian/chasqui` y el binario, la base y el `.env`
#     viven los tres ahí. Gana el servidor.
#  2. Dice «primero el respaldo: `systemctl start chasqui-respaldo`». **Esa
#     unidad NO está instalada** —`systemctl is-enabled` da `not-found` y no hay
#     temporizador—, así que ese paso no respaldaba nada. Por eso el respaldo lo
#     hace ESTE guion, aquí, antes de tocar nada.
#
# ── LO QUE HAY QUE SABER ANTES DE LANZARLO ───────────────────────────────────
#
# **CIERRA TODAS LAS SESIONES ABIERTAS, una vez.** `guaca::sesion` pasó a la
# forma de tres campos con revocación por `version` (guaca 0.7.0), así que las
# cookies emitidas con el formato viejo dejan de verificar. Es a propósito: si
# entraran, la versión se leería de la caducidad y el token sería inmune a la
# revocación. No es un efecto secundario que se descubre; se anuncia.
set -euo pipefail

DESTINO="${1:-}"
if [ -z "$DESTINO" ]; then
    echo "Uso: $0 usuario@servidor      (por ejemplo: debian@192.99.42.108)" >&2
    exit 2
fi

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"
BIN=target/release/chasqui
REMOTO=/home/debian/chasqui
SVC=chasqui

# El acceso se comprueba ANTES de construir: construir tarda, y el `ssh` es lo
# primero que puede fallar por algo ajeno al código.
echo "==> 0/7  comprobar el acceso a $DESTINO"
ssh -o ConnectTimeout=10 "$DESTINO" true || {
    echo "No se pudo entrar en «$DESTINO». No se construyó nada." >&2
    exit 3
}

echo "==> 1/7  construir"
cargo build --release

# Que el binario lleve el panel DENTRO se comprueba antes de subir 10 MB.
#
# Las marcas se eligen por lo que el compilador NO puede cazar: que `$BIN` sea un
# binario de ESTE proyecto y no otro que quedó ahí. NO distinguen un binario
# nuevo de uno rancio —eso lo dice el `md5` contra el recién construido, que es
# el paso 4— y conviene no confundir las dos cosas: el guion de inventario
# afirmaba lo segundo comprobando lo primero.
echo "==> 2/7  comprobar que el binario lleva el panel dentro"
for marca in "htmx" "boi" "chasqui"; do
    grep -qa "$marca" "$BIN" || {
        echo "El binario no lleva «$marca» dentro: ¿es el que acaba de construir" >&2
        echo "\`cargo build --release\`? No se subió nada." >&2
        exit 4
    }
done
echo "    $(du -h "$BIN" | cut -f1)  ✔"

# RESPALDO ANTES DE PISAR NADA. La base y el binario, con marca de tiempo. No es
# ceremonia: es la única vía de vuelta, y la unidad `chasqui-respaldo` que el
# documento invocaba no está instalada.
MARCA="$(date +%Y%m%d-%H%M%S)"
echo "==> 3/7  respaldo con marca $MARCA"
ssh "$DESTINO" "set -e
    cd $REMOTO
    cp -a chasqui chasqui.bak-$MARCA
    # La base se copia con \`sqlite3 .backup\` si está la herramienta —una copia
    # en caliente con \`cp\` puede quedar a medias si hay una escritura en
    # curso—, y con \`cp\` si no, diciéndolo.
    if command -v sqlite3 >/dev/null 2>&1; then
        sqlite3 chasqui.db \".backup 'chasqui.db.bak-$MARCA'\"
        echo '    base respaldada con sqlite3 .backup'
    else
        cp -a chasqui.db chasqui.db.bak-$MARCA
        echo '    OJO: sin sqlite3, la base se copió con cp (puede quedar a medias si escribía)'
    fi
    ls -la chasqui.bak-$MARCA chasqui.db.bak-$MARCA | sed 's/^/    /'"

# El `.env` NO se toca NUNCA desde aquí: lleva `CHASQUI_SECRETO_APP` y
# `CHASQUI_FRASE` en uso. Reescribirlo invalidaría lo cifrado y las sesiones.
echo "==> 4/7  subir el binario"
scp -q "$BIN" "$DESTINO:/tmp/chasqui.nuevo"
# Se escribe al lado y se renombra encima: abrir con O_TRUNC un binario que se
# está ejecutando da ETXTBSY. `mv` sustituye la entrada del directorio, el
# proceso vivo conserva su inodo hasta el reinicio, y no hay un instante en que
# la ruta no exista.
ssh "$DESTINO" "set -e
    install -m 0755 /tmp/chasqui.nuevo $REMOTO/chasqui.nuevo
    mv -f $REMOTO/chasqui.nuevo $REMOTO/chasqui
    rm -f /tmp/chasqui.nuevo"

# md5 a los dos lados: `scp` puede truncar y salir 0 si el disco se llena.
LOCAL="$(md5sum "$BIN" | cut -d' ' -f1)"
ARRIBA="$(ssh "$DESTINO" "md5sum $REMOTO/chasqui | cut -d' ' -f1")"
[ "$LOCAL" = "$ARRIBA" ] || {
    echo "el binario no llegó igual ($LOCAL vs $ARRIBA). Volviendo atrás." >&2
    ssh "$DESTINO" "cp -a $REMOTO/chasqui.bak-$MARCA $REMOTO/chasqui"
    exit 5
}
echo "    md5 $LOCAL  ✔"

echo "==> 5/7  reiniciar"
ssh "$DESTINO" "sudo -n systemctl restart $SVC && sleep 5 && systemctl is-active $SVC"

echo "==> 6/7  el log, no solo el código de salida"
ssh "$DESTINO" "journalctl -u $SVC -n 8 --no-pager | sed 's/^/    /'"

# LA VERDAD DE LO DESPLEGADO ES EL `curl` CONTRA EL DOMINIO, no `is-active`: un
# servicio puede estar «activo» sirviendo la versión anterior —`Restart=always`
# lo relanza solo— y `systemctl restart` puede devolver 0 mientras la aplicación
# revienta al primer request. Hoy mismo se dio el caso hermano: la demo de
# informes llevaba días caída y su dominio contestaba 303 porque respondía nginx.
echo "==> 7/7  comprobar contra el DOMINIO"
# **SIGUIENDO LA REDIRECCIÓN (`-L`), y aquí me equivoqué la primera vez.** La
# raíz responde 303 hacia `/cola` y de ahí a `/entrar`; un 303 NO TIENE CUERPO,
# así que buscar «chasqui» dentro de la redirección no puede acertar nunca. La
# comprobación se disparaba siempre, sobre un despliegue correcto — la alarma que
# a la tercera nadie mira, y encima estrenada en su primera corrida.
#
# Lo que se quiere saber es si contesta LA APLICACIÓN o solo nginx, y eso solo se
# ve en la página final.
VIVO=$(mktemp)
CODIGO="$(curl -sL -o "$VIVO" -w '%{http_code}' -m 15 https://chasqui.xiliux.com/ || echo 000)"
FINAL="$(curl -sL -o /dev/null -w '%{url_effective}' -m 15 https://chasqui.xiliux.com/ || echo '?')"
echo "    https://chasqui.xiliux.com/  →  $CODIGO en $FINAL"
if [ "$CODIGO" != "200" ] || ! grep -qi "chasqui" "$VIVO" 2>/dev/null; then
    echo "    OJO: no llegó a servir la pantalla de chasqui. Puede ser nginx contestando" >&2
    echo "    por ese nombre, o la aplicación reventando al primer request." >&2
    echo "    La vuelta atrás está al final de este mensaje." >&2
    rm -f "$VIVO"
    exit 6
fi
rm -f "$VIVO"
echo "    y la página final nombra a chasqui  ✔"

cat <<FIN

Desplegado. El binario anterior queda en $REMOTO/chasqui.bak-$MARCA y la base en
chasqui.db.bak-$MARCA.

Para volver atrás:
    ssh $DESTINO "cd $REMOTO && cp -a chasqui.bak-$MARCA chasqui && sudo -n systemctl restart $SVC"

RECUERDE: las sesiones abiertas se han cerrado, una vez, por el cambio de formato
del token (guaca 0.7.0). Quien estuviera dentro tiene que volver a entrar.
FIN
