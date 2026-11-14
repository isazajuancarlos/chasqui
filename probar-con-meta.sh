#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
# Copyright (c) 2026 Juan Carlos Isaza
#
# Levanta Chasqui detrás de una URL pública con HTTPS para poder darlo de alta
# como webhook en la consola de Meta, y comprueba solo el apretón de manos.
#
# POR QUÉ EXISTE: todas las pruebas del repositorio hablan con un Meta de mentira
# en localhost. Miden el cableado, no el producto contra el mundo. Esto es lo más
# cerca que se puede llegar sin una cuenta de Meta, y lo que falta después está
# escrito en docs/META.md.
#
# NO USA DATOS DE NADIE: crea una base nueva en un directorio temporal y la borra
# al salir. Nunca toca la instalación de un cliente.

set -euo pipefail

PUERTO="${CHASQUI_PUERTO:-18844}"
RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TRABAJO="$(mktemp -d /tmp/chasqui-meta-XXXXXX)"

# Los secretos de LA APP DE META. Si no se dan, se usan unos de prueba y el alta
# del webhook en la consola no funcionará — sirve para probar el túnel y poco más.
SECRETO_APP="${CHASQUI_SECRETO_APP:-secreto-de-prueba-sin-app-real}"
TOKEN_VERIFICACION="${CHASQUI_TOKEN_VERIFICACION:-chasqui-verificacion-de-prueba}"

# Un `curl` que falla NO aborta el guion. Con `set -e`, el primer 403 —que es un
# resultado ESPERADO— se llevaría por delante todas las comprobaciones que vienen
# después, y el guion diría que falló lo que en realidad no llegó a probarse.
codigo() { curl -s -o /dev/null -w '%{http_code}' --max-time 15 "$1" || echo 000; }
cuerpo() { curl -s --max-time 15 "$1" || true; }

