// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
//
// confirmar-doble — una acción irreversible pide DOS pulsaciones.
//
//     <button data-confirmar="¿Seguro? Pulse otra vez" hx-delete="/x">Borrar</button>
//
// Dos pulsaciones y no un `confirm()` del navegador: el diálogo nativo se acepta
// sin leerlo —es el mismo gesto que cerrar un aviso de cookies— y además bloquea
// el hilo. Aquí la confirmación ocurre en el sitio donde está la mano y con el
// texto de la acción concreta delante.
//
// ── POR QUÉ ESTE ARCHIVO EXISTE ──────────────────────────────────────────────
//
// Porque ya estaba escrito DOS veces. `chasqui` lo estrenó, `informes` lo copió
// —su propio comentario lo dice: «se copia y no se reescribe porque hoy no hay
// forma de compartir un comportamiento entre proyectos sin publicarlo»— y
// `medico` iba a ser la tercera. Sí hay forma: es la misma por la que viaja
// `boi.js`, vendorizado dentro de cada binario. Directiva 9.
//
// ── Y AL COMPARTIRLO SALIÓ UN DEFECTO QUE LAS DOS COPIAS TENÍAN ──────────────
//
// Las dos hacían `e.stopPropagation()` sobre el PROPIO botón, y registraban su
// oyente en ese mismo botón — que es donde htmx registra el suyo. Por
// especificación, `stopPropagation()` **no** detiene a los demás oyentes del
// mismo nodo: para eso está `stopImmediatePropagation()`. Y los oyentes de un
// mismo nodo corren en ORDEN DE REGISTRO, así que si htmx registraba primero, su
// manejador se ejecutaba igual: la petición salía en la PRIMERA pulsación y la
// confirmación no confirmaba nada. Sin error y sin aviso, con el botón cambiando
// de texto como si hubiera funcionado.
//
// **Qué está medido y qué no, para no afirmar de más**: lo comprobado es la
// regla del DOM —`stopPropagation` no corta a los hermanos del mismo nodo—, con
// la prueba `htmx NO dispara en la primera pulsación`, que se pone roja contra
// la versión que hoy corre en chasqui e informes (el mutante ya existía). Lo que
// NO se ha medido es si htmx, en un navegador real y con su orden de registro,
// llegaba a enviar: eso depende de sus internos. O sea que esto cierra una
// FRAGILIDAD demostrada, no un incidente observado.
//
// Aquí el oyente va en **fase de captura sobre el documento**, que corre antes
// que cualquier oyente del elemento destino sea cual sea el orden de registro, y
// corta con `stopImmediatePropagation()`. Es el arreglo en la REGLA y no en el
// caso (directiva 23), y es justo la clase de defecto que solo se arregla una
// vez cuando el código vive en un sitio.

import { registrar } from "../boi.js";

// Cuánto dura el «armado» antes de volver solo. Se puede ajustar por elemento
// con `data-confirmar-espera` (milisegundos); el defecto son 5 s, que es lo que
// llevaban las dos copias.
const ESPERA_POR_DEFECTO = 5000;

// El tiempo MÍNIMO que el botón está armado antes de que una segunda pulsación
// cuente. Sin esto, un doble clic atraviesa la barrera entera: medido en
// Chromium el 2026-08-12, dos pulsaciones a 8 ms una de otra ejecutaban la
// acción irreversible, con el texto de confirmación apareciendo y
// desapareciendo sin que nadie pueda leerlo. Es la forma más común de fallar de
// este patrón, y afectaba a los 27 usos vivos.
//
// 400 ms se elige contra el USUARIO LEGÍTIMO, no contra el atacante
// (directiva 21): un doble clic accidental cae por debajo de 300 ms, y leer
// «¿Seguro? Pulse otra vez» y volver a pulsar no baja de medio segundo. No es
// una barrera de seguridad —ésa está en el servidor—: es que la confirmación
// sea confirmación y no un trámite que el dedo se salta solo.
export const MINIMO_ARMADO = 400;

// Lo que hay que dejarle a una persona para reaccionar DESPUÉS del mínimo. La
// ventana útil no es `espera`, es `espera - MINIMO_ARMADO`, y esa resta es la
// que se me olvidó al poner el mínimo: con `data-confirmar-espera="450"` quedan
// 50 ms para confirmar, o sea que la acción legítima se vuelve **imposible** —
// exactamente el fallo que `leerEspera` dice impedir, movido de sitio.
const MARGEN_DE_REACCION = 1000;

