#!/usr/bin/env bash
# SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
# Copyright (c) 2026 Juan Carlos Isaza
#
# EL WIDGET, EN FOTOS. La única prueba de cómo se VE.
#
# `probar-widget.mjs` comprueba que el chat funciona; esto comprueba que se ve
# bien, que es otra cosa. Dos fallos reales aparecieron solo aquí: el chat salía
# oscuro sobre una página clara, y un `<form>` sin `action` navegaba la página
# del cliente en bucle. Ninguna prueba de Rust ni de jsdom podía verlos.
#
#   herramientas/capturar-widget.sh http://127.0.0.1:18895 [carpeta-destino]
#
# El primer argumento es el Chasqui que sirve el widget. Tiene que tener dado de
# alta el sitio `http://localhost:8090` como cuenta de canal web, y alguna
# pregunta frecuente que responda a «cuanto vale».
#
# DOS TRUCOS QUE HACEN QUE ESTO FUNCIONE, y que costaron encontrar:
#
# 1. Firefox dispara la captura en el evento `load`, y la conversación del widget
#    es asíncrona: sin más, la foto sale antes de que llegue nada. La página
#    carga un guion DELIBERADAMENTE LENTO que retrasa el `load` lo justo.
# 2. Ese retraso no puede pasarse: con 7 segundos Firefox se cansa y no escribe
#    el archivo, SIN decir nada —sale con código 0—. Tres funciona.
#
# Y cada captura usa un PERFIL LIMPIO. Con uno reutilizado, el identificador de
# la conversación sobrevive y el hilo va acumulando mensajes de corridas
# anteriores: parece un bucle y no lo es. Esa confusión ya costó una vuelta.

set -euo pipefail

CHASQUI="${1:-http://127.0.0.1:18895}"
DESTINO="${2:-$HOME/Imágenes}"
PUERTO_SITIO=8090
RETRASO=3

command -v firefox >/dev/null || { echo "hace falta firefox"; exit 1; }

TRABAJO="$(mktemp -d /tmp/chasqui-capturas-XXXXXX)"
mkdir -p "$TRABAJO/sitio" "$DESTINO"
trap 'kill ${PID_SITIO:-0} 2>/dev/null || true; rm -rf "$TRABAJO"' EXIT

# --- El servidor del sitio, con el guion lento ---
cat > "$TRABAJO/servidor.py" <<'PY'
import http.server, time, sys
RAIZ, SEGUNDOS = sys.argv[1], float(sys.argv[2])
class M(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k): super().__init__(*a, directory=RAIZ, **k)
    def do_GET(self):
        if self.path.startswith('/lento.js'):
            time.sleep(SEGUNDOS)
            c = b'/* retrasa el load para que la captura llegue con la conversacion hecha */'
            self.send_response(200)
            self.send_header('content-type', 'application/javascript')
            self.send_header('content-length', str(len(c)))
            self.end_headers(); self.wfile.write(c); return
        super().do_GET()
    def log_message(self, *a): pass
http.server.ThreadingHTTPServer(('127.0.0.1', int(sys.argv[3])), M).serve_forever()
PY

# --- La página, HOSTIL A PROPÓSITO ---
#
# Sin el CSS agresivo, la foto solo diría «se ve bien en una página que no
# molesta», y lo que hay que comprobar es que el shadow DOM aísla de verdad.
pagina() {
  cat <<HTML
<!doctype html>
<html lang="es"><head><meta charset="utf-8"><title>Muebles ACME</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  *{font-family:Georgia,serif !important;color:#7a0000 !important}
  button{background:#ff0 !important;border:4px dashed #f0f !important;border-radius:0 !important;
         font-size:26px !important;padding:24px !important}
  div{outline:2px solid #0a0 !important}
  input{background:#000 !important;color:#0f0 !important;border:3px solid #f00 !important}
  section{background:#fdd !important}
  body{margin:0;padding:32px;background:#fafafa}
</style></head>
<body>
  <h1>Muebles ACME</h1>
  <p>Página con CSS agresivo: letra serif granate forzada, botones amarillos con
     borde rosa, campos negros y contorno verde en cada div.
     El chat NO debería parecerse a esto.</p>
  <button>Un botón de la página</button>
  <script src="$CHASQUI/web/widget.js" data-titulo="Escríbanos" $1></script>
  <script>
    (function () {
      var n = 0;
      var t = setInterval(function () {
        var h = document.querySelector('[data-chasqui]');
        if (!h && n++ < 100) return;
        clearInterval(t);
        if (!h) return;
        h.shadowRoot.querySelector('.boton').click();
        setTimeout(function () {
          h.shadowRoot.querySelector('.pie input').value = 'cuanto vale';
          h.shadowRoot.querySelector('.pie button').click();
        }, 500);
      }, 50);
    })();
  </script>
  <script src="/lento.js"></script>
</body></html>
HTML
}

pagina '' > "$TRABAJO/sitio/index.html"
pagina 'data-tema="oscuro"' > "$TRABAJO/sitio/oscuro.html"

python3 "$TRABAJO/servidor.py" "$TRABAJO/sitio" "$RETRASO" "$PUERTO_SITIO" &
PID_SITIO=$!
sleep 1

# --- Las capturas ---
#
# `--no-remote` y un perfil propio: sin eso, si hay un Firefox abierto la orden
# se la pasa a ÉL y sale con código 0 sin escribir nada.
capturar() { # nombre  pagina  ancho,alto  sistema_oscuro
  local perfil="$TRABAJO/perfil-$1"
  mkdir -p "$perfil"
  printf 'user_pref("ui.systemUsesDarkTheme", %s);\nuser_pref("browser.shell.checkDefaultBrowser", false);\n' \
    "$4" > "$perfil/user.js"
  timeout 90 firefox --headless --no-remote --profile "$perfil" --window-size "$3" \
    --screenshot "$DESTINO/chasqui-widget-$1.png" \
    "http://localhost:$PUERTO_SITIO/$2" >/dev/null 2>&1 || true
  if [ -s "$DESTINO/chasqui-widget-$1.png" ]; then
    echo "  ok   $DESTINO/chasqui-widget-$1.png"
  else
    echo "  FALLA no se escribió la captura «$1»"; FALLOS=1
  fi
}

FALLOS=0
echo "Capturas del widget (cada una con perfil limpio):"
# El caso que destapó el fallo del tema: sistema OSCURO, chat que debe salir CLARO.
capturar "claro-sistema-oscuro" ""            1280,900 1
capturar "movil"                ""            390,844  0
capturar "tema-oscuro"          "oscuro.html" 900,760  0

echo
if [ "$FALLOS" = 0 ]; then
  echo "QUÉ MIRAR EN ELLAS:"
  echo "  · el chat NO debe tener letra serif granate, ni botón amarillo, ni campo negro:"
  echo "    si los tiene, el shadow DOM dejó de aislar;"
  echo "  · «claro-sistema-oscuro» debe salir CLARO aunque el sistema esté en oscuro:"
  echo "    el tema lo manda el sitio, no el visitante;"
  echo "  · el hilo debe tener UN «cuanto vale», el aviso de privacidad y la respuesta."
  echo "    Más de uno con un perfil limpio significa que la página se está recargando sola."
else
  exit 1
fi
