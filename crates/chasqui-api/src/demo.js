// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//
// La página de las dos mitades. Habla con las mismas rutas que el widget de
// verdad (`/web/*`) más las cuatro de `/web/demo/*`, que solo alcanzan la
// conversación de quien las llama.
//
// Va en un archivo aparte y no en línea porque la política de contenido de
// Chasqui no admite `script` en línea. Es la misma defensa que protege al
// panel; la demostración no es motivo para relajarla.
//
// NADA SE PINTA CON `innerHTML`. Todo lo que viene del servidor entra por
// `textContent`, y las clases se ponen por propiedad. La primera versión de
// este archivo escapaba `<`, `>` y `&` a mano y metía el estado dentro de un
// atributo `class` — donde ese escape no sirve, porque no cubre las comillas.
// Funcionaba porque ese valor lo pone un enum de Rust; es decir, funcionaba por
// una razón que el archivo de al lado puede cambiar sin enterarse.

(() => {
  'use strict';

  const SITIO = location.origin;
  const hilo = document.getElementById('hilo');
  const panel = document.getElementById('panel');
  const forma = document.getElementById('forma');
  const entrada = document.getElementById('texto');
  const sugerencias = document.getElementById('sugerencias');

  let visitante = null;
  let ultimo = 0;
  let sondeando = false;
  // Lo último que pintó el lado del equipo. Sirve para NO repintar cuando no ha
  // cambiado nada: el sondeo va cada dos segundos, y repintar rehace el cuadro
  // de responder — con lo que el agente estaba escribiendo dentro. Se descubrió
  // usándolo: se teclea una respuesta y desaparece antes de poder enviarla.
  let ultimoPanel = '';

  const cabeceras = (json) => {
    const h = { 'x-chasqui-sitio': SITIO };
    if (visitante) h['x-chasqui-visitante'] = visitante;
    if (json) h['content-type'] = 'application/json';
    return h;
  };

  /** Un elemento con clase y texto. Nunca HTML. */
  function el(tag, clase, texto) {
    const n = document.createElement(tag);
    if (clase) n.className = clase;
    if (texto !== undefined && texto !== null) n.textContent = String(texto);
    return n;
  }

  function burbuja(de, texto) {
    const d = el('div', 'b ' + (de === 'visitante' ? 'suyo' : 'bot'));
    if (de !== 'visitante') d.appendChild(el('span', 'quien', 'Chasqui'));
    d.appendChild(document.createTextNode(texto));
    hilo.appendChild(d);
    hilo.scrollTop = hilo.scrollHeight;
  }

  async function abrir() {
    const r = await fetch('/web/conversacion', { method: 'POST', headers: cabeceras() });
    if (!r.ok) throw new Error('no se pudo abrir');
    visitante = (await r.json()).visitante;
  }

  async function enviar(texto) {
    if (!visitante) await abrir();
    await fetch('/web/mensaje', {
      method: 'POST', headers: cabeceras(true), body: JSON.stringify({ texto }),
    });
  }

  async function traerMensajes() {
    if (!visitante) return;
    const r = await fetch('/web/mensajes?desde=' + ultimo, { headers: cabeceras() });
    if (!r.ok) return;
    for (const m of await r.json()) {
      burbuja(m.de, m.texto);
      ultimo = Math.max(ultimo, m.id);
    }
  }

  // --- El lado del equipo -------------------------------------------------

  const MOTIVOS = {
    peticion: 'Pidió hablar con una persona',
    no_sabe: 'El bot no supo responder',
    no_soportado: 'Mandó algo que el bot no entiende',
    pedido_completo: 'Pedido terminado',
    pedido_incompleto: 'Pedido a medias',
    fallo_tecnico: 'Fallo técnico',
  };
  const ESTADOS = {
    bot: 'La atiende el bot',
    esperando_humano: 'Esperando a una persona',
    con_humano: 'La atiende una persona',
  };

  async function traerPanel() {
    if (!visitante) return;
    const r = await fetch('/web/demo/panel', { headers: cabeceras() });
    if (!r.ok) return;
    const texto = await r.text();
    if (texto === ultimoPanel) return;   // nada cambió: no se toca la pantalla
    ultimoPanel = texto;
    pintarPanel(JSON.parse(texto));
  }

  function pintarPanel(p) {
    // Y aun cuando SÍ hay que repintar —llegó un mensaje mientras el agente
    // escribía—, lo tecleado se conserva. Perder texto escrito por culpa de una
    // actualización es de las cosas que hacen desconfiar de una herramienta.
    const enCurso = document.getElementById('respuesta')?.value ?? '';
    panel.replaceChildren();

    // La clase del estado se elige de una lista CONOCIDA. Si el servidor
    // mandara algo inesperado, se queda sin color en vez de meterse en el
    // atributo.
    const clase = Object.hasOwn(ESTADOS, p.estado) ? p.estado : '';
    const linea = el('p');
    linea.appendChild(el('span', 'estado ' + clase, ESTADOS[p.estado] || p.estado));
    panel.appendChild(linea);
    panel.appendChild(el('p', 'pista', `Conversación #${p.conversacion_id} · ${p.mensajes} mensajes`));

    if (p.escalado) {
      const t = el('div', 'tarjeta alerta');
      t.appendChild(el('h3', null, MOTIVOS[p.escalado.motivo] || p.escalado.motivo));
      t.appendChild(el('p', null, p.escalado.detalle));
      const acciones = el('div', 'acciones');
      const b = el('button', null, 'Lo atiendo yo');
      b.onclick = async () => {
        b.disabled = true;
        await fetch('/web/demo/atender', { method: 'POST', headers: cabeceras() });
        await traerPanel();
      };
      acciones.appendChild(b);
      t.appendChild(acciones);
      panel.appendChild(t);
    }

    if (p.pedido) {
      const t = el('div', 'tarjeta');
      t.appendChild(el('h3', null, 'Pedido'));
      const dl = el('dl', 'campos');
      for (const [k, v] of p.pedido) {
        dl.appendChild(el('dt', null, k));
        dl.appendChild(el('dd', null, v));
      }
      t.appendChild(dl);
      panel.appendChild(t);
    }

    if (p.estado === 'con_humano') {
      const t = el('div', 'tarjeta');
      t.appendChild(el('h3', null, 'Responda usted, como su equipo'));
      t.appendChild(el('p', null,
        'Lo que escriba aquí le llega al cliente por su canal. Mientras tanto el bot calla.'));

      const f = document.createElement('form');
      const acciones = el('div', 'acciones');
      const campo = document.createElement('input');
      campo.type = 'text';
      campo.placeholder = 'Su respuesta…';
      campo.id = 'respuesta';
      campo.maxLength = 500;
      campo.style.flex = '1';
      campo.value = enCurso;
      const enviarB = el('button', null, 'Enviar');
      enviarB.type = 'submit';
      const devolver = el('button', 'tenue', 'Devolver al bot');
      devolver.type = 'button';
      devolver.onclick = async () => {
        await fetch('/web/demo/devolver', { method: 'POST', headers: cabeceras() });
        await traerPanel();
      };
      acciones.append(campo, enviarB, devolver);
      f.appendChild(acciones);
      f.onsubmit = async (e) => {
        e.preventDefault();
        const texto = campo.value.trim();
        if (!texto) return;
        campo.value = '';
        await fetch('/web/demo/responder', {
          method: 'POST', headers: cabeceras(true), body: JSON.stringify({ texto }),
        });
        await traerMensajes();
      };
      t.appendChild(f);
      panel.appendChild(t);
    } else if (!p.escalado) {
      panel.appendChild(el('p', 'pista',
        'Cuando el bot no sepa algo, o le pidan una persona, aquí aparecerá la tarjeta para que ' +
        'su equipo lo atienda. Pruebe a preguntar algo raro, o escriba «quiero hablar con una persona».'));
    }
  }

  // --- Sondeo -------------------------------------------------------------
  //
  // Cada dos segundos y las dos mitades a la vez: el visitante tiene que ver
  // aparecer su mensaje al otro lado sin recargar, que es todo el argumento.

  async function sondear() {
    if (sondeando) return;
    sondeando = true;
    try { await traerMensajes(); await traerPanel(); } catch (_) {} finally { sondeando = false; }
  }

  forma.onsubmit = async (e) => {
    e.preventDefault();
    const texto = entrada.value.trim();
    if (!texto) return;
    entrada.value = '';
    await enviar(texto);
    setTimeout(sondear, 400);
  };

  for (const s of ['hola', '¿cuánto cuesta?', 'cotizar', 'quiero hablar con una persona']) {
    const b = el('button', null, s);
    b.type = 'button';
    b.onclick = () => { entrada.value = s; forma.requestSubmit(); };
    sugerencias.appendChild(b);
  }

  abrir().then(sondear).catch(() => {
    panel.replaceChildren(el('p', 'pista', 'No se pudo abrir la conversación de demostración.'));
  });
  setInterval(sondear, 2000);
})();