// El techo, y no es un capricho: `setTimeout` TRUNCA el retardo a 32 bits, así
// que a partir de 2³¹ el reloj dispara de INMEDIATO. `data-confirmar-espera` con
// un número grande —la forma natural de escribir «que no caduque»— desarmaba el
// botón antes de que nadie pudiera confirmar, y la acción quedaba inalcanzable
// **para siempre y sin decir nada**. Medido en Chromium: 2147483647 no dispara
// en 500 ms; 2147483648 dispara a los 0 ms.
//
// Es la misma regresión que la revisión anterior encontró abajo, movida arriba:
// acoté un extremo y dejé el otro abierto. Y mi banco no podía verlo porque los
// ocho valores malos que escribí estaban TODOS por debajo (directiva 33: un
// banco de un solo lado sale verde con la regla buena y con una regla ciega).
export const ESPERA_MAXIMA = 2 ** 31 - 1;

// Tras confirmar, el MISMO elemento no se vuelve a armar durante este tiempo.
// Sin esto, el patrón se re-arma en el acto y machacar el botón ejecuta la
// acción irreversible una vez por ronda: medido, un clic cada 80 ms durante 2 s
// daba CUATRO borrados. En las tablas de medico htmx cambia el DOM bajo el
// cursor entre disparo y disparo, así que los sobrantes ni caen sobre la fila
// que el operador miraba.
//
// No estorba a nadie legítimo (directiva 21): borrar dos cosas son dos botones
// distintos —claves distintas, bloqueos distintos—, y repetir a propósito la
// MISMA acción irreversible un segundo después no es un gesto que exista.
export const BLOQUEO_TRAS_CONFIRMAR = 1000;

// La espera más corta que de verdad se puede usar: por debajo, la ventana útil
// (`espera - MINIMO_ARMADO`) no da tiempo a confirmar.
export const ESPERA_MINIMA_UTIL = MINIMO_ARMADO + MARGEN_DE_REACCION;

// Los elementos armados ahora mismo, con su reloj y su etiqueta original.
//
// Va en un `WeakMap` **por higiene, no porque baste**: aquí decía que un botón
// que htmx retira del DOM «no queda retenido por esta librería», y eso es FALSO
// mientras el reloj viva. El cierre del `setTimeout` captura `el` con
// referencia FUERTE, así que un botón armado y retirado sobrevive hasta que
// venza su espera. Medido con su pareja el 2026-08-13: 300 de 300 retenidos
// tras forzar GC, control 0.
//
// Con los 5 s por defecto es inocuo. Con una espera grande —`data-confirmar-espera`
// admite hasta `ESPERA_MAXIMA`— la retención dura lo que dure el reloj.
//
// Lo que lo cerraría de verdad es desarmar al retirarse (`AbortController`, o
// `htmx:beforeCleanupElement`), y eso es una decisión de alcance que no se toma
// dentro de un comentario. Queda dicho el límite real, que era lo urgente: un
// comentario que promete lo que no cumple es peor que no tenerlo.
const armados = new WeakMap();

// Cuándo confirmó por última vez cada elemento. Ver `BLOQUEO_TRAS_CONFIRMAR`.
const confirmados = new WeakMap();

