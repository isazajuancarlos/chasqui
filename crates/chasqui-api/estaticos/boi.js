// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza Arenas
//
// boi — comportamiento de cliente declarativo, sobre htmx, sin dependencias.
//
// El complemento de `maud` + htmx: maud teje el HTML en el servidor, htmx hace
// los intercambios parciales, y boi cubre lo que queda —la interacción que
// ocurre solo en el navegador—. Se marca el HTML con `data-*` y boi actúa; el
// HTML sigue siendo la única fuente de verdad.
//
// Cero dependencias, cero build: un módulo de JavaScript que el navegador
// entiende tal cual (`<script type="module" src="boi.js">`). Un tercero no puede
// comprometer lo que no existe — ese es el punto.
//
// Este archivo es el NÚCLEO: el registro de comportamientos, el escáner y el
// enganche al ciclo de htmx. NO trae comportamientos concretos (máscaras,
// validaciones): esos se añaden desde lo que cada proyecto pide de verdad, cada
// uno en su archivo, importando `registrar` de aquí.

// El catálogo de comportamientos registrados. Cada uno: un nombre único, un
// selector CSS y una función `init(elemento)` que se llama UNA vez por elemento
// que encaje. Se exporta para que las pruebas puedan vaciarlo entre casos.
export const comportamientos = [];

// El atributo con que se marca un elemento ya inicializado para un
// comportamiento. Es lo que hace el escáner idempotente: re-escanear un subárbol
// no vuelve a inicializar lo ya hecho. `boi-<nombre>-listo`.
export function marca(nombre) {
  return "boi-" + nombre + "-listo";
}

// Registra un comportamiento. Lo llama cada archivo de comportamiento al
// cargarse. Un nombre repetido es un error de programación —dos comportamientos
// peleando por el mismo—, no algo que se ignora en silencio (directiva 20).
export function registrar(nombre, selector, init) {
  if (comportamientos.some((c) => c.nombre === nombre)) {
    throw new Error("boi: el comportamiento «" + nombre + "» ya está registrado");
  }
  comportamientos.push({ nombre, selector, init });
}

// Aplica todos los comportamientos a los elementos NUEVOS de `raiz` (incluida
// `raiz` misma). Salta los ya marcados. `raiz` se inyecta como parámetro —no se
// toma de un global— para que sea probable con un doble de prueba.
//
// Un `init` que lanza NO tumba el escaneo del resto: se aísla y se avisa por
// consola. Un comportamiento roto en una pantalla no debe dejar muerta la página
// entera (mejora progresiva, principio 4).
export function escanear(raiz) {
  if (!raiz || typeof raiz.querySelectorAll !== "function") return;
  for (const c of comportamientos) {
    const encajan = [];
    // La propia raíz puede encajar el selector, no solo sus descendientes.
    if (typeof raiz.matches === "function" && raiz.matches(c.selector)) {
      encajan.push(raiz);
    }
    raiz.querySelectorAll(c.selector).forEach((el) => encajan.push(el));
    for (const el of encajan) {
      if (el.hasAttribute(marca(c.nombre))) continue; // ya inicializado
      el.setAttribute(marca(c.nombre), "");
      try {
        c.init(el);
      } catch (err) {
        // No relanzar: aislar el fallo a este elemento (principio 4).
        if (typeof console !== "undefined") {
          console.error("boi: el comportamiento «" + c.nombre + "» falló en", el, err);
        }
      }
    }
  }
}

// El enganche al mundo. Se llama una vez, con el `document` del navegador.
//
// Dos disparadores, y los dos importan:
//   - `DOMContentLoaded`: la carga inicial de la página.
//   - `htmx:load`: CADA fragmento que htmx trae por un intercambio parcial. Sin
//     esto, un fragmento nuevo llegaría SIN sus comportamientos —el fallo que más
//     se olvida (principio 3)—. htmx emite `htmx:load` sobre el nodo recién
//     insertado, y `event.detail.elt` es ese nodo: se escanea SOLO él.
export function enganchar(doc) {
  if (!doc || typeof doc.addEventListener !== "function") return;
  doc.addEventListener("DOMContentLoaded", () => escanear(doc.body));
  doc.addEventListener("htmx:load", (e) => {
    const nodo = e && e.detail && e.detail.elt ? e.detail.elt : doc.body;
    escanear(nodo);
  });
}

// En un navegador de verdad: publicar el núcleo en `globalThis.boi` (para que un
// `<script>` no-módulo pueda registrarse) y auto-enganchar. En deno/node no hay
// `document`, así que nada de esto corre y la prueba maneja su propio doble.
if (typeof document !== "undefined") {
  globalThis.boi = { registrar, escanear, comportamientos, marca };
  enganchar(document);
}
