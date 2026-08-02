// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//
// El chat que el cliente pega en su sitio:
//
//   <script src="https://chat.suempresa.com/web/widget.js" async></script>
//
// TRES DECISIONES QUE NO SON DE ESTILO:
//
// 1. TODO va dentro de un shadow DOM. Sin él, el CSS de la página del cliente se
//    mete en el chat y el del chat se mete en su página — y el que se lleva la
//    culpa de que su sitio se vea raro es el proveedor del chat. Con shadow DOM,
//    ni entra ni sale.
// 2. La URL del servidor sale del `src` de ESTE script y no se escribe a mano.
//    Un widget que hay que configurar con la URL es un widget que alguien pega
//    mal, y el síntoma —silencio— no dice qué pasó.
// 3. El identificador de la conversación vive en `localStorage`, por servidor.
//    Es lo que hace que quien vuelve mañana encuentre su hilo. Y si el navegador
//    lo bloquea (modo privado estricto), el chat sigue funcionando: se queda en
//    memoria y dura lo que dure la pestaña, en vez de no arrancar.
// 4. EL TEMA POR DEFECTO ES CLARO, y no el del sistema del visitante. Esto se vio
//    mirándolo en un navegador de verdad y no en ninguna prueba: con el sistema
//    en oscuro, el chat salía negro encima de una página clara. El widget vive
//    DENTRO del diseño de otro, así que quien manda es el sitio, no el sistema
//    operativo de quien lo visita. El cliente pide lo que quiera:
//      data-tema="claro"  (por defecto)
//      data-tema="oscuro"
//      data-tema="auto"   (sigue al sistema, para un sitio que ya lo hace)