limpiar() {
    # Con -9 detrás de la cortesía: `cloudflared` no siempre se va con SIGTERM, y
    # un túnel público que sobrevive al guion es exactamente lo que no puede pasar.
    for pid in "${PID_TUNEL:-}" "${PID_SERVIDOR:-}"; do
        [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
    done
    sleep 1
    for pid in "${PID_TUNEL:-}" "${PID_SERVIDOR:-}"; do
        [ -n "$pid" ] && kill -9 "$pid" 2>/dev/null || true
    done
    echo
    if [ "${CONSERVAR:-no}" = "si" ]; then
        # Algo falló: el registro se queda. La primera versión de este guion
        # borraba el directorio en el mismo `trap` y luego decía «mire
        # $TRABAJO/tunel.log», que para entonces ya no existía — mandaba a
        # buscar la prueba justo después de destruirla.
        echo "cerrado: túnel abajo. Los registros SE CONSERVAN en $TRABAJO"
    else
        # La base de la prueba se va con el directorio: no se queda una instancia
        # medio configurada que alguien confunda mañana con la de un cliente.
        rm -rf "$TRABAJO"
        echo "cerrado: túnel abajo y base de prueba borrada."
    fi
}
# EXIT limpia; INT y TERM solo SALEN, y al salir dispara EXIT.
#
# Estaban los tres en el mismo `trap` y no funcionaba: bash aplaza el manejador
# hasta que termina el comando en primer plano, y el último de este guion es un
# `tail -f` que no termina nunca. Resultado medido el 2026-08-02: un `kill` al
# guion NO lo mataba y **el túnel público seguía arriba**, que es exactamente lo
# que el comentario de `limpiar` dice que no puede pasar. Con `wait` (abajo) la
# señal sí interrumpe, porque `wait` es interrumpible y `tail -f` no.
trap limpiar EXIT
trap 'exit 143' INT TERM

echo "== 1. El binario =="
BINARIO="$RAIZ/target/release/chasqui"
if [ ! -x "$BINARIO" ]; then
    BINARIO="$RAIZ/target/debug/chasqui"
    [ -x "$BINARIO" ] || { echo "compilando…"; (cd "$RAIZ" && cargo build); }
fi
echo "   $BINARIO"

echo "== 2. cloudflared (túnel rápido, sin cuenta) =="
CLOUDFLARED="$(command -v cloudflared || true)"
if [ -z "$CLOUDFLARED" ]; then
    CLOUDFLARED="$TRABAJO/cloudflared"
    echo "   descargando…"
    curl -sSL -o "$CLOUDFLARED" \
        "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64"
    chmod +x "$CLOUDFLARED"
fi
echo "   $("$CLOUDFLARED" --version)"

echo "== 3. Instancia de prueba en $TRABAJO =="
export CHASQUI_BD="sqlite://$TRABAJO/prueba.db"
export CHASQUI_FRASE="frase de despliegue efímera para la prueba con Meta 1234"
export CHASQUI_SECRETO_APP="$SECRETO_APP"
export CHASQUI_TOKEN_VERIFICACION="$TOKEN_VERIFICACION"
export CHASQUI_PUERTO="$PUERTO"
export CHASQUI_ESCUCHA=127.0.0.1
# El túnel termina el TLS: hacia aquí llega http. Es el mismo caso que nginx
# delante, y por eso hay que pedirlo explícitamente.
export CHASQUI_SIN_TLS=1

"$BINARIO" ajuste poner guion.aviso_privacidad \
    "Prueba de Chasqui. Sus datos los trata el negocio de prueba." > /dev/null

if [ -n "${CHASQUI_PHONE_NUMBER_ID:-}" ] && [ -n "${CHASQUI_TOKEN_META:-}" ]; then
    printf '%s' "$CHASQUI_TOKEN_META" | "$BINARIO" cuenta añadir \
        --canal whatsapp --id "$CHASQUI_PHONE_NUMBER_ID" --nombre "Prueba" > /dev/null
    echo "   cuenta $CHASQUI_PHONE_NUMBER_ID dada de alta: RESPONDERÁ DE VERDAD"
    # Lo que se carga aquí es el MVP ENTERO, no un saludo. La primera petición
    # de Workana pide tres cosas —preguntas frecuentes, precios y tomar el
    # pedido por la conversación— y las tres tienen que poder enseñarse en la
    # misma sesión de prueba. Hasta el 2026-08-02 esto cargaba una sola
    # pregunta, y por eso los flujos y los pedidos nunca habían cruzado la
    # frontera contra Meta de verdad (directiva 28).
    cat > "$TRABAJO/preguntas.json" <<'JSON'
[
  {"titulo":"Saludo","respuesta":"Hola. Esto es una prueba de Chasqui y funciona.","disparadores":["hola","prueba","buenas"]},
  {"titulo":"Horario","respuesta":"Atendemos de lunes a sábado, de 8:00 a 18:00.","disparadores":["horario","a qué hora","abren","cierran"]},
  {"titulo":"Precios","respuesta":"La silla cuesta $120.000 y la mesa $340.000. El domicilio dentro de la ciudad es gratis.","disparadores":["precio","cuánto cuesta","valor","cuanto vale"]}
]
JSON
    "$BINARIO" conocimiento cargar "$TRABAJO/preguntas.json" > /dev/null

    # El flujo: tomar un pedido. `cantidad` va tipada a propósito — es lo que
    # hace que «tres» no pase por una cantidad y el pedido no le llegue al
    # negocio con un campo inservible.
    cat > "$TRABAJO/flujos.json" <<'JSON'
[
  {
    "titulo": "Pedido",
    "disparadores": ["pedido", "quiero pedir", "comprar", "hacer un pedido"],
    "pasos": [
      {"clave":"producto",  "pregunta":"¿Qué producto quiere? Tenemos sillas y mesas."},
      {"clave":"cantidad",  "pregunta":"¿Cuántas unidades?", "tipo_dato":"numero"},
      {"clave":"direccion", "pregunta":"¿A qué dirección se lo llevamos?"}
    ],
    "confirmacion": "Anotado: {cantidad} de {producto} para {direccion}. Le confirmamos por aquí. ¡Gracias!"
  }
]
JSON
    "$BINARIO" flujo cargar "$TRABAJO/flujos.json" > /dev/null
    echo "   cargado: 3 preguntas frecuentes y el flujo «Pedido» (3 pasos)"

    # Y una persona para el panel: sin esto no se puede probar el escalado
    # entero, que es la mitad que se vende. La contraseña se imprime porque la
    # instancia es efímera y se borra al salir.
    #
    # `$RANDOM` NO vale aquí, y no por purismo: el túnel publica el binario
    # ENTERO, así que el panel queda accesible desde internet igual que el
    # webhook. Dos `$RANDOM` son unos 30 bits y los siembra el PID y la hora —
    # adivinables—; esto son 128 bits de un generador criptográfico.
    CLAVE_PANEL="prueba-$(openssl rand -hex 16)"
    printf '%s\n' "$CLAVE_PANEL" | "$BINARIO" usuario crear \
        --correo agente@prueba.local --nombre "Agente de prueba" > /dev/null 2>&1
else
    echo "   SIN cuenta de WhatsApp: los mensajes que lleguen se registrarán y NO se contestarán."
    echo "   Para contestar de verdad, exporte CHASQUI_PHONE_NUMBER_ID y CHASQUI_TOKEN_META."
fi

"$BINARIO" servir > "$TRABAJO/servidor.log" 2>&1 &
PID_SERVIDOR=$!
for _ in $(seq 1 40); do
    curl -sf "http://127.0.0.1:$PUERTO/salud" >/dev/null 2>&1 && break
    sleep 0.25
done

echo "== 4. Túnel público =="
"$CLOUDFLARED" tunnel --url "http://127.0.0.1:$PUERTO" --no-autoupdate \
    > "$TRABAJO/tunel.log" 2>&1 &
PID_TUNEL=$!
URL=""
for _ in $(seq 1 60); do
    URL="$(grep -oE 'https://[a-z0-9-]+\.trycloudflare\.com' "$TRABAJO/tunel.log" | head -1 || true)"
    [ -n "$URL" ] && break
    sleep 1
done
[ -n "$URL" ] || { echo "no se obtuvo URL pública; mire $TRABAJO/tunel.log"; exit 1; }
echo "   $URL"

# El nombre del túnel TARDA EN EXISTIR EN EL DNS, y esa espera hay que
# respetarla sin tocar nada. MEDIDO el 2026-08-01:
#
#   - sondeando cada segundo desde el momento en que aparece la URL, el nombre
#     NO resolvió nunca, ni a los tres minutos;
#   - sin consultar el DNS durante 75 s, resolvió a la primera y devolvió 200.
#
# La causa es la CACHÉ NEGATIVA: la primera consulta llega antes de que exista el
# registro, el resolutor se guarda el «no existe» durante su TTL, y a partir de
# ahí da igual cuántas veces se reintente — se responde desde la caché. Reintentar
# más deprisa lo empeora, que es exactamente lo contrario de lo que uno haría.
#
# Por eso: se espera ANTES de la primera consulta, y luego se sondea despacio.
ESPERA_INICIAL=30
INTERVALO=20
INTENTOS=12   # 30 s + 12 × 20 s ≈ 4,5 minutos
printf '   el nombre tarda en existir en el DNS; esperando %ss antes de mirar' "$ESPERA_INICIAL"
sleep "$ESPERA_INICIAL"
LISTO=no
SEGUNDOS=$ESPERA_INICIAL
for _ in $(seq 1 "$INTENTOS"); do
    if [ "$(codigo "$URL/salud")" = "200" ]; then LISTO=si; break; fi
    printf '.'
    sleep "$INTERVALO"
    SEGUNDOS=$((SEGUNDOS + INTERVALO))
done
echo
if [ "$LISTO" != "si" ]; then
    CONSERVAR=si
    echo "   la URL pública no respondió en ${SEGUNDOS}s."
    echo "   Mire $TRABAJO/tunel.log — puede ser que Cloudflare esté limitando"
    echo "   los túneles rápidos desde esta IP; en ese caso, esperar y repetir."
    exit 1
fi
echo "   publicada en ${SEGUNDOS}s"

echo "== 5. Comprobación del apretón de manos, por internet =="
comprobar() { # descripción, esperado, código
    if [ "$3" = "$2" ]; then echo "   OK   $1 ($3)"; else echo "   FALLA $1: $3, se esperaba $2"; fi
}

comprobar "salud" 200 "$(codigo "$URL/salud")"

RETO=1158201444
DEVUELTO="$(cuerpo "$URL/webhook/whatsapp?hub.mode=subscribe&hub.verify_token=$TOKEN_VERIFICACION&hub.challenge=$RETO")"
if [ "$DEVUELTO" = "$RETO" ]; then
    echo "   OK   el alta del webhook devuelve el reto TAL CUAL"
else
    echo "   FALLA el alta del webhook devolvió [$DEVUELTO]"
fi

comprobar "alta con token equivocado" 403 \
    "$(codigo "$URL/webhook/whatsapp?hub.mode=subscribe&hub.verify_token=el-de-otro&hub.challenge=$RETO")"

CUERPO='{"object":"whatsapp_business_account","entry":[{"changes":[{"value":{"metadata":{"phone_number_id":"X"},"messages":[{"from":"573001","id":"wamid.SH","timestamp":"1785000000","type":"text","text":{"body":"hola"}}]}}]}]}'
FIRMA="sha256=$(printf '%s' "$CUERPO" | openssl dgst -sha256 -hmac "$SECRETO_APP" | awk '{print $2}')"
# LA COMPROBACIÓN QUE IMPORTA: si el proxy tocara un solo byte del cuerpo, la
# firma no casaría — y ese fallo en local no se ve nunca.
comprobar "webhook firmado (la firma sobrevive al proxy)" 200 \
    "$(curl -s -o /dev/null -w '%{http_code}' --max-time 15 -X POST "$URL/webhook/whatsapp" \
        -H 'content-type: application/json' -H "x-hub-signature-256: $FIRMA" --data-binary "$CUERPO" || echo 000)"

comprobar "webhook sin firma" 403 \
    "$(curl -s -o /dev/null -w '%{http_code}' --max-time 15 -X POST "$URL/webhook/whatsapp" \
        -H 'content-type: application/json' --data-binary "$CUERPO" || echo 000)"

cat <<FIN

========================================================================
  PEGUE ESTO EN LA CONSOLA DE META
  (developers.facebook.com → su app → WhatsApp → Configuración → Webhooks)

  URL de devolución de llamada:  $URL/webhook/whatsapp
  Token de verificación:         $TOKEN_VERIFICACION

  Suscríbase al campo «messages» — Y ADEMÁS suscriba la APP a la cuenta de
  WhatsApp Business, que la consola NO hace y no menciona:

      curl -s -X POST -H "Authorization: Bearer \$CHASQUI_TOKEN_META" \\
        "https://graph.facebook.com/v23.0/<WABA_ID>/subscribed_apps"

  Sin eso Meta recibe el mensaje, lo enseña en su propio panel y no se lo
  entrega a usted. Todo en verde y no llega nada.

  Los pasos completos, y lo que hay que mirar en cada uno, están en
  docs/META.md.
========================================================================

  QUÉ PROBAR POR WHATSAPP, escribiéndole al número:

    «hola»            → contesta el saludo (y antes, el aviso de privacidad)
    «cuánto cuesta»   → contesta los precios
    «quiero pedir»    → arranca el flujo de 3 pasos y termina en un pedido
    una nota de voz   → no la entiende, escala a una persona y CALLA

  EL PANEL, para hacer de esa persona:  http://127.0.0.1:$PUERTO
    correo:      agente@prueba.local
    contraseña:  ${CLAVE_PANEL:-<no se creó: falta la cuenta de WhatsApp>}
========================================================================

El túnel muere al cerrar esta terminal, y con él la URL. Si Meta ya estaba
suscrito a esa URL, dejará de funcionar: es un túnel de usar y tirar, para
una sesión de pruebas.

OJO: el túnel publica el BINARIO ENTERO, no solo /webhook. El panel también
queda accesible desde internet en esa URL mientras esto corra. Por eso la
contraseña de arriba es aleatoria de verdad y por eso esto se cierra al
terminar la prueba en vez de dejarlo levantado.

Ctrl-C para terminar. Registro del servidor:

FIN

# En segundo plano y con `wait`: ver el comentario de los `trap`. Con
# `tail -f` en primer plano, Ctrl-C y `kill` se quedaban pendientes.
tail -f "$TRABAJO/servidor.log" &
wait $!