// Lee `data-confirmar-espera` y AVISA de lo que no sirve, en vez de tragárselo.
//
// Antes, `Number(x) || DEFECTO` mandaba al defecto en silencio `"abc"`, `""` y
// `"0"` —eso solo se nota cuando alguien se pregunta por qué su ajuste no hace
// nada—, y dejaba pasar los que rompen: con `-1` o `0.5` el botón se desarma
// antes de que nadie pueda pulsar otra vez y **la acción se vuelve inalcanzable
// en silencio**, que es peor. El servidor sigue siendo la barrera, así que aquí
// se avisa y se SUJETA al mínimo utilizable —no al defecto— para conservar la
// intención de quien lo escribió (directiva 20: no hay dato válido, se dice).
function leerEspera(el) {
  const crudo = el.getAttribute("data-confirmar-espera");
  if (crudo === null) return ESPERA_POR_DEFECTO;

  const valor = Number(crudo);
  if (Number.isFinite(valor) && valor >= ESPERA_MINIMA_UTIL && valor <= ESPERA_MAXIMA) {
    return valor;
  }

  // Se SUJETA hacia arriba, nunca hacia abajo, y se dice. Alargar de más
  // molesta; acortar de menos deja un botón que no hace nada y nadie sabe por
  // qué. Entre las dos formas de equivocarse, solo una es recuperable.
  //
  // Y se sujeta en vez de caer al defecto porque conserva la intención de quien
  // escribió el atributo: `450` quería una ventana corta, y `1400` es la más
  // corta que de verdad se puede usar.
  if (typeof console !== "undefined") {
    console.error(
      "boi: data-confirmar-espera=«" + crudo + "» no sirve — tiene que estar entre " +
        ESPERA_MINIMA_UTIL + " y " + ESPERA_MAXIMA + " ms. Por debajo, la ventana útil " +
        "(`espera - " + MINIMO_ARMADO + "`) no da tiempo a confirmar; por encima, " +
        "`setTimeout` trunca a 32 bits y desarma en el acto. Se sujeta a " +
        ESPERA_MINIMA_UTIL + " ms.",
      el,
    );
  }
  return ESPERA_MINIMA_UTIL;
}

function desarmar(el) {
  const estado = armados.get(el);
  if (!estado) return;
  clearTimeout(estado.reloj);
  el.textContent = estado.etiqueta;
  el.classList.remove("armado");
  el.removeAttribute("aria-live");
  armados.delete(el);
}

// Los documentos que ya tienen el oyente. UNO por documento, no uno por botón.
//
// Antes se registraba dentro del `init`, o sea uno por elemento, y **nunca se
// retiraba**: cada botón que traía un swap de htmx dejaba un oyente permanente
// cuyo cierre retenía con fuerza al elemento: el `WeakMap` de arriba existe para
// no retener nada, y ese cierre lo anulaba. En un panel que se refresca por
// sondeo, los oyentes y los nodos muertos crecían sin techo — y todos corrían en
// cada clic de la página. Lo encontró la revisión de seguridad del 2026-08-12.
//
// Con el oyente en el documento, el botón se resuelve en el momento del clic con
// `closest`, así que no hay cierre sobre ningún elemento y no hay nada que
// retirar.
//
// ── EL ALCANCE CAMBIÓ, Y HAY QUE DECIRLO ─────────────────────────────────────
//
// Antes solo quedaban protegidos los botones que boi había INICIALIZADO. Ahora
// lo está cualquiera que case `[data-confirmar]` en el documento, lo haya
// escaneado boi o no. La dirección es la segura —se protege de más, nunca de
// menos— y de paso cierra un agujero que nadie había visto: un elemento que
// llega YA MARCADO con `boi-confirmar-doble-listo` —un `cloneNode`, o la caché
// de historial de htmx, que guarda el `innerHTML` con los atributos puestos—
// nunca recibía `init`, y con el oyente por elemento eso significaba CERO
// protección: la acción irreversible se ejecutaba a la primera pulsación.
//
// ── SOLO SOBRE ELEMENTOS HOJA ────────────────────────────────────────────────
//
// `data-confirmar` va en el `<button>`, no en un contenedor. `closest` sube por
// los ancestros, así que un clic en cualquier hijo de un contenedor marcado lo
// arma — y al armarse, `el.textContent = …` **borra su subárbol entero**. Con
// un botón hoja eso es justo lo que se quiere (cambiar su etiqueta); con un
// contenedor, se lleva por delante la pantalla. Los 27 usos vivos en medico,
// informes y chasqui son botones hoja; esto es para que el 28 también lo sea.
// La deduplicación va MARCADA EN EL DOM, no en un `WeakSet` de módulo.
//
// El `WeakSet` no cruza instancias: sirviendo `confirmar-doble.js` y
// `confirmar-doble.js?v=2` —una cadena rompe-cachés basta— el navegador evalúa
// el módulo DOS veces, cada copia estrena su propio `WeakSet` vacío, y el
// documento acaba con dos oyentes de captura. Los dos arman y desarman el mismo
// botón, así que la confirmación se vuelve casi imposible: medido en la revisión
// final, 6 clics a 700 ms → 0 acciones, y 8 clics a 1,3 s → 2 en vez de 4.
//
// El DOM sí es común a las dos copias. La marca en `documentElement` es el único
// estado que las dos ven.
//
// Se conserva el `WeakSet` como respaldo para documentos SIN raíz —los dobles de
// prueba, un `DocumentFragment`—: ahí no hay dónde marcar, y quedarse sin
// deduplicación ninguna sería peor que no cruzar instancias.
const MARCA_ENGANCHADO = "boi-confirmar-doble-enganchado";
const documentosEnganchados = new WeakSet();