(function () {
  'use strict';

  var guion = document.currentScript;
  if (!guion) return;
  var BASE = new URL(guion.src, location.href).origin;
  var LLAVE = 'chasqui:' + BASE;

  // El almacenamiento puede lanzar: Safari en navegación privada, y cualquier
  // navegador con cookies de terceros bloqueadas. Que el chat no arranque por
  // eso sería peor que perder el historial al cerrar.
  var memoria = null;
  function leerId() {
    try { return localStorage.getItem(LLAVE); } catch (e) { return memoria; }
  }
  function guardarId(v) {
    memoria = v;
    try { localStorage.setItem(LLAVE, v); } catch (e) { /* queda en memoria */ }
  }

  // Los temas admitidos. Cerrado a propósito: un valor inventado no se degrada
  // a nada raro, se queda en el claro, que es el que no rompe ningún sitio.
  var TEMAS = ['claro', 'oscuro', 'auto'];
  var tema = guion.getAttribute('data-tema');
  if (TEMAS.indexOf(tema) === -1) tema = 'claro';

  function oscuro(prefijo) {
    return [
      prefijo + '.panel{background:#0d1117;color:#e6edf3}',
      prefijo + '.hilo{background:#0d1117}',
      prefijo + '.bot{background:#161b22;border-color:#30363d}',
      prefijo + '.pie{background:#0d1117;border-color:#30363d}',
      prefijo + '.pie input{background:#161b22;border-color:#30363d;color:#e6edf3}',
    ].join('\n');
  }

  var anfitrion = document.createElement('div');
  anfitrion.setAttribute('data-chasqui', '');
  anfitrion.setAttribute('data-tema', tema);
  var raiz = anfitrion.attachShadow ? anfitrion.attachShadow({ mode: 'open' }) : null;
  if (!raiz) return; // Navegador sin shadow DOM: mejor nada que romperle el sitio.

  // innerHTML con una CONSTANTE de este archivo, que es la única forma en que
  // se puede usar aquí. Lo que llega del servidor no pasa por aquí jamás: cada
  // mensaje se pinta con textContent, unas líneas más abajo. Si alguna vez se
  // interpola algo en esta plantilla, deja de ser seguro — y no hace falta,
  // porque lo variable es el contenido, no la estructura.
  raiz.innerHTML = [
    '<style>',
    ':host{all:initial}',
    '*{box-sizing:border-box;font-family:system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}',
    '.boton{position:fixed;right:20px;bottom:20px;width:56px;height:56px;border-radius:50%;',
    '  border:0;cursor:pointer;background:#1f6feb;color:#fff;font-size:24px;line-height:1;',
    '  box-shadow:0 4px 14px rgba(0,0,0,.25);z-index:2147483000}',
    '.panel{position:fixed;right:20px;bottom:88px;width:340px;max-width:calc(100vw - 40px);',
    '  height:460px;max-height:calc(100vh - 120px);display:none;flex-direction:column;',
    '  background:#fff;color:#111;border-radius:12px;overflow:hidden;',
    '  box-shadow:0 10px 40px rgba(0,0,0,.3);z-index:2147483000}',
    '.panel[data-abierto]{display:flex}',
    '.cabecera{background:#1f6feb;color:#fff;padding:12px 14px;font-weight:600;font-size:15px}',
    '.hilo{flex:1;overflow-y:auto;padding:12px;display:flex;flex-direction:column;gap:8px;',
    '  background:#f6f8fa}',
    '.linea{max-width:82%;padding:8px 11px;border-radius:12px;font-size:14px;line-height:1.4;',
    '  white-space:pre-wrap;word-wrap:break-word}',
    '.bot{align-self:flex-start;background:#fff;border:1px solid #d0d7de}',
    '.mio{align-self:flex-end;background:#1f6feb;color:#fff}',
    '.pie{display:flex;gap:8px;padding:10px;border-top:1px solid #d0d7de;background:#fff}',
    '.pie input{flex:1;padding:9px 11px;border:1px solid #d0d7de;border-radius:8px;font-size:14px}',
    '.pie button{padding:9px 14px;border:0;border-radius:8px;background:#1f6feb;color:#fff;',
    '  cursor:pointer;font-size:14px}',
    '.pie button[disabled]{opacity:.5;cursor:default}',
    // El tema oscuro, escrito una vez y aplicado en dos sitios: cuando el
    // cliente lo pide a secas, y cuando pide «auto» y el sistema está en oscuro.
    // Duplicar estas reglas es garantizar que un día se cambien en una sola.
    oscuro(':host([data-tema="oscuro"]) '),
    '@media (prefers-color-scheme:dark){',
    oscuro(':host([data-tema="auto"]) '),
    '}',
    '</style>',
    '<button class="boton" part="boton" aria-label="Abrir el chat">&#128172;</button>',
    '<section class="panel" role="dialog" aria-label="Chat">',
    '  <div class="cabecera"></div>',
    '  <div class="hilo" aria-live="polite"></div>',
    // NO HAY <form>, y es la decisión más importante de este archivo.
    //
    // Un formulario sin `action` envía a la URL actual, así que CUALQUIER envío
    // inesperado —un `submit` sintético, un navegador con su propia idea, un
    // fallo antes del `preventDefault`— **se lleva por delante la página del
    // cliente**. Se vio: la barra de direcciones acabó en «/?» y la conversación
    // se repetía sola en un bucle.
    //
    // `preventDefault` tapaba eso, que es un mecanismo puesto para contener lo
    // que creó otro. Sin formulario no hay envío que prevenir: se manda con el
    // botón y con Enter, que es todo lo que el formulario daba.
    '  <div class="pie">',
    '    <input type="text" autocomplete="off" placeholder="Escriba su mensaje…" ',
    '           aria-label="Su mensaje">',
    '    <button type="button">Enviar</button>',
    '  </div>',
    '</section>',
  ].join('\n');

  var boton = raiz.querySelector('.boton');
  var panel = raiz.querySelector('.panel');
  var cabecera = raiz.querySelector('.cabecera');
  var hilo = raiz.querySelector('.hilo');
  var pie = raiz.querySelector('.pie');
  var campo = raiz.querySelector('.pie input');
  var enviar = raiz.querySelector('.pie button');

  cabecera.textContent = guion.getAttribute('data-titulo') || 'Escríbanos';

  var ultimo = 0;
  var sondeo = null;
  var abierto = false;

  function pintar(m) {
    var d = document.createElement('div');
    d.className = 'linea ' + (m.de === 'visitante' ? 'mio' : 'bot');
    // textContent y NUNCA innerHTML: lo que devuelve el servidor es texto que
    // escribió una persona, y con innerHTML un mensaje con etiquetas se
    // ejecutaría dentro de la página del cliente.
    d.textContent = m.texto;
    hilo.appendChild(d);
    hilo.scrollTop = hilo.scrollHeight;
  }

  function cabeceras(extra) {
    var h = { 'x-chasqui-visitante': leerId() || '' };
    if (extra) for (var k in extra) h[k] = extra[k];
    return h;
  }

  function asegurarId() {
    if (leerId()) return Promise.resolve(leerId());
    return fetch(BASE + '/web/conversacion', { method: 'POST' })
      .then(function (r) {
        if (!r.ok) throw new Error('no se pudo abrir la conversación');
        return r.json();
      })
      .then(function (j) { guardarId(j.visitante); return j.visitante; });
  }

  function traer() {
    if (!leerId()) return Promise.resolve();
    return fetch(BASE + '/web/mensajes?desde=' + ultimo, { headers: cabeceras() })
      .then(function (r) { return r.ok ? r.json() : []; })
      .then(function (lista) {
        for (var i = 0; i < lista.length; i++) {
          pintar(lista[i]);
          if (lista[i].id > ultimo) ultimo = lista[i].id;
        }
      })
      // Un fallo de red no rompe el chat ni llena la consola del cliente: el
      // sondeo siguiente lo reintenta.
      .catch(function () {});
  }

  function arrancarSondeo() {
    if (sondeo) return;
    // Tres segundos: en un chat se nota poco y no castiga al servidor del
    // cliente, que es un SQLite en su propia máquina. Solo mientras esté
    // abierto — un widget que sondea con el panel cerrado gasta la batería de
    // todo el que entre a la página.
    sondeo = setInterval(traer, 3000);
  }

  function pararSondeo() {
    if (sondeo) { clearInterval(sondeo); sondeo = null; }
  }

  boton.addEventListener('click', function () {
    abierto = !abierto;
    if (abierto) {
      panel.setAttribute('data-abierto', '');
      campo.focus();
      asegurarId().then(traer).then(arrancarSondeo).catch(function () {});
    } else {
      panel.removeAttribute('data-abierto');
      pararSondeo();
    }
  });

  function mandar() {
    var texto = campo.value.trim();
    if (!texto) return;
    campo.value = '';
    enviar.disabled = true;

    asegurarId()
      .then(function () {
        return fetch(BASE + '/web/mensaje', {
          method: 'POST',
          headers: cabeceras({ 'content-type': 'application/json' }),
          body: JSON.stringify({ texto: texto }),
        });
      })
      // El mensaje propio NO se pinta al enviarlo: se pinta cuando vuelve del
      // servidor, con su id. Pintarlo antes obliga a llevar dos listas —lo que
      // se ve y lo que el servidor tiene— y a decidir qué hacer cuando el envío
      // falla; así, lo que se ve es siempre lo que quedó guardado.
      .then(traer)
      .catch(function () {})
      .then(function () { enviar.disabled = false; campo.focus(); });
  }

  enviar.addEventListener('click', mandar);
  // Enter manda, que es lo único que daba el formulario. Se excluye el Enter con
  // modificadores: quien pulsa Mayúsculas+Enter no está mandando.
  campo.addEventListener('keydown', function (e) {
    if (e.key === 'Enter' && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      mandar();
    }
  });

  document.addEventListener('visibilitychange', function () {
    if (document.hidden) pararSondeo();
    else if (abierto) { traer(); arrancarSondeo(); }
  });

  (document.body || document.documentElement).appendChild(anfitrion);
})();
