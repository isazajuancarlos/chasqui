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
// se monta, se abre, envía, pinta lo que el servidor devuelve, y que al volver a
// cargar la página reutiliza la conversación en vez de abrir otra. NO comprueba
// CÓMO SE VE —jsdom no pinta— ni las reglas propias de un navegador de verdad
// (la petición previa de CORS, o su política de almacenamiento en modo privado).
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
// Se espera a que el identificador exista antes de mirarlo. La primera versión
// de esta comprobación miraba 600 ms después del clic y decía que no se guardaba:
// medía antes de que la petición volviera. Un negativo solo vale si el camino que
// lo produciría llegó a ejecutarse.
let id = null;
for (let i = 0; i < 40 && !id; i++) {
  await new Promise((r) => setTimeout(r, 100));
  id = dom.window.localStorage.getItem('chasqui:' + BASE);
}
comprobar('el servidor emitió el identificador y el navegador lo guardó', !!id);

// Y se escribe.
raiz.querySelector('.pie input').value = 'cuanto vale';
// Se PULSA el botón, como una persona. La versión anterior despachaba un
// `submit` sintético, y eso enviaba el formulario de verdad: la página del
// cliente navegaba a «/?» y la conversación se repetía en bucle. El widget ya
// no tiene formulario, pero la prueba tampoco debe inventarse gestos que una
// persona no hace.
raiz.querySelector('.pie button').click();

let lineas = [];
for (let i = 0; i < 60; i++) {
  await new Promise((r) => setTimeout(r, 200));
  lineas = [...raiz.querySelectorAll('.linea')].map((l) => l.textContent);
  if (lineas.length >= 3) break;
}

console.log('  hilo:', JSON.stringify(lineas));
// El fallo que se vio en un navegador de verdad: al mandar, la página del
// cliente navegaba a «/?» y todo empezaba otra vez.
comprobar(
  'mandar NO navega la página del cliente',
  dom.window.location.href === SITIO + '/',
  `acabó en ${dom.window.location.href}`,
);
comprobar('no se repitió el mensaje', lineas.filter((l) => l === 'cuanto vale').length === 1);
comprobar('lo escrito aparece en el hilo', lineas[0] === 'cuanto vale');
comprobar('el aviso de privacidad sale antes que la respuesta', /trata/i.test(lineas[1] || ''));
comprobar('el bot contesta la pregunta frecuente', /50\.000/.test(lineas[2] || ''));
comprobar('cada lado se pinta del suyo', raiz.querySelector('.mio') && raiz.querySelector('.bot'));
comprobar('no hubo errores durante la conversación', errores.length === 0, errores.join(' | '));

// --- ENTER ---
//
// Es lo único que daba el <form> que se quitó, así que es lo primero que se
// rompe al quitarlo. Se manda una tecla de verdad, no se llama a la función.
raiz.querySelector('.pie input').value = 'horario';
raiz.querySelector('.pie input').dispatchEvent(
  new dom.window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
);

let conEnter = [];
for (let i = 0; i < 40; i++) {
  await new Promise((r) => setTimeout(r, 150));
  conEnter = [...raiz.querySelectorAll('.linea')].map((l) => l.textContent);
  if (conEnter.length > lineas.length + 1) break;
}
comprobar('Enter manda el mensaje', conEnter.includes('horario'));
comprobar('y el bot contesta', conEnter.some((l) => /lunes a s/i.test(l)));
comprobar(
  'Enter tampoco navega la página',
  dom.window.location.href === SITIO + '/',
  `acabó en ${dom.window.location.href}`,
);
lineas = conEnter;

dom.window.close();

// --- Y AHORA LA RECARGA ---
//
// Es la mitad que faltaba: que el identificador se ESCRIBA no sirve de nada si al
// volver no se LEE. Se abre una ventana nueva con el mismo almacenamiento —que es
// lo que ve el navegador al recargar— y se comprueba que el widget retoma la
// conversación en vez de abrir otra, y que repinta lo dicho.
console.log('\n  --- se recarga la página ---');

const dom2 = new JSDOM(
  `<!doctype html><html><body><script src="${BASE}/web/widget.js"></script></body></html>`,
  {
    url: SITIO + '/',
    runScripts: 'dangerously',
    resources: { fetch() { return Promise.resolve(Buffer.from(guion)); } },
    virtualConsole: consola,
    beforeParse(win) {
      win.fetch = (url, opciones = {}) =>
        fetch(url, { ...opciones, headers: { ...(opciones.headers || {}), origin: SITIO } });
      // El almacenamiento que sobrevive a la recarga.
      win.localStorage.setItem('chasqui:' + BASE, id);
    },
  },
);

await new Promise((r) => dom2.window.addEventListener('load', r));
await new Promise((r) => setTimeout(r, 200));

const raiz2 = dom2.window.document.querySelector('[data-chasqui]').shadowRoot;
raiz2.querySelector('.boton').dispatchEvent(new dom2.window.MouseEvent('click', { bubbles: true }));

let devuelto = [];
for (let i = 0; i < 40; i++) {
  await new Promise((r) => setTimeout(r, 150));
  devuelto = [...raiz2.querySelectorAll('.linea')].map((l) => l.textContent);
  if (devuelto.length >= 3) break;
}

console.log('  hilo tras recargar:', JSON.stringify(devuelto));
comprobar('el hilo sobrevive a la recarga', devuelto.length === lineas.length);
comprobar('y es el mismo, no uno nuevo', devuelto[0] === 'cuanto vale');
comprobar(
  'no se abrió una conversación distinta',
  dom2.window.localStorage.getItem('chasqui:' + BASE) === id,
);

dom2.window.close();
console.log(fallos.length === 0 ? '\nEL WIDGET FUNCIONA' : `\nFALLARON ${fallos.length}`);
process.exit(fallos.length === 0 ? 0 : 1);