function raizDe(doc) {
  const raiz = doc && doc.documentElement;
  return raiz && typeof raiz.setAttribute === "function" && typeof raiz.hasAttribute === "function"
    ? raiz
    : null;
}

function engancharDocumento(doc) {
  const raiz = raizDe(doc);
  if (raiz ? raiz.hasAttribute(MARCA_ENGANCHADO) : documentosEnganchados.has(doc)) return;
  if (raiz) raiz.setAttribute(MARCA_ENGANCHADO, "");
  documentosEnganchados.add(doc);

  // CAPTURA sobre el documento: corre antes que cualquier oyente del destino,
  // incluido el de htmx, sin depender del orden de registro. Ver la cabecera.
  doc.addEventListener(
    "click",
    (e) => {
      // ¿El clic cayó en un botón de confirmación, o dentro de uno? `closest`
      // incluye al propio destino, así que un clic en un hijo también arma.
      //
      // OJO, y aquí ponía «(o dentro de uno, si lleva un icono)» como si eso
      // estuviera soportado: **no lo está**. Al armar se hace
      // `el.textContent = …`, que borra el subárbol entero, y al desarmar solo
      // se devuelve TEXTO. Un botón con `<svg>` + `<span>` pierde el `<svg>`
      // PARA SIEMPRE — medido el 2026-08-13. Los 27 usos vivos son texto puro;
      // el `init` avisa por consola si encuentra hijos, para que el 28 no lo
      // descubra en producción.
      const destino = e.target;
      const el =
        destino && typeof destino.closest === "function"
          ? destino.closest("[data-confirmar]")
          : null;
      if (!el) return;

      const estado = armados.get(el);
      if (estado) {
        if (Date.now() - estado.armadoEn < MINIMO_ARMADO) {
          // Demasiado pronto: esto es el segundo golpe de un doble clic, no una
          // confirmación. Se corta igual que la primera y el botón SIGUE
          // armado, así que la pulsación deliberada que venga después sí pasa.
          e.preventDefault();
          e.stopImmediatePropagation();
          return;
        }
        // Segunda pulsación dentro de la ventana: se deja pasar TAL CUAL. No se
        // llama a `preventDefault` ni se corta nada, así que htmx envía su
        // petición como si boi no existiera — que es lo que tiene que pasar.
        confirmados.set(el, Date.now());
        desarmar(el);
        return;
      }

      // Acaba de confirmar: no se re-arma en caliente. Es la cola de un
      // machaqueo, no una segunda intención.
      //
      // El bloqueo se cuenta desde el ÚLTIMO clic, no desde la confirmación, y
      // esa diferencia es todo el arreglo: contándolo desde la confirmación,
      // machacar 2 s a 80 ms seguía dando DOS borrados —el bloqueo expiraba a
      // mitad de la ráfaga y el botón se re-armaba dentro de ella—. Contándolo
      // desde el último clic, la ráfaga se prolonga a sí misma y hace falta
      // SOLTAR de verdad: 2 s de machaqueo → una sola acción, medido.
      const ultimoBloqueo = confirmados.get(el);
      if (ultimoBloqueo !== undefined && Date.now() - ultimoBloqueo < BLOQUEO_TRAS_CONFIRMAR) {
        confirmados.set(el, Date.now());
        e.preventDefault();
        e.stopImmediatePropagation();
        return;
      }

      // Primera pulsación: no llega a nadie más.
      e.preventDefault();
      e.stopImmediatePropagation();

      const espera = leerEspera(el);
      const etiqueta = el.textContent;
      // `aria-live` para que quien usa lector de pantalla se entere de que el
      // botón cambió: sin esto, la primera pulsación no produce NADA audible y
      // parece que la aplicación se colgó.
      el.setAttribute("aria-live", "assertive");
      el.textContent = el.getAttribute("data-confirmar");
      el.classList.add("armado");
      armados.set(el, {
        etiqueta,
        armadoEn: Date.now(),
        reloj: setTimeout(() => desarmar(el), espera),
      });
    },
    true, // ← captura
  );

  // ── LA REPETICIÓN DEL TECLADO, QUE ATRAVESABA LA BARRERA ENTERA ────────────
  //
  // Mantener Enter pulsado sobre el botón —o sobre un campo del formulario que
  // lo contiene— produce `click` de CONFIANZA cada 15-40 ms, indistinguibles de
  // una pulsación buena: mismo `isTrusted`, mismo `detail: 0`. El manejador de
  // clic no puede separarlos, así que el mínimo de armado no cierra nada — solo
  // retrasa la ejecución hasta la primera repetición que caiga pasados los
  // 400 ms. Medido EN CHROMIUM con las cinco cadencias de repetición habituales
  // (X11, GNOME, macOS, Windows y la de accesibilidad): la acción salía en las
  // cinco, sin que el usuario soltara la tecla una sola vez.
  //
  // Se corta en `keydown`, que es donde SÍ se distingue: `e.repeat` lo dice.
  //
  // La barra espaciadora ya era segura —Chromium activa el botón en `keyup`—, y
  // aun así se cubría «porque el coste es cero». Se quitó: cinco líneas más
  // abajo el código hace `if (e.key !== "Enter") return;`. Aquí quedaba el
  // comentario que decía lo contrario, contradiciendo al de al lado dentro del
  // mismo bloque.
  doc.addEventListener(
    "keydown",
    (e) => {
      if (!e.repeat) return;
      // SOLO Enter. La barra espaciadora estaba aquí «porque el coste es cero»,
      // y el coste NO era cero: cortarla impide teclear espacios en cualquier
      // campo del formulario —incluidos los de contraseña de informes y medico,
      // donde el usuario no ve lo que escribió—. Y no compraba nada: medido con
      // el mutante, mantener espacio sobre el botón ejecuta CERO acciones con la
      // guarda y CERO sin ella, porque Chromium activa el botón en `keyup`.
      // Pagar una regresión real por una protección de valor medido cero.
      if (e.key !== "Enter") return;
      const destino = e.target;
      if (!destino || typeof destino.closest !== "function") return;
      // Un campo de texto largo usa Enter para ESCRIBIR: ahí la repetición es
      // legítima y cortarla rompería la escritura de quien redacta una nota
      // dentro de un formulario que, más abajo, tiene un botón de confirmar
      // (directiva 21: la regla se prueba contra el usuario legítimo).
      // `isContentEditable` es la PROPIEDAD del DOM, no una enumeración de
      // cómo se escribió el atributo: el selector fallaba con
      // `contenteditable="TRUE"` —HTML no distingue mayúsculas, el selector
      // sí— y con `plaintext-only`, que es un campo de texto largo de pleno
      // derecho (directiva 23: la regla, no el caso).
      if (destino.closest("textarea") || destino.isContentEditable) return;

      const boton = destino.closest("[data-confirmar]");
      const formulario = destino.closest("form");
      const enviaríaUnConfirmable =
        formulario && typeof formulario.querySelector === "function"
          ? formulario.querySelector("[data-confirmar]")
          : null;
      if (boton || enviaríaUnConfirmable) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
    },
    true, // ← captura, igual que el clic
  );
}

