// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza Arenas
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

// Los elementos armados ahora mismo, con su reloj y su etiqueta original.
// Va en un `WeakMap` para que un botón que htmx retira del DOM no quede
// retenido por esta librería.
const armados = new WeakMap();

function desarmar(el) {
  const estado = armados.get(el);
  if (!estado) return;
  clearTimeout(estado.reloj);
  el.textContent = estado.etiqueta;
  el.classList.remove("armado");
  el.removeAttribute("aria-live");
  armados.delete(el);
}

registrar("confirmar-doble", "[data-confirmar]", (el) => {
  const doc = el.ownerDocument;
  if (!doc || typeof doc.addEventListener !== "function") return;

  // CAPTURA sobre el documento: corre antes que cualquier oyente del destino,
  // incluido el de htmx, sin depender del orden de registro. Ver la cabecera.
  doc.addEventListener(
    "click",
    (e) => {
      // ¿El clic fue en ESTE botón (o dentro de él, si lleva un icono)?
      const destino = e.target;
      const dentro =
        destino === el ||
        (destino && typeof destino.closest === "function" && destino.closest("[data-confirmar]") === el);
      if (!dentro) return;

      if (armados.has(el)) {
        // Segunda pulsación dentro de la ventana: se deja pasar TAL CUAL. No se
        // llama a `preventDefault` ni se corta nada, así que htmx envía su
        // petición como si boi no existiera — que es lo que tiene que pasar.
        desarmar(el);
        return;
      }

      // Primera pulsación: no llega a nadie más.
      e.preventDefault();
      e.stopImmediatePropagation();

      const espera = Number(el.getAttribute("data-confirmar-espera")) || ESPERA_POR_DEFECTO;
      const etiqueta = el.textContent;
      // `aria-live` para que quien usa lector de pantalla se entere de que el
      // botón cambió: sin esto, la primera pulsación no produce NADA audible y
      // parece que la aplicación se colgó.
      el.setAttribute("aria-live", "assertive");
      el.textContent = el.getAttribute("data-confirmar");
      el.classList.add("armado");
      armados.set(el, {
        etiqueta,
        reloj: setTimeout(() => desarmar(el), espera),
      });
    },
    true, // ← captura
  );
});

// Se exporta solo para las pruebas: deja el estado limpio entre casos.
export const _armados = armados;
export const _desarmar = desarmar;
