// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//
// EL WIDGET, EJECUTÁNDOSE DE VERDAD (directiva 28).
//
// Las pruebas de Rust comprueban que `/web/widget.js` se sirve y que dice las
// palabras correctas. Eso mide el transporte, no el producto: un widget que
// revienta en la primera línea pasa esas pruebas igual de bien que uno que
// funciona. Aquí se carga en un DOM real (jsdom), con `fetch` de verdad contra un
// Chasqui de verdad, y se pulsa el botón como lo pulsa una persona.
//
//   node herramientas/probar-widget.mjs http://127.0.0.1:8787 https://acme.com
//
// ALCANCE, para que el verde no diga más de lo que sabe: comprueba que el widget
// se monta, se abre, envía y pinta lo que el servidor devuelve. NO comprueba cómo
// se ve —jsdom no pinta— ni que el identificador sobreviva a una recarga.
//
// El segundo argumento es el origen dado de alta: jsdom lo usa como la URL de la
// página, así que el navegador manda ese `Origin`, que es justo lo que se quiere
// probar.

import { JSDOM, VirtualConsole } from '../interfaz/node_modules/jsdom/lib/api.js';

const BASE = process.argv[2] || 'http://127.0.0.1:8787';
const SITIO = process.argv[3] || 'https://acme.com';

const fallos = [];
function comprobar(que, condicion, detalle = '') {
  if (condicion) {
    console.log(`  ok   ${que}`);
  } else {
    console.log(`  FALLA ${que} ${detalle}`);
    fallos.push(que);
  }
}

const guion = await (await fetch(`${BASE}/web/widget.js`)).text();

// La consola del DOM se reenvía: un error dentro del widget no puede pasar
// desapercibido, que es exactamente lo que este guion existe para evitar.
const consola = new VirtualConsole();
const errores = [];
consola.on('jsdomError', (e) => errores.push(String(e)));
consola.on('error', (...a) => errores.push(a.join(' ')));

const dom = new JSDOM(
  `<!doctype html><html><body><h1>Sitio del cliente</h1>
   <script src="${BASE}/web/widget.js"></script></body></html>`,
  {
    url: SITIO + '/',
    runScripts: 'dangerously',
    resources: {
      // jsdom no descarga el guion solo; se le entrega el que ya se bajó, que es
      // el MISMO que sirve el binario.
      fetch() { return Promise.resolve(Buffer.from(guion)); },
    },
    virtualConsole: consola,
    beforeParse(win) {
      // El `fetch` del navegador simulado va al Chasqui real, añadiendo el
      // `Origin` que jsdom no pone por sí mismo.
      win.fetch = (url, opciones = {}) => {
        const cabeceras = { ...(opciones.headers || {}), origin: SITIO };
        return fetch(url, { ...opciones, headers: cabeceras });
      };
    },
  },
);

await new Promise((r) => dom.window.addEventListener('load', r));
await new Promise((r) => setTimeout(r, 200));

const doc = dom.window.document;
const anfitrion = doc.querySelector('[data-chasqui]');
comprobar('el widget se monta en la página', !!anfitrion);
comprobar('sin errores al cargar', errores.length === 0, errores.join(' | '));

const raiz = anfitrion && anfitrion.shadowRoot;
comprobar('vive dentro de un shadow DOM', !!raiz);
comprobar('no ensucia el DOM del cliente', doc.querySelectorAll('.panel').length === 0);

const boton = raiz && raiz.querySelector('.boton');
comprobar('hay un botón para abrirlo', !!boton);

// Se abre, como lo abre una persona.
boton.dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true }));
await new Promise((r) => setTimeout(r, 600));

const panel = raiz.querySelector('.panel');
comprobar('al pulsar se abre', panel.hasAttribute('data-abierto'));
// LO QUE ESTE GUION NO PUEDE DECIR.
//
// El identificador se guarda en `localStorage` para que quien vuelva mañana
// encuentre su hilo. En jsdom no queda guardado —y aun así la conversación
// funciona, porque el widget cae a memoria—, pero **eso no es evidencia sobre un
// navegador de verdad**: jsdom no lo es. Así que se informa y no se falla: dar
// esto por roto sería tan falso como darlo por bueno, y confundir «no pude
// mirar» con «vi algo malo» manda a arreglar lo que no está mal.
//
// PENDIENTE: comprobarlo en un navegador de verdad, recargando la página.
const guardado = !!dom.window.localStorage.getItem('chasqui:' + BASE);
console.log(
  `  ~    persistencia entre recargas: ${guardado ? 'guardada' : 'NO VERIFICADA aquí'}` +
    ' (jsdom no decide esto; hace falta un navegador)',
);

// Y se escribe.
raiz.querySelector('.pie input').value = 'cuanto vale';
raiz.querySelector('.pie').dispatchEvent(new dom.window.Event('submit'));

let lineas = [];
for (let i = 0; i < 60; i++) {
  await new Promise((r) => setTimeout(r, 200));
  lineas = [...raiz.querySelectorAll('.linea')].map((l) => l.textContent);
  if (lineas.length >= 3) break;
}

console.log('  hilo:', JSON.stringify(lineas));
comprobar('lo escrito aparece en el hilo', lineas[0] === 'cuanto vale');
comprobar('el aviso de privacidad sale antes que la respuesta', /trata/i.test(lineas[1] || ''));
comprobar('el bot contesta la pregunta frecuente', /50\.000/.test(lineas[2] || ''));
comprobar('cada lado se pinta del suyo', raiz.querySelector('.mio') && raiz.querySelector('.bot'));
comprobar('no hubo errores durante la conversación', errores.length === 0, errores.join(' | '));

dom.window.close();
console.log(fallos.length === 0 ? '\nEL WIDGET FUNCIONA' : `\nFALLARON ${fallos.length}`);
process.exit(fallos.length === 0 ? 0 : 1);