// EL OYENTE NO DEPENDE DEL ESCÁNER, y esa es la corrección de fondo.
//
// Colgarlo solo del `init` dejaba la barrera atada a que algún `[data-confirmar]`
// pasara por el escáner — y el escáner SALTA el `init` de todo elemento que ya
// venga marcado. La caché de historial de htmx produce justo ese estado: guarda
// el `innerHTML` **con la marca puesta**, así que al volver con el botón Atrás
// el botón renace marcado; si es el único de la página, no hay `init`, no hay
// oyente, y **la acción irreversible se ejecuta a la PRIMERA pulsación**. Medido
// de extremo a extremo con htmx real el 2026-08-12, con su pareja de control.
//
// Hoy no muerde —ninguno de los cinco consumidores usa `hx-boost` ni
// `hx-push-url`—, y por eso mismo se arregla ahora: el día que alguien los
// añada, esto se cae sin un solo error en consola.
// Los ÚNICOS disparadores de htmx que esta barrera puede interceptar.
//
// La barrera vive en un oyente de captura de `click`. Si el elemento pide a htmx
// que dispare con otro evento —`mousedown`, `pointerdown`, `mouseup`,
// `dblclick`, `keyup`, `every 2s`…— htmx lanza la petición SIN pasar por ese
// oyente: un gesto, una acción irreversible, y encima el botón se arma después
// como si la confirmación hubiera funcionado. Medido en la revisión final.
//
// `submit` entra porque el submit nativo del formulario sí produce el `click`
// del botón antes de enviarse.
const DISPARADORES_CUBIERTOS = new Set(["click", "submit"]);

