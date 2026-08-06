// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
//
// Los comportamientos de cliente de Chasqui, sobre boi. Cada uno se marca con un
// `data-*` en el HTML que teje maud; boi los aplica al cargar y tras cada swap de
// htmx. Son los PRIMEROS comportamientos reales de boi: nacieron aquí porque el
// panel de chasqui los pidió de verdad, no por especulación (directiva 9).

import { registrar } from "./boi.js";

// Confirmación de doble pulsación para acciones irreversibles. Dos pulsaciones y
// no un `confirm()` del navegador: el borrado no se deshace y el diálogo nativo
// se acepta sin leerlo. El texto de aviso viaja en `data-confirmar`.
//   <button data-confirmar="¿Seguro? Pulse otra vez" hx-post="…">Olvidar…</button>
registrar("confirmar-doble", "[data-confirmar]", (el) => {
  const normal = el.textContent;
  const aviso = el.getAttribute("data-confirmar");
  let armado = false;
  let reloj = null;
  el.addEventListener("click", (e) => {
    if (!armado) {
      // Primera pulsación: NO deja que htmx dispare todavía.
      e.preventDefault();
      e.stopPropagation();
      armado = true;
      el.textContent = aviso;
      el.classList.add("armado");
      reloj = setTimeout(() => {
        armado = false;
        el.textContent = normal;
        el.classList.remove("armado");
      }, 5000);
      return;
    }
    // Segunda pulsación dentro de los 5 s: se deja pasar (htmx envía).
    clearTimeout(reloj);
    armado = false;
    el.textContent = normal;
    el.classList.remove("armado");
  });
});

// Lleva un hilo de mensajes al final al cargar y tras cada swap: quien atiende
// quiere ver lo último, no el saludo de hace una hora. htmx reemplaza el
// contenido interno en cada sondeo, así que el elemento marcado es NUEVO cada
// vez y boi lo reinicializa —justo lo que hace falta—. Sube por los ancestros
// hasta el primero que tenga barra de desplazamiento y lo lleva al final.
registrar("autoscroll", "[data-autoscroll]", (el) => {
  let c = el;
  while (c && c.scrollHeight <= c.clientHeight + 1) c = c.parentElement;
  if (c) c.scrollTop = c.scrollHeight;
});