// Saca el nombre del evento de cada cláusula de un `hx-trigger`.
// «keyup changed delay:500ms, click once» → ["keyup", "click"].
export function _eventosDeTrigger(valor) {
  return String(valor)
    .split(",")
    .map((clausula) => clausula.trim().split(/\s+/)[0].toLowerCase())
    .filter((e) => e.length > 0);
}

registrar("confirmar-doble", "[data-confirmar]", (el) => {
  // La restricción vivía SOLO en prosa; esto la convierte en código (directiva
  // 6: hook que impide > prueba que falla > directiva que recordar). No se
  // bloquea el elemento —bloquearlo dejaría una pantalla muerta por un atributo
  // que quizá el autor sí quería—, se DICE, que es lo que la directiva 20 pide
  // cuando el camino permisivo lo tiene que pedir el llamante.
  if (typeof el.getAttribute === "function") {
    const trigger = el.getAttribute("hx-trigger");
    if (trigger) {
      const fuera = _eventosDeTrigger(trigger).filter((e) => !DISPARADORES_CUBIERTOS.has(e));
      if (fuera.length > 0) {
        console.warn(
          "boi/confirmar-doble: este [data-confirmar] usa hx-trigger «" + trigger +
            "» y la confirmación NO cubre " + fuera.join(", ") +
            ": htmx dispararía la acción irreversible sin pasar por la barrera. " +
            "Solo se cubren " + [...DISPARADORES_CUBIERTOS].join(" y ") + ".",
          el,
        );
      }
    }
  }

  // Y el aviso de los hijos: armar hace `textContent = …`, que borra el subárbol
  // y no vuelve. Un botón con icono lo pierde para siempre.
  if (el.children && el.children.length > 0) {
    console.warn(
      "boi/confirmar-doble: este [data-confirmar] tiene " + el.children.length +
        " hijo(s) y al armarse los PIERDE para siempre (`textContent` borra el " +
        "subárbol y la restauración solo devuelve texto). Use un botón de texto puro.",
      el,
    );
  }

  // El `init` sigue existiendo para documentos que no son el principal (un
  // iframe, una ventana propia), y porque es lo que deja la marca que hace
  // observable el comportamiento desde un E2E.
  const doc = el.ownerDocument;
  if (!doc || typeof doc.addEventListener !== "function") return;
  engancharDocumento(doc);
});

// EL ENGANCHE DEL DOCUMENTO VA DESPUÉS DEL `registrar`, y el orden es la mitad
// del arreglo del 2026-08-13.
//
// Estaba antes, y entonces una segunda copia del módulo instalaba su oyente y
// SOLO DESPUÉS tropezaba con «ya está registrado». La guarda existía y llegaba
// tarde: el daño —el segundo oyente de captura— ya estaba hecho, y encima el
// `throw` dejaba el módulo a medio evaluar.
//
// Ahora `registrar` lanza primero y esta línea no llega a correr. La marca en el
// DOM es la segunda capa, para el caso en que alguien registre con otro nombre.
if (typeof document !== "undefined") engancharDocumento(document);

// Se exportan solo para las pruebas: dejar el estado limpio entre casos, y
// poder comprobar la deduplicación entre instancias sin un navegador.
export const _engancharDocumento = engancharDocumento;
export const _MARCA_ENGANCHADO = MARCA_ENGANCHADO;
export const _armados = armados;
export const _desarmar = desarmar;
